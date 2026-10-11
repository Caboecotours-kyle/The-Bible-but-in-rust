#!/usr/bin/env bash
# Sign and notarize the macOS download. Run on the Mac that holds the Developer ID
# certificate, so the signing key never leaves it.
#
# usage: tools/sign_macos.sh <unsigned Bible-macos.dmg> <signed output .dmg>
#
# Expects, on that Mac:
#   ~/.appstoreconnect/developer-id/developer_id.p12, p12_password.txt, identity.txt
#   ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH in the environment (App Store Connect API key)
set -euo pipefail

IN=$1
OUT=$2
STORE="$HOME/.appstoreconnect/developer-id"
IDENTITY=$(cat "$STORE/identity.txt")
WORK=$(mktemp -d "${SIGN_TMPDIR:-${TMPDIR:-/tmp}}/bible-sign.XXXXXX")
KEYCHAIN="$WORK/signing.keychain-db"
KEYCHAIN_PASSWORD=$(uuidgen)
ORIGINAL_KEYCHAINS=()
while IFS= read -r line; do ORIGINAL_KEYCHAINS+=("$(echo "$line" | sed -E 's/^ *"(.*)"$/\1/')"); done < <(security list-keychains -d user)

cleanup() {
  hdiutil detach "$WORK/mount" >/dev/null 2>&1 || true
  security list-keychains -d user -s "${ORIGINAL_KEYCHAINS[@]}"
  security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# A throwaway keychain, so nothing is added to the login keychain.
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 1800 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$STORE/developer_id.p12" -P "$(cat "$STORE/p12_password.txt")" -A -t cert -f pkcs12 -k "$KEYCHAIN" >/dev/null
security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" "${ORIGINAL_KEYCHAINS[@]}"

mkdir "$WORK/mount" "$WORK/stage"
hdiutil attach "$IN" -nobrowse -readonly -mountpoint "$WORK/mount" >/dev/null
cp -R "$WORK/mount/Bible.app" "$WORK/stage/"
hdiutil detach "$WORK/mount" >/dev/null

codesign --force --options runtime --timestamp --sign "$IDENTITY" --keychain "$KEYCHAIN" "$WORK/stage/Bible.app"
codesign --verify --strict "$WORK/stage/Bible.app"
# The signed program must still start; this loads all three translations and exits.
BIBLE_TIMING=1 "$WORK/stage/Bible.app/Contents/MacOS/Bible"

ln -s /Applications "$WORK/stage/Applications"
rm -f "$OUT"
hdiutil create -volname Bible -srcfolder "$WORK/stage" -ov -format UDZO "$OUT" >/dev/null
codesign --force --timestamp --sign "$IDENTITY" --keychain "$KEYCHAIN" "$OUT"

xcrun notarytool submit "$OUT" --key "${ASC_KEY_PATH/#\~/$HOME}" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" --wait
xcrun stapler staple "$OUT"
spctl --assess --type open --context context:primary-signature --verbose "$OUT"
