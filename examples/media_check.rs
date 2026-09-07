//! Real GTK checks using temporary media and preview-only windows.
use decklock::{
    config::{Config, Theme},
    i18n::I18n,
    media_editor, ui,
};
use gtk::{gio, prelude::*};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
fn find(widget: &gtk::Widget, name: &str) -> gtk::Widget {
    if widget.widget_name() == name {
        return widget.clone();
    }
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if let Some(found) = walk(&node, name) {
                return Some(found);
            }
        }
        None
    }
    walk(widget, name).unwrap_or_else(|| panic!("Missing {name}"))
}
fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    while Instant::now() < end {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.MediaCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.svg");
    let b = dir.path().join("b.svg");
    for (path, color) in [(&a, "red"), (&b, "blue")] {
        std::fs::write(path, format!("<svg xmlns='http://www.w3.org/2000/svg' width='160' height='90'><rect width='160' height='90' fill='{color}'/></svg>")).unwrap();
    }
    let window = gtk::ApplicationWindow::builder().application(&app).build();
    let editor = media_editor::build(
        &window,
        media_editor::Catalog::new(vec![a.clone(), b.clone()]),
        vec![a.clone()],
        Rc::new(I18n::new(Some("en-US"), None).unwrap()),
        "media-test",
    );
    window.set_child(Some(&editor.widget));
    window.present();
    pump(Duration::from_millis(100));
    let images = find(window.upcast_ref(), "media-test-images")
        .downcast::<gtk::ListBox>()
        .unwrap();
    images.select_row(images.row_at_index(1).as_ref());
    let add = find(window.upcast_ref(), "media-test-add")
        .downcast::<gtk::Button>()
        .unwrap();
    add.emit_clicked();
    add.emit_clicked();
    assert_eq!(*editor.paths.borrow(), vec![a.clone(), b.clone()]);
    let pool = find(window.upcast_ref(), "media-test-pool")
        .downcast::<gtk::ListBox>()
        .unwrap();
    pool.select_row(pool.row_at_index(0).as_ref());
    find(window.upcast_ref(), "media-test-remove")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(*editor.paths.borrow(), vec![b.clone()]);
    assert!(a.is_file());
    window.destroy();
    let settings = |enabled, reuse, interval| {
        Rc::new(ui::Settings {
            config: Config {
                background_pool: Some(vec![a.clone(), b.clone()]),
                idle_pool: Some(vec![b.clone()]),
                idle_enabled: enabled,
                idle_reuse_background: reuse,
                idle_seconds: 1,
                slideshow_seconds: interval,
                ..Config::default()
            },
            theme: Theme::load(None).unwrap(),
            strings: I18n::new(Some("en-US"), None).unwrap(),
            preview: true,
            show_keyboard: false,
            start_idle: true,
            username: "Test".into(),
        })
    };
    let view = ui::build(
        &app,
        settings(true, true, 30),
        Rc::new(|_| panic!("No authentication")),
    );
    view.window.present();
    let stack = find(view.window.upcast_ref(), "background")
        .first_child()
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    let initial = stack.visible_child();
    pump(Duration::from_millis(400));
    assert_eq!(stack.visible_child(), initial, "Reuse restarted background");
    assert!(!find(view.window.upcast_ref(), "power").is_visible());
    assert!(!find(view.window.upcast_ref(), "clock-block").is_visible());
    view.activity.set(Instant::now());
    pump(Duration::from_millis(300));
    assert!(find(view.window.upcast_ref(), "power").is_visible());
    assert_eq!(stack.visible_child(), initial);
    view.window.destroy();
    let view = ui::build(
        &app,
        settings(false, false, 1),
        Rc::new(|_| panic!("No authentication")),
    );
    view.window.present();
    let stack = find(view.window.upcast_ref(), "background")
        .first_child()
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    let initial = stack.visible_child();
    pump(Duration::from_millis(1400));
    assert!(
        find(view.window.upcast_ref(), "power").is_visible(),
        "Disabled idle became active"
    );
    assert_ne!(stack.visible_child(), initial, "Slideshow did not advance");
    view.window.destroy();
    println!(
        "PASS: pool add/remove/dedup, file preservation, idle reuse without resetting media, wake, disabled idle and live slideshow"
    );
}
