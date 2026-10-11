#!/usr/bin/env bash
# Turn the two macOS builds into one Bible.app inside Bible-macos.dmg.
# The result is not yet signed for distribution: tools/sign_macos.sh does that on a
# Mac that holds the Developer ID certificate, so no signing key leaves that Mac.
set -euo pipefail

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP=dist/Bible.app
rm -rf dist Bible-macos.dmg
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# The icon, in every size macOS asks for.
ICONSET=dist/Bible.iconset
mkdir "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size assets/icon/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) assets/icon/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Bible.icns"
rm -rf "$ICONSET"

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
  <key>CFBundleIconFile</key><string>Bible</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID:-com.kylepopp.bible}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.reference</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# A placeholder signature; tools/sign_macos.sh applies the real one.
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"
# The signed program must still start; this loads all three translations and exits.
BIBLE_TIMING=1 "$APP/Contents/MacOS/Bible"

ln -s /Applications dist/Applications
hdiutil create -volname Bible -srcfolder dist -ov -format UDZO Bible-macos.dmg
