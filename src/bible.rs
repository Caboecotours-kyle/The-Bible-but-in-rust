//! The text model: loading a translation, finding references and searching.
//!
//! Each translation is embedded in the binary in the format written by
//! `tools/build_text.py` and parsed once, on first use, into flat arrays.

use memchr::memmem;
use std::ops::Range;
use std::sync::OnceLock;

pub const ITALIC: u8 = 1;
pub const NAME: u8 = 2;
pub const RED: u8 = 4;

/// A run of styled text; offsets are into [`Translation::text`].
pub struct Span {
    pub range: Range<u32>,
    pub flags: u8,
}

/// A stretch of one verse that sits on one line or in one paragraph.
pub struct Piece {
    pub verse: u32,
    /// The first piece of a verse carries its number.
    pub numbered: bool,
    pub text: Range<u32>,
    pub spans: Range<u32>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Head {
    Major,
    Section,
    Reference,
    Title,
    Minor,
}

pub enum Block {
    Head(Head, String),
    Gap,
    /// `kind` is the break code from the text file: p m 1 2 3 c.
    Para { kind: u8, pieces: Range<u32> },
}

pub struct Note {
    pub block: u32,
    pub verse: u8,
    pub text: &'static str,
}

pub struct Verse {
    pub book: u8,
    pub chapter: u8,
    pub number: u8,
    pub text: Range<u32>,
    pub block: u32,
}

pub struct Chapter {
    pub blocks: Range<u32>,
    pub verses: Range<u32>,
    pub notes: Range<u32>,
}

pub struct Book {
    pub name: &'static str,
    pub chapters: Vec<Chapter>,
}

pub struct Translation {
    pub id: &'static str,
    /// Verse text with markup removed. Verses are separated by newlines.
    pub text: String,
    lower: String,
    curly: bool,
    pub books: Vec<Book>,
    pub verses: Vec<Verse>,
    pub blocks: Vec<Block>,
    pub pieces: Vec<Piece>,
    pub spans: Vec<Span>,
    pub notes: Vec<Note>,
}

struct Source {
    id: &'static str,
    data: &'static str,
    cell: OnceLock<Translation>,
}

static SOURCES: [Source; 3] = [
    Source {
        id: "KJV",
        data: include_str!("../assets/kjv.tsv"),
        cell: OnceLock::new(),
    },
    Source {
        id: "BSB",
        data: include_str!("../assets/bsb.tsv"),
        cell: OnceLock::new(),
    },
    Source {
        id: "WEB",
        data: include_str!("../assets/web.tsv"),
        cell: OnceLock::new(),
    },
];

pub fn translation_ids() -> impl Iterator<Item = &'static str> {
    SOURCES.iter().map(|s| s.id)
}

/// The translation with this id, parsed on first use. Unknown ids give the first one.
pub fn translation(id: &str) -> &'static Translation {
    let source = SOURCES.iter().find(|s| s.id == id).unwrap_or(&SOURCES[0]);
    source
        .cell
        .get_or_init(|| Translation::parse(source.id, source.data))
}

impl Translation {
    fn parse(id: &'static str, data: &'static str) -> Self {
        let mut t = Translation {
            id,
            text: String::with_capacity(data.len()),
            lower: String::new(),
            curly: false,
            books: Vec::with_capacity(66),
            verses: Vec::with_capacity(31_200),
            blocks: Vec::new(),
            pieces: Vec::new(),
            spans: Vec::new(),
            notes: Vec::new(),
        };
        // The paragraph that a run-on piece would join, if one is open.
        let mut open: Option<usize> = None;
        // A verse whose number has not been shown yet because its text starts on a later line.
        let mut unnumbered: Option<u32> = None;
        // Notes on a title, waiting for the paragraph that follows it.
        let mut waiting_notes: Vec<usize> = Vec::new();

        for line in data.lines() {
            let mut fields = line.split('\t');
            let kind = fields.next().unwrap_or("");
            match kind {
                "B" | "C" => {
                    t.flush_unnumbered(&mut unnumbered, &mut open, &mut waiting_notes);
                    t.close_chapter();
                    open = None;
                    if kind == "B" {
                        t.books.push(Book {
                            name: fields.nth(1).unwrap(),
                            chapters: Vec::new(),
                        });
                    }
                    if kind == "C" {
                        let (blocks, verses, notes) =
                            (t.blocks.len() as u32, t.verses.len() as u32, t.notes.len() as u32);
                        t.books.last_mut().unwrap().chapters.push(Chapter {
                            blocks: blocks..blocks,
                            verses: verses..verses,
                            notes: notes..notes,
                        });
                    }
                }
                "M" | "S" | "R" | "D" | "H" => {
                    let head = match kind {
                        "M" => Head::Major,
                        "S" => Head::Section,
                        "R" => Head::Reference,
                        "D" => Head::Title,
                        _ => Head::Minor,
                    };
                    t.blocks.push(Block::Head(head, strip_tags(fields.next().unwrap())));
                    open = None;
                }
                "K" => {
                    if !matches!(t.blocks.last(), Some(Block::Gap)) {
                        t.blocks.push(Block::Gap);
                    }
                    open = None;
                }
                "V" => {
                    t.flush_unnumbered(&mut unnumbered, &mut open, &mut waiting_notes);
                    let number: u8 = fields.next().unwrap().parse().unwrap();
                    let brk = fields.next().unwrap().as_bytes()[0];
                    let body = fields.next().unwrap_or("");
                    if !t.text.is_empty() {
                        t.text.push('\n');
                    }
                    let at = t.text.len() as u32;
                    let verse = t.verses.len() as u32;
                    t.verses.push(Verse {
                        book: (t.books.len() - 1) as u8,
                        chapter: t.books.last().unwrap().chapters.len() as u8,
                        number,
                        text: at..at,
                        block: t.blocks.len() as u32,
                    });
                    if body.is_empty() {
                        unnumbered = Some(verse);
                    } else {
                        t.push_piece(verse, true, brk, body, &mut open, &mut waiting_notes);
                    }
                }
                "+" => {
                    let brk = fields.next().unwrap().as_bytes()[0];
                    let body = fields.next().unwrap();
                    let Some(verse) = t.verses.len().checked_sub(1) else { continue };
                    let verse = verse as u32;
                    let numbered = unnumbered.take() == Some(verse);
                    if !numbered {
                        t.text.push(' ');
                    }
                    t.push_piece(verse, numbered, brk, body, &mut open, &mut waiting_notes);
                }
                "N" => {
                    let verse: u8 = fields.next().unwrap().parse().unwrap();
                    let text = fields.next().unwrap();
                    let block = open.unwrap_or(t.blocks.len()) as u32;
                    if open.is_none() {
                        waiting_notes.push(t.notes.len());
                    }
                    t.notes.push(Note { block, verse, text });
                }
                _ => {}
            }
        }
        t.flush_unnumbered(&mut unnumbered, &mut open, &mut waiting_notes);
        t.close_chapter();
        t.curly = t.text.contains('’');
        t.lower = lowercase_keeping_offsets(&t.text);
        t
    }

    /// A verse with no text of its own (omitted in this translation) still shows its number.
    fn flush_unnumbered(
        &mut self,
        unnumbered: &mut Option<u32>,
        open: &mut Option<usize>,
        waiting_notes: &mut Vec<usize>,
    ) {
        if let Some(verse) = unnumbered.take() {
            self.push_piece(verse, true, b'-', "", open, waiting_notes);
        }
    }

    fn close_chapter(&mut self) {
        let (blocks, verses, notes) =
            (self.blocks.len() as u32, self.verses.len() as u32, self.notes.len() as u32);
        if let Some(chapter) = self.books.last_mut().and_then(|b| b.chapters.last_mut()) {
            chapter.blocks.end = blocks;
            chapter.verses.end = verses;
            chapter.notes.end = notes;
        }
    }

    fn push_piece(
        &mut self,
        verse: u32,
        numbered: bool,
        brk: u8,
        body: &'static str,
        open: &mut Option<usize>,
        waiting_notes: &mut Vec<usize>,
    ) {
        let block = match *open {
            Some(block) if brk == b'-' => block,
            _ => {
                let at = self.pieces.len() as u32;
                let kind = if brk == b'-' { b'm' } else { brk };
                self.blocks.push(Block::Para { kind, pieces: at..at });
                self.blocks.len() - 1
            }
        };
        *open = Some(block);
        for note in waiting_notes.drain(..) {
            self.notes[note].block = block as u32;
        }

        let text_start = self.text.len() as u32;
        let spans_start = self.spans.len() as u32;
        let (mut flags, mut run_start) = (0u8, text_start);
        let mut rest = body;
        while let Some(at) = rest.find('<') {
            self.text.push_str(&rest[..at]);
            let closing = rest.as_bytes()[at + 1] == b'/';
            let letter = rest.as_bytes()[at + 1 + closing as usize];
            let flag = match letter {
                b'i' => ITALIC,
                b'n' => NAME,
                _ => RED,
            };
            let here = self.text.len() as u32;
            if flags != 0 && here > run_start {
                self.spans.push(Span { range: run_start..here, flags });
            }
            flags = if closing { flags & !flag } else { flags | flag };
            run_start = here;
            rest = &rest[at + 3 + closing as usize..];
        }
        self.text.push_str(rest);
        let text_end = self.text.len() as u32;
        if flags != 0 && text_end > run_start {
            self.spans.push(Span { range: run_start..text_end, flags });
        }

        self.pieces.push(Piece {
            verse,
            numbered,
            text: text_start..text_end,
            spans: spans_start..self.spans.len() as u32,
        });
        if let Block::Para { pieces, .. } = &mut self.blocks[block] {
            pieces.end = self.pieces.len() as u32;
        }
        let v = &mut self.verses[verse as usize];
        if numbered {
            v.text.start = text_start;
            v.block = block as u32;
        }
        v.text.end = text_end;
    }

    pub fn verse_text(&self, verse: u32) -> &str {
        let range = &self.verses[verse as usize].text;
        &self.text[range.start as usize..range.end as usize]
    }

    /// "John 3:16"
    pub fn verse_label(&self, verse: u32) -> String {
        let v = &self.verses[verse as usize];
        format!("{} {}:{}", self.books[v.book as usize].name, v.chapter, v.number)
    }

    /// The index of a verse, given its book, chapter and verse numbers.
    pub fn verse_at(&self, book: usize, chapter: usize, number: usize) -> Option<u32> {
        let chapter = self.books.get(book)?.chapters.get(chapter.checked_sub(1)?)?;
        chapter
            .verses
            .clone()
            .find(|&v| self.verses[v as usize].number as usize == number)
    }

    /// Resolve what the reader typed against this translation: the book, chapter and,
    /// when one was given and exists, the verse.
    pub fn locate(&self, query: &str) -> Option<(usize, usize, Option<u32>)> {
        let (book, numbers) = parse_reference(query)?;
        let chapters = self.books.get(book)?.chapters.len();
        let (chapter, verse) = match numbers {
            // "Jude 5" means verse 5 of the only chapter.
            (Some(n), None) if chapters == 1 && n > 1 => (1, Some(n)),
            (chapter, verse) => (chapter.unwrap_or(1), verse),
        };
        if chapter == 0 || chapter > chapters {
            return None;
        }
        match verse {
            Some(number) => Some((book, chapter, Some(self.verse_at(book, chapter, number)?))),
            None => Some((book, chapter, None)),
        }
    }

    /// Verses containing the query, ignoring case. Verses with the exact phrase come
    /// first, then verses that contain every word, each group in Bible order.
    pub fn search(&self, query: &str) -> Vec<u32> {
        let mut query = query.to_lowercase();
        if self.curly {
            query = query.replace('\'', "’");
        }
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let phrase = self.verses_containing(&words.join(" "));
        if words.len() == 1 {
            return phrase;
        }
        let mut all: Option<Vec<u32>> = None;
        for word in &words {
            let hits = self.verses_containing(word);
            all = Some(match all {
                None => hits,
                Some(prev) => intersect(&prev, &hits),
            });
        }
        let mut results = phrase.clone();
        results.extend(difference(&all.unwrap_or_default(), &phrase));
        results
    }

    fn verses_containing(&self, needle: &str) -> Vec<u32> {
        let mut found = Vec::new();
        let mut verse = 0usize;
        for at in memmem::find_iter(self.lower.as_bytes(), needle.as_bytes()) {
            while self.verses[verse].text.end as usize <= at {
                verse += 1;
            }
            if found.last() != Some(&(verse as u32)) {
                found.push(verse as u32);
            }
        }
        found
    }
}

/// Lowercase for searching. Every character keeps its byte length, so an offset
/// into the result is the same offset into the original.
fn lowercase_keeping_offsets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let mut lower = c.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(l), None) if l.len_utf8() == c.len_utf8() => out.push(l),
            _ => out.push(c),
        }
    }
    out
}

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at + rest[at..].find('>').map_or(1, |end| end + 1)..];
    }
    out.push_str(rest);
    out
}

fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn difference(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut j = 0;
    a.iter()
        .copied()
        .filter(|x| {
            while j < b.len() && b[j] < *x {
                j += 1;
            }
            j == b.len() || b[j] != *x
        })
        .collect()
}

/// Book names as typed, without spaces, in Bible order.
const NAMES: [&str; 66] = [
    "genesis", "exodus", "leviticus", "numbers", "deuteronomy", "joshua", "judges", "ruth",
    "1samuel", "2samuel", "1kings", "2kings", "1chronicles", "2chronicles", "ezra", "nehemiah",
    "esther", "job", "psalms", "proverbs", "ecclesiastes", "songofsolomon", "isaiah", "jeremiah",
    "lamentations", "ezekiel", "daniel", "hosea", "joel", "amos", "obadiah", "jonah", "micah",
    "nahum", "habakkuk", "zephaniah", "haggai", "zechariah", "malachi", "matthew", "mark", "luke",
    "john", "acts", "romans", "1corinthians", "2corinthians", "galatians", "ephesians",
    "philippians", "colossians", "1thessalonians", "2thessalonians", "1timothy", "2timothy",
    "titus", "philemon", "hebrews", "james", "1peter", "2peter", "1john", "2john", "3john",
    "jude", "revelation",
];

/// Abbreviations that are not simply the start of a name.
const ALIASES: [(&str, usize); 40] = [
    ("gn", 0), ("ex", 1), ("lv", 2), ("nm", 3), ("dt", 4), ("jdg", 6), ("jgs", 6), ("rt", 7),
    ("1sm", 8), ("2sm", 9), ("1kgs", 10), ("2kgs", 11), ("1chr", 12), ("2chr", 13), ("jb", 17),
    ("ps", 18), ("pss", 18), ("prv", 19), ("qoheleth", 20), ("sos", 21), ("sng", 21),
    ("songofsongs", 21), ("canticles", 21), ("jer", 23), ("ezk", 25), ("jol", 28), ("nam", 33),
    ("mt", 39), ("mk", 40), ("mrk", 40), ("lk", 41), ("jn", 42), ("jhn", 42), ("php", 49),
    ("phm", 56), ("jas", 58), ("1jn", 61), ("2jn", 62), ("3jn", 63), ("rv", 65),
];

/// Read "1 cor 13:4", "John 3", "ps23" or "Jude 5" as a book index plus up to two numbers.
/// Returns `None` when the text is not a reference, so it can be searched for instead.
pub fn parse_reference(query: &str) -> Option<(usize, (Option<usize>, Option<usize>))> {
    let query = query.trim().to_lowercase();
    let mut tokens = query.split_whitespace().peekable();
    let mut key = String::new();
    // A leading ordinal: "1", "ii", "first".
    if let Some(first) = tokens.peek() {
        let ordinal = match *first {
            "1" | "i" | "1st" | "first" => Some('1'),
            "2" | "ii" | "2nd" | "second" => Some('2'),
            "3" | "iii" | "3rd" | "third" => Some('3'),
            _ => None,
        };
        if let Some(digit) = ordinal {
            key.push(digit);
            tokens.next();
        }
    }
    let rest: String = tokens.collect::<Vec<_>>().join(" ");
    let mut chars = rest.char_indices().peekable();
    if key.is_empty() {
        if let Some(&(_, c)) = chars.peek() {
            if ('1'..='3').contains(&c) {
                key.push(c);
                chars.next();
            }
        }
    }
    let mut tail = rest.len();
    for (at, c) in chars {
        if c.is_ascii_digit() {
            tail = at;
            break;
        }
        if c.is_alphabetic() {
            key.push(c);
        } else if c != ' ' && c != '.' {
            return None;
        }
    }
    let letters = key.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if letters == 0 {
        return None;
    }

    let mut numbers = [None, None];
    let mut count = 0;
    // Whatever follows a dash is the end of a range ("3:16-18", "3-4"); the start is enough.
    let mut past_dash = false;
    let mut digits = String::new();
    for c in rest[tail..].chars().chain([' ']) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            if count < 2 && !past_dash {
                numbers[count] = Some(digits.parse().ok()?);
            }
            count += 1;
            digits.clear();
        }
        past_dash |= matches!(c, '-' | '–' | '—');
    }
    if rest[tail..].chars().any(|c| c.is_alphabetic()) || count > 3 {
        return None;
    }

    let book = NAMES
        .iter()
        .position(|name| *name == key)
        .or_else(|| ALIASES.iter().find(|(alias, _)| *alias == key).map(|(_, book)| *book))
        .or_else(|| {
            // Without a chapter number, two letters are too few to mean a book.
            if letters >= 3 || (letters >= 2 && count > 0) {
                NAMES.iter().position(|name| name.starts_with(&key))
            } else {
                None
            }
        })?;
    Some((book, (numbers[0], numbers[1])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn located(t: &Translation, query: &str) -> Option<String> {
        let (book, chapter, verse) = t.locate(query)?;
        Some(match verse {
            Some(v) => t.verse_label(v),
            None => format!("{} {}", t.books[book].name, chapter),
        })
    }

    #[test]
    fn every_translation_has_the_whole_canon() {
        for (id, verses) in [("KJV", 31_102), ("BSB", 31_086), ("WEB", 31_103)] {
            let t = translation(id);
            assert_eq!(t.books.len(), 66, "{id}");
            assert_eq!(t.books.iter().map(|b| b.chapters.len()).sum::<usize>(), 1189, "{id}");
            assert_eq!(t.verses.len(), verses, "{id}");
            for (b, book) in t.books.iter().enumerate() {
                for (c, chapter) in book.chapters.iter().enumerate() {
                    assert!(!chapter.verses.is_empty(), "{id} {} {}", book.name, c + 1);
                    for v in chapter.verses.clone() {
                        let verse = &t.verses[v as usize];
                        assert_eq!((verse.book as usize, verse.chapter as usize), (b, c + 1));
                        assert!(chapter.blocks.contains(&verse.block), "{}", t.verse_label(v));
                    }
                }
            }
        }
    }

    #[test]
    fn known_verses_read_correctly() {
        let kjv = translation("KJV");
        let john = kjv.verse_at(42, 3, 16).unwrap();
        assert_eq!(
            kjv.verse_text(john),
            "For God so loved the world, that he gave his only begotten Son, that whosoever \
             believeth in him should not perish, but have everlasting life."
        );
        assert_eq!(kjv.verse_label(john), "John 3:16");
        assert_eq!(
            kjv.verse_text(kjv.verse_at(0, 1, 1).unwrap()),
            "In the beginning God created the heaven and the earth."
        );
        assert_eq!(
            kjv.verse_text(kjv.verse_at(65, 22, 21).unwrap()),
            "The grace of our Lord Jesus Christ be with you all. Amen."
        );
        let bsb = translation("BSB");
        assert_eq!(bsb.verse_text(bsb.verse_at(42, 11, 35).unwrap()), "Jesus wept.");
        // A verse set as several lines of poetry reads as one sentence.
        let web = translation("WEB");
        assert_eq!(
            web.verse_text(web.verse_at(18, 23, 1).unwrap()),
            "The LORD is my shepherd; I shall lack nothing."
        );
    }

    #[test]
    fn markup_becomes_spans_not_text() {
        for id in translation_ids() {
            let t = translation(id);
            assert!(!t.text.contains('<') && !t.text.contains('>'), "{id}");
            assert!(t.spans.iter().any(|s| s.flags & RED != 0), "{id} has no words of Jesus");
            for piece in &t.pieces {
                for span in &t.spans[piece.spans.start as usize..piece.spans.end as usize] {
                    assert!(span.range.start >= piece.text.start && span.range.end <= piece.text.end);
                    assert!(span.range.start < span.range.end);
                }
            }
        }
        // "Jesus wept." is narration; "Lazarus, come forth." is speech.
        let kjv = translation("KJV");
        let red = |verse: u32| {
            let range = &kjv.verses[verse as usize].text;
            kjv.spans
                .iter()
                .filter(|s| s.flags & RED != 0 && s.range.start >= range.start && s.range.end <= range.end)
                .map(|s| &kjv.text[s.range.start as usize..s.range.end as usize])
                .collect::<String>()
        };
        assert_eq!(red(kjv.verse_at(42, 11, 35).unwrap()), "");
        assert_eq!(red(kjv.verse_at(42, 11, 43).unwrap()), "Lazarus, come forth.");
    }

    #[test]
    fn references_are_understood() {
        let t = translation("KJV");
        for (query, expected) in [
            ("John 3:16", "John 3:16"),
            ("jn 3.16", "John 3:16"),
            ("john3:16-18", "John 3:16"),
            ("1 cor 13:4", "1 Corinthians 13:4"),
            ("I Corinthians 13", "1 Corinthians 13"),
            ("2tim 3:16", "2 Timothy 3:16"),
            ("ps 23", "Psalms 23"),
            ("psalm 119:105", "Psalms 119:105"),
            ("gen", "Genesis 1"),
            ("Gen 1 1", "Genesis 1:1"),
            ("rev 22:21", "Revelation 22:21"),
            ("jude 5", "Jude 1:5"),
            ("phil 4:13", "Philippians 4:13"),
            ("phm 6", "Philemon 1:6"),
            ("song 2:1", "Song of Solomon 2:1"),
            ("3 john 4", "3 John 1:4"),
            ("mt 5", "Matthew 5"),
            ("john 3-4", "John 3"),
            ("jude 5-7", "Jude 1:5"),
            ("1 cor 13:4–7", "1 Corinthians 13:4"),
        ] {
            assert_eq!(located(t, query).as_deref(), Some(expected), "{query}");
        }
        for query in ["", "love", "in the beginning", "john 99", "john 3:99", "the", "is", "3:16", "faith hope"] {
            assert_eq!(located(t, query), None, "{query}");
        }
    }

    #[test]
    fn search_finds_phrases_then_words() {
        let kjv = translation("KJV");
        let hits = kjv.search("Jesus wept");
        assert_eq!(kjv.verse_label(hits[0]), "John 11:35");
        // The exact phrase outranks verses that merely contain the same words.
        assert_eq!(kjv.verse_label(kjv.search("in the beginning god created")[0]), "Genesis 1:1");
        assert_eq!(kjv.search("  "), Vec::<u32>::new());
        assert_eq!(kjv.search("xyzzy"), Vec::<u32>::new());
        // Every result contains every word, and nothing is listed twice.
        let hits = kjv.search("faith works");
        assert!(hits.len() > 10);
        let mut seen = std::collections::HashSet::new();
        for &v in &hits {
            let text = kjv.verse_text(v).to_lowercase();
            assert!(text.contains("faith") && text.contains("works"), "{}", kjv.verse_label(v));
            assert!(seen.insert(v));
        }
        // A phrase that spans two lines of one verse is still found; one that spans two verses is not.
        let web = translation("WEB");
        assert_eq!(web.verse_label(web.search("my shepherd; i shall lack")[0]), "Psalms 23:1");
        assert!(kjv.search("created the heaven and the earth. and the earth was").is_empty());
        // Capital letters outside ASCII are matched too.
        assert_eq!(kjv.verse_label(kjv.search("ænon")[0]), "John 3:23");
        assert_eq!(kjv.search("Æneas").len(), 2);
        // Straight apostrophes match the curly ones in the modern texts.
        assert!(!web.search("don't be afraid").is_empty());
    }
}
