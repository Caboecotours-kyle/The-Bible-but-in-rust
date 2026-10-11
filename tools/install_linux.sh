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
# The launcher names the program by its full path, so it works whether or not ~/.local/bin
# is on PATH. The path is quoted the way desktop entries require, so a home folder with
# spaces or symbols in its name still works.
quoted="$BIN/bible"
quoted=${quoted//\\/\\\\}
quoted=${quoted//\"/\\\"}
quoted=${quoted//\$/\\\$}
quoted=${quoted//\`/\\\`}
# Desktop entries decode backslashes once more before reading the quotes.
quoted=${quoted//\\/\\\\}
{
  grep -v '^Exec=' bible.desktop
  printf 'Exec="%s"\n' "$quoted"
} > "$APPS/bible.desktop"
echo "Installed. Open Bible from your launcher, or run $BIN/bible"
