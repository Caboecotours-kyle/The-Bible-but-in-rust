//! The window: a sidebar for finding a place, and the page itself.

use crate::bible::{self, Block, Head, ITALIC, RED, Translation};
use crate::store::{SIZES, Store, VerseKey};
use gpui::{prelude::*, *};
use std::cell::RefCell;
use std::ops::Range;
use std::time::{Duration, Instant};

const FACE: &str = "EB Garamond";
const CHROME: f32 = 15.5;
const RESULT_HEIGHT: f32 = 66.0;

#[derive(Clone, Copy)]
struct Theme {
    page: Hsla,
    side: Hsla,
    field: Hsla,
    ink: Hsla,
    muted: Hsla,
    rule: Hsla,
    hover: Hsla,
    red: Hsla,
    mark: Hsla,
    select: Hsla,
    bar: Hsla,
    bar_ink: Hsla,
}

fn theme(dark: bool) -> Theme {
    let c = |hex: u32| -> Hsla { rgb(hex).into() };
    if dark {
        Theme {
            page: c(0x1b1814),
            side: c(0x15120f),
            field: c(0x231f1a),
            ink: c(0xe8e1d3),
            muted: c(0xa3988a),
            rule: c(0x342e27),
            hover: c(0x2b2620),
            red: c(0xe8897d),
            mark: c(0x5c4a14),
            select: c(0x3d362c),
            bar: c(0xe8e1d3),
            bar_ink: c(0x1b1814),
        }
    } else {
        Theme {
            page: c(0xf7f2e8),
            side: c(0xefe8da),
            field: c(0xfbf8f1),
            ink: c(0x2a2520),
            muted: c(0x6b6153),
            rule: c(0xdcd2bf),
            hover: c(0xe5dcc9),
            red: c(0x9e2a22),
            mark: c(0xf3dd8b),
            select: c(0xe2d8c1),
            bar: c(0x2a2520),
            bar_ink: c(0xf7f2e8),
        }
    }
}

/// One laid-out paragraph or line of the open chapter.
struct Para {
    kind: u8,
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    /// Byte ranges of `text` that belong to each entry of `verses`.
    clicks: Vec<Range<usize>>,
    verses: Vec<u32>,
    notes: Vec<(u8, &'static str)>,
}

enum Row {
    Head(Head, SharedString),
    Gap,
    Para(Para),
}

pub struct Reader {
    store: Store,
    t: &'static Translation,
    focus: FocusHandle,
    page_scroll: ScrollHandle,
    side_scroll: ScrollHandle,
    results_scroll: UniformListScrollHandle,
    rows: Vec<Row>,
    /// Where each row's text landed on screen in the last frame, for scrolling to a verse.
    layouts: RefCell<Vec<Option<TextLayout>>>,
    /// A verse to bring into view once its chapter has been laid out.
    jump: Option<u32>,
    has_notes: bool,
    selected: Option<u32>,
    copied: bool,
    typing: bool,
    query: String,
    /// The reference the query names, if it names one.
    target: Option<(usize, usize, Option<u32>)>,
    results: Vec<u32>,
    cursor: usize,
    search_time: Duration,
    showing_saved: bool,
}

impl Reader {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut store = Store::load();
        let t = bible::translation(&store.translation);
        store.translation = t.id.to_string();
        if store.book >= t.books.len() || !(1..=t.books[store.book].chapters.len()).contains(&store.chapter) {
            (store.book, store.chapter) = (0, 1);
        }
        let focus = cx.focus_handle();
        window.focus(&focus);
        window.set_window_title("Bible");
        let mut reader = Reader {
            store,
            t,
            focus,
            page_scroll: ScrollHandle::new(),
            side_scroll: ScrollHandle::new(),
            results_scroll: UniformListScrollHandle::new(),
            rows: Vec::new(),
            layouts: RefCell::default(),
            jump: None,
            has_notes: false,
            selected: None,
            copied: false,
            typing: false,
            query: String::new(),
            target: None,
            results: Vec::new(),
            cursor: 0,
            search_time: Duration::ZERO,
            showing_saved: false,
        };
        reader.lay_out();
        // The sidebar has no size until it has been drawn once, so reveal the open book after that.
        cx.on_next_frame(window, |this, _, cx| {
            this.side_scroll.scroll_to_top_of_item(side_row(this.store.book));
            cx.notify();
        });
        reader
    }

    fn key_of(&self, verse: u32) -> VerseKey {
        let v = &self.t.verses[verse as usize];
        (v.book, v.chapter, v.number)
    }

    /// Rebuild the cached rows for the open chapter. Called whenever the chapter or
    /// anything that changes how it looks (theme, marks, selection) changes.
    fn lay_out(&mut self) {
        let th = theme(self.store.dark);
        let t = self.t;
        let chapter = &t.books[self.store.book].chapters[self.store.chapter - 1];
        let mut notes = t.notes[chapter.notes.start as usize..chapter.notes.end as usize].iter().peekable();
        self.has_notes = notes.peek().is_some();
        self.rows.clear();

        for block_ix in chapter.blocks.clone() {
            let row = match &t.blocks[block_ix as usize] {
                Block::Head(head, text) => {
                    // The KJV prints a Hebrew letter before each Psalm 119 heading. The typeface has
                    // no Hebrew, and a leading right-to-left letter reverses the line, so show the name alone.
                    let latin: String = text.chars().filter(|c| !('\u{0590}'..='\u{05FF}').contains(c)).collect();
                    Row::Head(*head, latin.trim().to_string().into())
                }
                Block::Gap => Row::Gap,
                Block::Para { kind, pieces } => {
                    let mut text = String::new();
                    let mut highlights = Vec::new();
                    let (mut clicks, mut verses) = (Vec::new(), Vec::new());
                    for piece in &t.pieces[pieces.start as usize..pieces.end as usize] {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        let key = self.key_of(piece.verse);
                        let background = if self.selected == Some(piece.verse) {
                            Some(th.select)
                        } else if self.store.highlights.contains(&key) {
                            Some(th.mark)
                        } else {
                            None
                        };
                        let start = text.len();
                        if piece.numbered {
                            push_superscript(&mut text, key.2);
                            let bookmarked = self.store.bookmarks.contains(&key);
                            highlights.push((
                                start..text.len(),
                                HighlightStyle {
                                    color: Some(if bookmarked { th.red } else { th.muted }),
                                    font_weight: bookmarked.then_some(FontWeight::SEMIBOLD),
                                    background_color: background,
                                    ..Default::default()
                                },
                            ));
                        }
                        // Walk the piece, alternating plain stretches with styled spans.
                        let base = text.len();
                        text.push_str(&t.text[piece.text.start as usize..piece.text.end as usize]);
                        let to_local = |at: u32| base + (at - piece.text.start) as usize;
                        let mut at = piece.text.start;
                        let plain = |from: u32, to: u32, highlights: &mut Vec<_>| {
                            if to > from && background.is_some() {
                                highlights.push((
                                    to_local(from)..to_local(to),
                                    HighlightStyle { background_color: background, ..Default::default() },
                                ));
                            }
                        };
                        for span in &t.spans[piece.spans.start as usize..piece.spans.end as usize] {
                            plain(at, span.range.start, &mut highlights);
                            let red = span.flags & RED != 0 && self.store.red_letters;
                            let italic = span.flags & ITALIC != 0;
                            if red || italic || background.is_some() {
                                highlights.push((
                                    to_local(span.range.start)..to_local(span.range.end),
                                    HighlightStyle {
                                        color: red.then_some(th.red),
                                        font_style: italic.then_some(FontStyle::Italic),
                                        background_color: background,
                                        ..Default::default()
                                    },
                                ));
                            }
                            at = span.range.end;
                        }
                        plain(at, piece.text.end, &mut highlights);
                        clicks.push(start..text.len());
                        verses.push(piece.verse);
                    }
                    let mut beside = Vec::new();
                    while let Some(note) = notes.next_if(|note| note.block <= block_ix) {
                        beside.push((note.verse, note.text));
                    }
                    Row::Para(Para { kind: *kind, text: text.into(), highlights, clicks, verses, notes: beside })
                }
            };
            self.rows.push(row);
        }
    }

    fn go(&mut self, book: usize, chapter: usize, verse: Option<u32>, cx: &mut Context<Self>) {
        if book != self.store.book {
            self.side_scroll.scroll_to_top_of_item(side_row(book));
        }
        (self.store.book, self.store.chapter) = (book, chapter);
        self.selected = verse;
        self.copied = false;
        self.store.save();
        self.lay_out();
        self.page_scroll.set_offset(point(px(0.), px(0.)));
        self.jump = verse;
        cx.notify();
    }

    /// Scroll so the verse sits a little below the top of the page.
    fn finish_jump(&mut self, cx: &mut Context<Self>) {
        let Some(verse) = self.jump.take() else { return };
        let layouts = self.layouts.borrow();
        for (row, layout) in self.rows.iter().zip(layouts.iter()) {
            let (Row::Para(para), Some(layout)) = (row, layout) else { continue };
            let Some(piece) = para.verses.iter().position(|v| *v == verse) else { continue };
            let Some(at) = layout.position_for_index(para.clicks[piece].start) else { return };
            let mut offset = self.page_scroll.offset();
            let wanted = offset.y - (at.y - self.page_scroll.bounds().top() - px(96.));
            offset.y = wanted.max(-self.page_scroll.max_offset().height).min(px(0.));
            self.page_scroll.set_offset(offset);
            cx.notify();
            return;
        }
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let (book, chapter) = (self.store.book, self.store.chapter);
        let books = &self.t.books;
        let next = if forward {
            if chapter < books[book].chapters.len() {
                Some((book, chapter + 1))
            } else {
                (book + 1 < books.len()).then_some((book + 1, 1))
            }
        } else if chapter > 1 {
            Some((book, chapter - 1))
        } else {
            book.checked_sub(1).map(|b| (b, books[b].chapters.len()))
        };
        if let Some((book, chapter)) = next {
            self.go(book, chapter, None, cx);
        }
    }

    fn neighbour_label(&self, forward: bool) -> Option<String> {
        let (book, chapter) = (self.store.book, self.store.chapter);
        let books = &self.t.books;
        let (b, c) = if forward {
            if chapter < books[book].chapters.len() {
                (book, chapter + 1)
            } else {
                (book + 1, 1)
            }
        } else if chapter > 1 {
            (book, chapter - 1)
        } else {
            (book.checked_sub(1)?, books[book - 1].chapters.len())
        };
        Some(format!("{} {}", books.get(b)?.name, c))
    }

    fn set_translation(&mut self, id: &'static str, cx: &mut Context<Self>) {
        let key = self.selected.map(|v| self.key_of(v));
        self.t = bible::translation(id);
        self.store.translation = id.to_string();
        self.selected = key.and_then(|(b, c, v)| self.t.verse_at(b as usize, c as usize, v as usize));
        self.copied = false;
        self.store.save();
        self.run_search();
        self.lay_out();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.store.save();
        self.lay_out();
        cx.notify();
    }

    fn select(&mut self, verse: Option<u32>, cx: &mut Context<Self>) {
        self.selected = if self.selected == verse { None } else { verse };
        self.copied = false;
        // A click on the page ends typing, so B, H and C act on the verse.
        self.typing = false;
        self.lay_out();
        cx.notify();
    }

    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(verse) = self.selected {
            let text = format!("{} ({}, {})", self.t.verse_text(verse), self.t.verse_label(verse), self.t.id);
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.copied = true;
            cx.notify();
        }
    }

    fn toggle_mark(&mut self, bookmark: bool, cx: &mut Context<Self>) {
        if let Some(verse) = self.selected {
            let key = self.key_of(verse);
            let set = if bookmark { &mut self.store.bookmarks } else { &mut self.store.highlights };
            if !set.remove(&key) {
                set.insert(key);
            }
            self.refresh(cx);
        }
    }

    fn resize(&mut self, by: f32, cx: &mut Context<Self>) {
        self.store.size = (self.store.size + by).clamp(*SIZES.start(), *SIZES.end());
        self.refresh(cx);
    }

    fn run_search(&mut self) {
        let query = self.query.trim();
        let started = Instant::now();
        self.target = self.t.locate(query);
        self.results = if query.chars().count() >= 2 { self.t.search(query) } else { Vec::new() };
        self.search_time = started.elapsed();
        self.cursor = 0;
        self.results_scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn result_count(&self) -> usize {
        self.results.len() + self.target.is_some() as usize
    }

    fn open_result(&mut self, index: usize, cx: &mut Context<Self>) {
        self.cursor = index;
        match (self.target, index) {
            (Some((book, chapter, verse)), 0) => self.go(book, chapter, verse, cx),
            (target, index) => {
                if let Some(&verse) = self.results.get(index - target.is_some() as usize) {
                    let v = &self.t.verses[verse as usize];
                    self.go(v.book as usize, v.chapter as usize, Some(verse), cx);
                }
            }
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let command = stroke.modifiers.control || stroke.modifiers.platform;
        let key = stroke.key.as_str();
        if command && key == "q" {
            cx.quit();
            return;
        }
        if command && matches!(key, "=" | "+" | "-") {
            self.resize(if key == "-" { -1.0 } else { 1.0 }, cx);
            return;
        }
        if self.typing {
            match key {
                "escape" => {
                    if self.query.is_empty() {
                        self.typing = false;
                    } else {
                        self.query.clear();
                        self.run_search();
                    }
                }
                "enter" => {
                    if self.result_count() > 0 {
                        // Leave the field so B, H and C act on the verse; the arrows still walk the results.
                        self.typing = false;
                        self.open_result(self.cursor, cx);
                    }
                }
                "down" | "up" => {
                    let last = self.result_count().saturating_sub(1);
                    self.cursor = if key == "down" { (self.cursor + 1).min(last) } else { self.cursor.saturating_sub(1) };
                    self.results_scroll.scroll_to_item(self.cursor, ScrollStrategy::Center);
                }
                "backspace" => {
                    if command || stroke.modifiers.alt {
                        let kept = self.query.trim_end().rfind(' ').map_or(0, |at| at + 1);
                        self.query.truncate(kept);
                    } else {
                        self.query.pop();
                    }
                    self.run_search();
                }
                "u" if command => {
                    self.query.clear();
                    self.run_search();
                }
                "v" if command => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.query.push_str(text.lines().next().unwrap_or("").trim());
                        self.run_search();
                    }
                }
                _ => {
                    if let (false, Some(typed)) = (command, &stroke.key_char) {
                        self.query.push_str(typed);
                        self.run_search();
                    }
                }
            }
            cx.notify();
            return;
        }
        match key {
            "/" => self.typing = true,
            "f" | "k" | "l" if command => self.typing = true,
            "down" | "up" if self.result_count() > 0 => {
                let last = self.result_count() - 1;
                let next = if key == "down" { (self.cursor + 1).min(last) } else { self.cursor.saturating_sub(1) };
                self.results_scroll.scroll_to_item(next, ScrollStrategy::Center);
                self.open_result(next, cx);
            }
            "left" | "[" => self.step(false, cx),
            "right" | "]" => self.step(true, cx),
            "escape" => self.select(None, cx),
            "c" => self.copy_selected(cx),
            "h" => self.toggle_mark(false, cx),
            "b" => self.toggle_mark(true, cx),
            "t" => {
                let ids: Vec<_> = bible::translation_ids().collect();
                let next = (ids.iter().position(|id| *id == self.t.id).unwrap_or(0) + 1) % ids.len();
                self.set_translation(ids[next], cx);
            }
            "r" => {
                self.store.red_letters = !self.store.red_letters;
                self.refresh(cx);
            }
            "n" => {
                self.store.notes = !self.store.notes;
                self.refresh(cx);
            }
            "d" => {
                self.store.dark = !self.store.dark;
                self.refresh(cx);
            }
            _ => return,
        }
        cx.notify();
    }

    // ---- sidebar ----

    fn search_field(&self, th: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.query.is_empty();
        div()
            .id("search")
            .m(px(12.))
            .px(px(11.))
            .h(px(36.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(7.))
            .bg(th.field)
            .border_1()
            .border_color(if self.typing { th.muted } else { th.rule })
            .cursor_text()
            .on_click(cx.listener(|this, _, _, cx| {
                this.typing = true;
                cx.notify();
            }))
            .when(!empty, |field| field.child(div().truncate().child(SharedString::from(self.query.clone()))))
            .when(self.typing, |field| field.child(div().w(px(1.)).h(px(18.)).flex_none().bg(th.ink)))
            .when(empty, |field| {
                field.child(
                    div()
                        .truncate()
                        .text_color(th.muted)
                        .child(if self.typing { "John 3:16, or any words" } else { "Search or go to a verse" }),
                )
            })
    }

    fn book_list(&self, th: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let label = |text: &'static str| {
            div().px(px(18.)).pt(px(14.)).pb(px(4.)).text_size(px(13.)).text_color(th.muted).child(text)
        };
        let mut list = div()
            .id("books")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.side_scroll)
            .pb(px(16.))
            .child(label("Old Testament"));
        for (b, book) in self.t.books.iter().enumerate() {
            let open = b == self.store.book;
            let mut entry = div().child(
                div()
                    .id(("book", b))
                    .mx(px(8.))
                    .px(px(10.))
                    .h(px(29.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(open, |row| row.font_weight(FontWeight::SEMIBOLD))
                    .hover(|row| row.bg(th.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.go(b, 1, None, cx)))
                    .child(book.name),
            );
            if open {
                let mut grid = div().flex().flex_wrap().gap(px(3.)).px(px(16.)).pt(px(4.)).pb(px(10.));
                for c in 1..=book.chapters.len() {
                    let current = c == self.store.chapter;
                    grid = grid.child(
                        div()
                            .id(("chapter", c))
                            .w(px(31.))
                            .h(px(27.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .text_size(px(14.))
                            .cursor_pointer()
                            .when(current, |cell| cell.bg(th.ink).text_color(th.side))
                            .when(!current, |cell| cell.text_color(th.muted).hover(|cell| cell.bg(th.hover).text_color(th.ink)))
                            .on_click(cx.listener(move |this, _, _, cx| this.go(b, c, None, cx)))
                            .child(c.to_string()),
                    );
                }
                entry = entry.child(grid);
            }
            list = list.child(entry);
            if b == 38 {
                list = list.child(label("New Testament"));
            }
        }
        list
    }

    fn saved_list(&self, th: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("saved").flex_1().min_h_0().overflow_y_scroll().pb(px(16.));
        if self.store.bookmarks.is_empty() && self.store.highlights.is_empty() {
            return list.child(
                div()
                    .px(px(18.))
                    .pt(px(14.))
                    .text_color(th.muted)
                    .child("Nothing saved yet. Click a verse, then press B to bookmark it or H to highlight it."),
            );
        }
        for (title, name, set) in
            [("Bookmarks", "bookmark", &self.store.bookmarks), ("Highlights", "highlight", &self.store.highlights)]
        {
            if set.is_empty() {
                continue;
            }
            list = list.child(
                div().px(px(18.)).pt(px(14.)).pb(px(4.)).text_size(px(13.)).text_color(th.muted).child(title),
            );
            for (i, &(b, c, v)) in set.iter().enumerate() {
                let Some(verse) = self.t.verse_at(b as usize, c as usize, v as usize) else { continue };
                list = list.child(
                    div()
                        .id((name, i))
                        .mx(px(8.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|row| row.bg(th.hover))
                        .on_click(cx.listener(move |this, _, _, cx| this.go(b as usize, c as usize, Some(verse), cx)))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(self.t.verse_label(verse)))
                        .child(
                            div()
                                .text_size(px(14.5))
                                .line_height(px(19.))
                                .text_color(th.muted)
                                .line_clamp(2)
                                .child(self.t.verse_text(verse).to_string()),
                        ),
                );
            }
        }
        list
    }

    fn results_list(&self, th: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let summary = if self.results.is_empty() && self.target.is_some() {
            "Go to".to_string()
        } else if self.results.is_empty() {
            if self.query.trim().chars().count() < 2 { "Keep typing".to_string() } else { "No verses found".to_string() }
        } else {
            format!(
                "{} {} in {:.1} ms",
                thousands(self.results.len()),
                if self.results.len() == 1 { "verse" } else { "verses" },
                self.search_time.as_secs_f64() * 1000.0
            )
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().px(px(18.)).pb(px(6.)).text_size(px(13.)).text_color(th.muted).child(summary))
            .child(
                uniform_list(
                    "results",
                    self.result_count(),
                    cx.processor(move |this, range: Range<usize>, _window, cx| {
                        let t = this.t;
                        range
                            .map(|index| {
                                let (title, body) = match (this.target, index) {
                                    (Some((book, chapter, verse)), 0) => (
                                        match verse {
                                            Some(v) => t.verse_label(v),
                                            None => format!("{} {}", t.books[book].name, chapter),
                                        },
                                        match verse {
                                            Some(v) => t.verse_text(v).to_string(),
                                            None => "Open this chapter".to_string(),
                                        },
                                    ),
                                    (target, index) => {
                                        let verse = this.results[index - target.is_some() as usize];
                                        (t.verse_label(verse), t.verse_text(verse).to_string())
                                    }
                                };
                                let current = index == this.cursor;
                                div().id(index).h(px(RESULT_HEIGHT)).px(px(8.)).child(
                                    div()
                                        .h(px(RESULT_HEIGHT - 4.))
                                        .px(px(10.))
                                        .py(px(5.))
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .when(current, |row| row.bg(th.hover))
                                        .hover(|row| row.bg(th.hover))
                                        .child(div().font_weight(FontWeight::SEMIBOLD).line_height(px(19.)).truncate().child(title))
                                        .child(
                                            div()
                                                .text_size(px(14.5))
                                                .line_height(px(18.))
                                                .text_color(th.muted)
                                                .line_clamp(2)
                                                .child(body),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.typing = false;
                                    this.open_result(index, cx);
                                }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.results_scroll.clone())
                .flex_1(),
            )
    }

    fn sidebar(&self, th: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.query.is_empty();
        let saved = self.store.bookmarks.len() + self.store.highlights.len();
        let tab = |id: &'static str, label: String, active: bool| {
            div()
                .id(id)
                .px(px(10.))
                .h(px(28.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(active, |tab| tab.bg(th.hover).font_weight(FontWeight::SEMIBOLD))
                .when(!active, |tab| tab.text_color(th.muted).hover(|tab| tab.text_color(th.ink)))
                .child(label)
        };
        div()
            .w(px(272.))
            .h_full()
            .flex()
            .flex_col()
            .flex_none()
            .bg(th.side)
            .border_r_1()
            .border_color(th.rule)
            .child(self.search_field(&th, cx))
            .when(!searching, |side| {
                side.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap(px(4.))
                        .px(px(8.))
                        .child(tab("tab-books", "Books".into(), !self.showing_saved).on_click(cx.listener(|this, _, _, cx| {
                            this.showing_saved = false;
                            cx.notify();
                        })))
                        .child(
                            tab(
                                "tab-saved",
                                if saved == 0 { "Saved".to_string() } else { format!("Saved {saved}") },
                                self.showing_saved,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.showing_saved = true;
                                cx.notify();
                            })),
                        ),
                )
            })
            .map(|side| {
                if searching {
                    side.child(self.results_list(th, cx))
                } else if self.showing_saved {
                    side.child(self.saved_list(&th, cx))
                } else {
                    side.child(self.book_list(&th, cx))
                }
            })
    }

    // ---- page ----

    fn toolbar(&self, th: Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page heading already names the chapter, so the title gives way first.
        let narrow = window.viewport_size().width < px(980.);
        let button = move |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .px(px(9.))
                .h(px(28.))
                .flex()
                .flex_none()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(on, |b| b.bg(th.hover))
                .when(!on, |b| b.text_color(th.muted))
                .hover(|b| b.bg(th.hover).text_color(th.ink))
                .child(label)
        };
        let book = &self.t.books[self.store.book];
        let mut versions = div().flex().flex_none().gap(px(2.)).p(px(2.)).rounded(px(8.)).border_1().border_color(th.rule);
        for id in bible::translation_ids() {
            let on = id == self.t.id;
            versions = versions.child(
                div()
                    .id(id)
                    .px(px(9.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(on, |b| b.bg(th.ink).text_color(th.page))
                    .when(!on, |b| b.text_color(th.muted).hover(|b| b.text_color(th.ink)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_translation(id, cx)))
                    .child(id),
            );
        }
        div()
            .h(px(48.))
            .px(px(14.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .border_b_1()
            .border_color(th.rule)
            .child(button("back", "‹", false).text_size(px(22.)).on_click(cx.listener(|this, _, _, cx| this.step(false, cx))))
            .child(button("forward", "›", false).text_size(px(22.)).on_click(cx.listener(|this, _, _, cx| this.step(true, cx))))
            .when(!narrow, |bar| {
                bar.child(
                    div()
                        .pl(px(6.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(17.))
                        .truncate()
                        .child(format!("{} {}", book.name, self.store.chapter)),
                )
            })
            .child(div().flex_1())
            .child(versions)
            .child(div().w(px(6.)))
            .child(button("red", if narrow { "Red" } else { "Red letters" }, self.store.red_letters).on_click(cx.listener(|this, _, _, cx| {
                this.store.red_letters = !this.store.red_letters;
                this.refresh(cx);
            })))
            .child(button("notes", "Notes", self.store.notes).on_click(cx.listener(|this, _, _, cx| {
                this.store.notes = !this.store.notes;
                this.refresh(cx);
            })))
            .child(button("smaller", "A", false).text_size(px(13.)).on_click(cx.listener(|this, _, _, cx| this.resize(-1.0, cx))))
            .child(button("larger", "A", false).text_size(px(19.)).on_click(cx.listener(|this, _, _, cx| this.resize(1.0, cx))))
            .child(button("theme", if self.store.dark { "Light" } else { "Dark" }, false).on_click(cx.listener(
                |this, _, _, cx| {
                    this.store.dark = !this.store.dark;
                    this.refresh(cx);
                },
            )))
    }

    fn page(&self, th: Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = self.store.size;
        let em = |n: f32| px(size * n);
        let book = &self.t.books[self.store.book];
        let with_notes = self.store.notes && self.has_notes && window.viewport_size().width > px(980.);
        let column = em(33.) + if with_notes { px(236.) } else { px(0.) };
        // Every row is a direct child of the scroller so a verse can be scrolled to by row.
        let row = move |inner: Div| div().w_full().flex().flex_none().justify_center().px(px(36.)).child(inner.w_full().max_w(column));
        let text_column = move |inner: Div| if with_notes { div().flex().child(inner.flex_1().min_w_0()).child(div().w(px(236.)).flex_none()) } else { inner };

        let mut page = div()
            .id("page")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.page_scroll)
            .text_size(px(size))
            .line_height(em(1.52))
            .child(row(text_column(
                div()
                    .pt(em(2.2))
                    .pb(em(0.9))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().text_size(em(0.8)).line_height(em(1.2)).text_color(th.muted).child(book.name.to_uppercase()))
                    .child(
                        div()
                            .text_size(em(3.1))
                            .line_height(em(3.5))
                            .text_color(th.red)
                            .child(self.store.chapter.to_string()),
                    ),
            )));

        let entity = cx.entity();
        let mut layouts = self.layouts.borrow_mut();
        layouts.clear();
        layouts.resize(self.rows.len(), None);
        let mut after_text = false;
        for (ix, laid) in self.rows.iter().enumerate() {
            let element = match laid {
                Row::Gap => row(div().h(em(0.55))),
                Row::Head(head, text) => {
                    let base = div().child(text.clone());
                    row(text_column(match head {
                        Head::Major => base.pt(em(1.0)).text_center().text_size(em(0.82)).text_color(th.muted),
                        Head::Section => base.pt(em(if after_text { 1.3 } else { 0.3 })).pb(em(0.1)).font_weight(FontWeight::SEMIBOLD),
                        Head::Reference => base.italic().text_size(em(0.8)).line_height(em(1.15)).pb(em(0.3)).text_color(th.muted),
                        Head::Title => base.italic().pb(em(0.5)).text_color(th.muted),
                        Head::Minor => base.pt(em(0.9)).pb(em(0.2)).italic().text_size(em(0.86)).line_height(em(1.25)).text_color(th.muted),
                    }))
                }
                Row::Para(para) => {
                    let verses = para.verses.clone();
                    let entity = entity.clone();
                    let styled = StyledText::new(para.text.clone()).with_highlights(para.highlights.clone());
                    layouts[ix] = Some(styled.layout().clone());
                    let text = InteractiveText::new(("para", ix), styled)
                        .on_click(para.clicks.clone(), move |clicked, _window, cx| {
                            let verse = verses[clicked];
                            entity.update(cx, |this, cx| this.select(Some(verse), cx));
                        });
                    let body = match para.kind {
                        b'1' | b'2' | b'3' => div().pl(em(1.5 * (para.kind - b'1') as f32)).child(text),
                        b'c' => div().pt(em(0.5)).text_center().child(text),
                        b'p' if after_text => div().pt(em(0.5)).child(text),
                        _ => div().child(text),
                    };
                    row(if with_notes {
                        let mut margin = div().w(px(236.)).flex_none().pl(px(28.)).pt(em(if para.kind == b'p' && after_text { 0.62 } else { 0.12 })).flex().flex_col().gap(px(5.)).text_size(px(14.5)).line_height(px(18.5)).text_color(th.muted);
                        for (verse, note) in &para.notes {
                            let label = if *verse == 0 { String::new() } else { format!("{verse}  ") };
                            let lead = label.len();
                            margin = margin.child(StyledText::new(format!("{label}{note}")).with_highlights([(
                                0..lead,
                                HighlightStyle { color: Some(th.ink), font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() },
                            )]));
                        }
                        div().flex().items_start().child(body.flex_1().min_w_0()).child(margin)
                    } else {
                        body
                    })
                }
            };
            after_text = matches!(laid, Row::Para(_));
            page = page.child(element);
        }
        drop(layouts);

        let link = |id: &'static str, label: Option<String>, forward: bool, cx: &mut Context<Self>| {
            div().id(id).when_some(label, |link, label| {
                link.px(px(10.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(th.muted)
                    .hover(|link| link.bg(th.hover).text_color(th.ink))
                    .on_click(cx.listener(move |this, _, _, cx| this.step(forward, cx)))
                    .child(if forward { format!("{label}  ›") } else { format!("‹  {label}") })
            })
        };
        page.child(row(text_column(
            div()
                .pt(em(2.4))
                .pb(em(4.5))
                .flex()
                .justify_between()
                .text_size(px(16.))
                .line_height(px(22.))
                .child(link("previous", self.neighbour_label(false), false, cx))
                .child(link("next", self.neighbour_label(true), true, cx)),
        )))
    }

    fn verse_bar(&self, verse: u32, th: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let key = self.key_of(verse);
        let action = move |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px(px(10.))
                .h(px(28.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|a| a.bg(th.bar_ink.opacity(0.16)))
                .child(label)
        };
        div().absolute().bottom(px(22.)).left_0().right_0().flex().justify_center().child(
            div()
                .occlude()
                .flex()
                .items_center()
                .gap(px(2.))
                .pl(px(16.))
                .pr(px(6.))
                .h(px(40.))
                .rounded(px(10.))
                .bg(th.bar)
                .text_color(th.bar_ink)
                .shadow_lg()
                .child(div().pr(px(10.)).font_weight(FontWeight::SEMIBOLD).child(self.t.verse_label(verse)))
                .child(action("copy", if self.copied { "Copied" } else { "Copy" }).on_click(cx.listener(|this, _, _, cx| this.copy_selected(cx))))
                .child(
                    action("highlight", if self.store.highlights.contains(&key) { "Remove highlight" } else { "Highlight" })
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_mark(false, cx))),
                )
                .child(
                    action("bookmark", if self.store.bookmarks.contains(&key) { "Remove bookmark" } else { "Bookmark" })
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_mark(true, cx))),
                )
                .child(action("close", "×").text_size(px(19.)).on_click(cx.listener(|this, _, _, cx| this.select(None, cx)))),
        )
    }
}

impl Render for Reader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = theme(self.store.dark);
        if self.jump.is_some() {
            cx.on_next_frame(window, |this, _, cx| this.finish_jump(cx));
        }
        div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .bg(th.page)
            .text_color(th.ink)
            .font_family(FACE)
            .text_size(px(CHROME))
            .line_height(px(21.))
            .child(self.sidebar(th, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.toolbar(th, window, cx))
                    .child(self.page(th, window, cx))
                    .when_some(self.selected, |main, verse| main.child(self.verse_bar(verse, th, cx))),
            )
    }
}

/// The position of a book in the sidebar list, which also holds a label before
/// Genesis and another before Matthew.
fn side_row(book: usize) -> usize {
    book + 1 + (book > 38) as usize
}

fn push_superscript(out: &mut String, number: u8) {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    for digit in number.to_string().bytes() {
        out.push(DIGITS[(digit - b'0') as usize]);
    }
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
