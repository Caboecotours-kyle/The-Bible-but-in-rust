#!/usr/bin/env bash
# Install Bible for the current user: the program, its icon and a launcher entry.
set -euo pipefail
cd "$(dirname "$0")"

BIN="$HOME/.local/bin"
ICONS="$HOME/.local/share/icons/hicolor/512x512/apps"
APPS="$HOME/.local/share/applications"
mkdir -p "$BIN" "$ICONS" "$APPS"
install -m 755 bible "$BIN/bible"
install -m 644 icon.png "$ICONS/bible.png"
# The launcher names the program by its full path, so it works whether or not ~/.local/bin is on PATH.
sed "s|^Exec=.*|Exec=$BIN/bible|" bible.desktop > "$APPS/bible.desktop"
echo "Installed. Open Bible from your launcher, or run $BIN/bible"
