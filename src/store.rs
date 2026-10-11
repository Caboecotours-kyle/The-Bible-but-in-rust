//! What the reader left behind: place, settings, bookmarks and highlights.
//! Kept as a small text file under the user's data directory.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Book index, chapter number, verse number: the same in every translation.
pub type VerseKey = (u8, u8, u8);

pub struct Store {
    path: Option<PathBuf>,
    pub translation: String,
    pub book: usize,
    pub chapter: usize,
    pub red_letters: bool,
    pub notes: bool,
    pub dark: bool,
    pub size: f32,
    pub bookmarks: BTreeSet<VerseKey>,
    pub highlights: BTreeSet<VerseKey>,
}

/// Where this system keeps an application's own files.
fn data_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        var("APPDATA").map(|dir| dir.join("Bible"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support/Bible"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local/share"))).map(|dir| dir.join("bible"))
    }
}

pub const SIZES: std::ops::RangeInclusive<f32> = 15.0..=32.0;

impl Store {
    pub fn load() -> Self {
        Self::load_from(data_dir().map(|dir| dir.join("state.tsv")))
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut store = Store {
            path,
            translation: "KJV".into(),
            book: 0,
            chapter: 1,
            red_letters: true,
            notes: true,
            dark: false,
            size: 21.0,
            bookmarks: BTreeSet::new(),
            highlights: BTreeSet::new(),
        };
        let saved = store.path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        for line in saved.as_deref().unwrap_or("").lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            let number = |i: usize| fields.get(i).and_then(|f| f.parse::<u8>().ok());
            let key = || Some((number(1)?, number(2)?, number(3)?));
            match fields[0] {
                "translation" => store.translation = fields.get(1).unwrap_or(&"KJV").to_string(),
                "at" => {
                    if let (Some(book), Some(chapter)) = (number(1), number(2)) {
                        store.book = book as usize;
                        store.chapter = chapter as usize;
                    }
                }
                "red" => store.red_letters = fields.get(1) != Some(&"0"),
                "notes" => store.notes = fields.get(1) != Some(&"0"),
                "dark" => store.dark = fields.get(1) == Some(&"1"),
                "size" => {
                    if let Some(size) = fields.get(1).and_then(|f| f.parse::<f32>().ok()).filter(|size| size.is_finite()) {
                        store.size = size.clamp(*SIZES.start(), *SIZES.end());
                    }
                }
                "bookmark" => store.bookmarks.extend(key()),
                "highlight" => store.highlights.extend(key()),
                _ => {}
            }
        }
        store
    }

    /// Write everything out. The file is replaced in one step so a crash cannot leave half of it.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let mut out = format!(
            "translation\t{}\nat\t{}\t{}\nred\t{}\nnotes\t{}\ndark\t{}\nsize\t{}\n",
            self.translation,
            self.book,
            self.chapter,
            self.red_letters as u8,
            self.notes as u8,
            self.dark as u8,
            self.size
        );
        for (name, set) in [("bookmark", &self.bookmarks), ("highlight", &self.highlights)] {
            for (book, chapter, verse) in set {
                out += &format!("{name}\t{book}\t{chapter}\t{verse}\n");
            }
        }
        let tmp = path.with_extension("tmp");
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&tmp, out))
            .and_then(|_| std::fs::rename(&tmp, path));
        if let Err(error) = written {
            eprintln!("bible: could not save {}: {error}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("bible-store-test-{}/state.tsv", std::process::id()));
        let mut store = Store::load_from(Some(path.clone()));
        assert_eq!((store.translation.as_str(), store.book, store.chapter), ("KJV", 0, 1));
        store.translation = "BSB".into();
        (store.book, store.chapter) = (42, 3);
        store.dark = true;
        store.red_letters = false;
        store.size = 24.0;
        store.bookmarks.insert((42, 3, 16));
        store.highlights.insert((18, 23, 1));
        store.save();

        let again = Store::load_from(Some(path.clone()));
        assert_eq!((again.translation.as_str(), again.book, again.chapter), ("BSB", 42, 3));
        assert!(again.dark && !again.red_letters && again.notes);
        assert_eq!(again.size, 24.0);
        assert!(again.bookmarks.contains(&(42, 3, 16)) && again.highlights.contains(&(18, 23, 1)));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
