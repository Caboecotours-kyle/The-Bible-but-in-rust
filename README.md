# Bible

A desktop Bible reader written in Rust with GPUI. Three public-domain translations are built into the binary: the King James Version, the Berean Standard Bible and the World English Bible.

## Download

Builds for macOS, Windows and Linux are on the [releases page](../../releases). The macOS build is signed and notarized. The Windows build is not signed yet, so Windows asks you to choose More info, then Run anyway.

## Speed

Measured with the release build on a 2020 laptop (Intel Core i7-10875H, Linux).

| | |
|---|---|
| Open a translation (31,000 verses, with red letters and notes) | 31 ms |
| Search the whole Bible for a phrase | 1.2 ms |
| Search for "the", which matches 27,000 verses | 11 ms |
| Launch to window on screen | 0.76 s |
| Memory with one translation open | 100 MB |
| Memory with all three open | 133 MB |
| Download | 10 to 19 MB |

There is no database and nothing to index. Each translation is compiled into the program and kept in memory as one block of text, and a search is a single scan of it, fast enough to rerun on every keystroke. Only the chapter on screen is laid out.

`BIBLE_TIMING=1 cargo run --release` prints the load and search times on your machine.

## Run from source

```
cargo run --release
```

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
