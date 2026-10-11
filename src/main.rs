// A windowed program on Windows; without this a console opens behind it.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bible;
mod store;
mod ui;

use gpui::{App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size};
use std::borrow::Cow;

fn main() {
    // `BIBLE_TIMING=1 bible` reports how long loading and searching take, then exits.
    if std::env::var_os("BIBLE_TIMING").is_some() {
        for id in bible::translation_ids() {
            let started = std::time::Instant::now();
            let t = bible::translation(id);
            let loaded = started.elapsed();
            // Each figure is the average of 200 runs.
            let time = |work: &dyn Fn() -> usize| {
                let started = std::time::Instant::now();
                let mut found = 0;
                for _ in 0..200 {
                    found = std::hint::black_box(work());
                }
                (started.elapsed() / 200, found)
            };
            let (jump, _) = time(&|| t.locate("john 3:16").is_some() as usize);
            let (phrase, verses) = time(&|| t.search("for god so loved the world").len());
            let (common, many) = time(&|| t.search("the").len());
            println!("{id}: loaded {} verses in {loaded:.1?}", t.verses.len());
            println!("  go to \"john 3:16\": {jump:.2?}");
            println!("  search \"for god so loved the world\": {phrase:.2?} ({verses} verse)");
            println!("  search \"the\": {common:.2?} ({many} verses)");
        }
        return;
    }
    Application::new().run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/EBGaramond-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/EBGaramond-Italic.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/EBGaramond-Medium.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/EBGaramond-SemiBold.ttf").as_slice()),
            ])
            .expect("the bundled typeface loads");
        let bounds = Bounds::centered(None, size(px(1240.), px(840.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(800.), px(480.))),
                titlebar: Some(TitlebarOptions { title: Some("Bible".into()), ..Default::default() }),
                app_id: Some("bible".into()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| ui::Reader::new(window, cx)),
        )
        .expect("the window opens");
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
