#!/usr/bin/env bash
# Turn the two macOS builds into one Bible.app inside Bible-macos.dmg.
# Signs and notarizes when the Apple credentials are present; otherwise the app is
# ad-hoc signed and macOS asks for a right-click, Open on first launch.
set -euo pipefail

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP=dist/Bible.app
rm -rf dist Bible-macos.dmg
mkdir -p "$APP/Contents/MacOS"
lipo -create -output "$APP/Contents/MacOS/Bible" \
  target/aarch64-apple-darwin/release/bible target/x86_64-apple-darwin/release/bible

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Bible</string>
  <key>CFBundleDisplayName</key><string>Bible</string>
  <key>CFBundleExecutable</key><string>Bible</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID:-dev.biblereader.Bible}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.reference</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

if [[ -n "${MACOS_CERTIFICATE_P12:-}" ]]; then
  KEYCHAIN="$RUNNER_TEMP/signing.keychain-db"
  KEYCHAIN_PASSWORD=$(uuidgen)
  echo "$MACOS_CERTIFICATE_P12" | base64 --decode > "$RUNNER_TEMP/certificate.p12"
  security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  security set-keychain-settings -lut 3600 "$KEYCHAIN"
  security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  security import "$RUNNER_TEMP/certificate.p12" -P "$MACOS_CERTIFICATE_PASSWORD" -A -t cert -f pkcs12 -k "$KEYCHAIN"
  security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
  security list-keychains -d user -s "$KEYCHAIN" login.keychain-db
  codesign --force --options runtime --timestamp --sign "$MACOS_SIGNING_IDENTITY" "$APP"
else
  echo "No signing certificate configured: ad-hoc signing."
  codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

ln -s /Applications dist/Applications
hdiutil create -volname Bible -srcfolder dist -ov -format UDZO Bible-macos.dmg

if [[ -n "${MACOS_CERTIFICATE_P12:-}" && -n "${ASC_KEY_P8:-}" ]]; then
  codesign --force --timestamp --sign "$MACOS_SIGNING_IDENTITY" Bible-macos.dmg
  echo "$ASC_KEY_P8" | base64 --decode > "$RUNNER_TEMP/asc_key.p8"
  xcrun notarytool submit Bible-macos.dmg --key "$RUNNER_TEMP/asc_key.p8" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" --wait
  xcrun stapler staple Bible-macos.dmg
  rm -f "$RUNNER_TEMP/asc_key.p8" "$RUNNER_TEMP/certificate.p12"
fi
