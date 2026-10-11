# Bible

A desktop Bible reader written in Rust with GPUI. Three public-domain translations are built into the binary: the King James Version, the Berean Standard Bible and the World English Bible.

## Download

Builds for macOS, Windows and Linux are on the [releases page](../../releases). The macOS build is signed and notarized. The Windows build is not signed yet, so Windows asks you to choose More info, then Run anyway.

## Run from source

```
cargo run --release
```

`BIBLE_TIMING=1 cargo run --release` prints load and search times and exits.

## Keys

| Key | Does |
|---|---|
| `/` or Ctrl+F | Search, or go to a reference such as `john 3:16`, `ps 23`, `1 cor 13` |
| Up, Down, Enter | Move through results and open one; after Enter the arrows open the next result |
| Esc | Clear the search, then leave it; or deselect a verse |
| Left, Right | Previous and next chapter |
| T | Next translation |
| R, N, D | Red letters, margin notes, dark theme |
| Ctrl+= and Ctrl+- | Text size |
| C, H, B | With a verse selected: copy, highlight, bookmark |

Place, settings, bookmarks and highlights are saved in `state.tsv` under `~/.local/share/bible` on Linux, `~/Library/Application Support/Bible` on macOS and `%APPDATA%\Bible` on Windows.

## Text

`tools/build_text.py` converts USFM source files into `assets/*.tsv`, which the app embeds. The record format is described at the top of that script.

| Translation | Source |
|---|---|
| KJV (1769 text) | https://ebible.org/Scriptures/eng-kjv_usfm.zip |
| WEB | https://ebible.org/Scriptures/engwebp_usfm.zip |
| BSB | https://bereanbible.com/bsb_usfm.zip |

Each converted text was compared verse by verse with a second published edition of the same translation (the eBible verse-per-line files for KJV and WEB, `bereanbible.com/bsb.txt` for BSB). The KJV was also compared letter by letter with two independently maintained copies.

## Layout

- `src/bible.rs` loads a translation, resolves references and searches.
- `src/store.rs` saves and restores state.
- `src/ui.rs` is the window.
- `vendor/gpui` is GPUI 0.2.2 with one change, in `src/text_system/line_wrapper.rs`: punctuation and verse numbers stay attached to their word when a line wraps.
- `Cargo.lock` pins `libc` to 0.2.189, the last release an older GPUI dependency compiles against.

The typeface is EB Garamond, under the SIL Open Font License (`assets/fonts/OFL.txt`).
