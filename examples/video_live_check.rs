//! Regression: video paintable remains active when a settings preview is rebuilt.
use gtk::{gio, prelude::*};
fn picture(w: &gtk::Widget) -> Option<gtk::Picture> {
    if let Some(p) = w.downcast_ref::<gtk::Picture>() {
        return Some(p.clone());
    }
    let mut c = w.first_child();
    while let Some(w) = c {
        c = w.next_sibling();
        if let Some(p) = picture(&w) {
            return Some(p);
        }
    }
    None
}
fn pump() {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < until {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.VideoCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let mut c = decklock::config::Config {
        background: Some(std::env::args_os().nth(1).unwrap().into()),
        ..Default::default()
    };
    let mut live = decklock::live_preview::LivePreview::default();
    let mut initial = None;
    for preset in ["catppuccin-mocha", "catppuccin-latte", "tokyo-night"] {
        c.theme_preset = preset.into();
        live.update(&app, c.clone(), true).unwrap();
        pump();
        let p = gtk::Window::list_toplevels()
            .iter()
            .find_map(picture)
            .unwrap();
        let paintable = p.paintable().unwrap();
        assert!(paintable.intrinsic_width() > 0);
        if let Some(first) = &initial {
            assert_eq!(first, &paintable, "Palette change restarted video");
        } else {
            initial = Some(paintable);
        }
    }
    live.close();
    println!("PASS: video paintable retained across three palettes");
}
