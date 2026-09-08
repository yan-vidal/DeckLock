//! Procedural GUI contract in the private test display.
use decklock::{
    animation::{self, Animation, Effect},
    config::Config,
    settings,
};
use gtk::{gio, prelude::*};
use std::time::{Duration, Instant};
fn find(widget: &gtk::Widget, name: &str) -> gtk::Widget {
    fn walk(w: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if w.widget_name() == name {
            return Some(w.clone());
        }
        let mut child = w.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if let Some(found) = walk(&node, name) {
                return Some(found);
            }
        }
        None
    }
    walk(widget, name).unwrap_or_else(|| panic!("Missing widget {name}"))
}
fn top(name: &str) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap_or_else(|| panic!("Missing window {name}"))
        .downcast()
        .unwrap()
}
fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while Instant::now() < end && glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn click(widget: gtk::Widget) {
    widget.downcast::<gtk::Button>().unwrap().emit_clicked();
}
fn capture(window: &gtk::Window, name: &str) {
    let snapshot = gtk::Snapshot::new();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
    let node = snapshot.to_node().expect("Preview snapshot");
    let texture = window.renderer().unwrap().render_texture(&node, None);
    std::fs::create_dir_all("target/check-logs").unwrap();
    texture
        .save_to_png(format!("target/check-logs/{name}.png"))
        .unwrap();
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.AnimationCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    // Texture updates while mapped; unmapping stops work; dropping releases callbacks.
    for effect in [Effect::Starfield, Effect::Particles, Effect::Lissajous] {
        let picture = animation::widget(Animation {
            effect,
            ..Default::default()
        });
        let window = gtk::ApplicationWindow::builder()
            .application(&app)
            .default_width(1280)
            .default_height(720)
            .child(&picture)
            .build();
        window.present();
        pump(250);
        let first = picture.paintable().expect("Rendered frame");
        assert!(first.intrinsic_width() <= 640);
        pump(100);
        assert_ne!(picture.paintable().unwrap(), first);
        window.set_visible(false);
        pump(100);
        let stopped = picture.paintable();
        pump(100);
        assert_eq!(picture.paintable(), stopped);
        let weak = picture.downgrade();
        window.destroy();
        drop(picture);
        drop(window);
        pump(100);
        assert!(
            weak.upgrade().is_none(),
            "Animation callback retained its widget"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save(&path).unwrap();
    let window = settings::build(
        &app,
        path.clone(),
        Some("en-US"),
        std::env::current_exe().unwrap(),
    )
    .unwrap();
    window.present();
    pump(100);
    let get = |name| find(window.upcast_ref(), name);
    get("settings-animation-options")
        .downcast::<gtk::Expander>()
        .unwrap()
        .set_expanded(true);
    get("settings-idle-animation-options")
        .downcast::<gtk::Expander>()
        .unwrap()
        .set_expanded(true);
    pump(100);
    get("settings-animation-effect")
        .downcast::<gtk::DropDown>()
        .unwrap()
        .set_selected(1);
    get("settings-idle-animation-effect")
        .downcast::<gtk::DropDown>()
        .unwrap()
        .set_selected(3);
    click(get("settings-preview"));
    pump(500);
    let preview = top("settings-live-preview");
    assert!(
        find(preview.upcast_ref(), "procedural-background")
            .downcast::<gtk::Picture>()
            .unwrap()
            .paintable()
            .is_some()
    );
    capture(&preview, "animation-starfield");
    get("settings-media-tabs")
        .downcast::<gtk::Stack>()
        .unwrap()
        .set_visible_child_name("rest");
    pump(700);
    assert!(
        find(preview.upcast_ref(), "procedural-background")
            .downcast::<gtk::Picture>()
            .unwrap()
            .paintable()
            .is_some()
    );
    capture(&preview, "animation-rest");
    click(get("settings-save"));
    pump(200);
    let saved = Config::load(Some(&path)).unwrap();
    assert_eq!(saved.animation.effect, Effect::Starfield);
    assert_eq!(saved.idle_animation.effect, Effect::Lissajous);
    window.destroy();
    preview.destroy();
    pump(100);
    println!(
        "PASS: animated textures, bounded resolution, unmap/drop lifecycle, GUI save and rest preview"
    );
}
