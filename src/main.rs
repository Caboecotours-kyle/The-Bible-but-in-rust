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
            let started = std::time::Instant::now();
            let hits = t.search("the lord is my shepherd").len() + t.search("the").len();
            println!("{id}: loaded {} verses in {loaded:.1?}, two searches ({hits} hits) in {:.1?}", t.verses.len(), started.elapsed());
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
