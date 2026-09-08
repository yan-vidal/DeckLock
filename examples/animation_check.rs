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
    Config {
        background_pool: Some(vec![]),
        idle_pool: Some(vec![]),
        ..Default::default()
    }
    .save(&path)
    .unwrap();
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

    fn item(window: &gtk::ApplicationWindow, prefix: &str, id: &str) -> gtk::ListBoxRow {
        let list = find(window.upcast_ref(), &format!("{prefix}-procedurals"))
            .downcast::<gtk::ListBox>()
            .unwrap();
        let mut child = list.first_child();
        while let Some(row) = child {
            child = row.next_sibling();
            if row.first_child().and_then(|c| c.tooltip_text()).as_deref()
                == Some(&format!("procedural:{id}"))
            {
                return row.downcast().unwrap();
            }
        }
        panic!("Missing procedural {id}")
    }
    let stars = item(&window, "settings-background", "starfield");
    get("settings-background-tabs")
        .downcast::<gtk::Notebook>()
        .unwrap()
        .set_current_page(Some(2));
    let library = get("settings-background-procedurals")
        .downcast::<gtk::ListBox>()
        .unwrap();
    library.select_row(Some(&stars));
    click(get("settings-background-add"));
    let rest = item(&window, "settings-idle-background", "lissajous");
    get("settings-idle-background-tabs")
        .downcast::<gtk::Notebook>()
        .unwrap()
        .set_current_page(Some(2));
    get("settings-idle-background-procedurals")
        .downcast::<gtk::ListBox>()
        .unwrap()
        .select_row(Some(&rest));
    click(get("settings-idle-background-add"));
    click(find(stars.upcast_ref(), "media-eye"));
    pump(200);
    let viewer = top("media-viewer");
    assert!(
        find(viewer.upcast_ref(), "procedural-background")
            .downcast::<gtk::Picture>()
            .unwrap()
            .paintable()
            .is_some()
    );
    let before = std::fs::read(&path).unwrap();
    click(find(stars.upcast_ref(), "media-configure"));
    pump(100);
    let editor = top("procedural-editor");
    capture(&editor, "procedural-editor");
    let color = find(editor.upcast_ref(), "procedural-color")
        .downcast::<gtk::Entry>()
        .unwrap();
    color.set_text("invalid");
    click(find(editor.upcast_ref(), "procedural-apply"));
    assert!(editor.is_visible());
    color.set_text("#abcdef");
    find(editor.upcast_ref(), "procedural-speed")
        .downcast::<gtk::SpinButton>()
        .unwrap()
        .set_value(0.6);
    click(find(editor.upcast_ref(), "procedural-apply"));
    pump(200);
    assert!(!editor.is_visible());
    assert_eq!(top("media-viewer"), viewer);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "Apply must keep draft changes off disk"
    );
    click(get("settings-preview"));
    pump(500);
    let preview = top("settings-live-preview");
    let animation = find(preview.upcast_ref(), "procedural-background")
        .downcast::<gtk::Picture>()
        .unwrap();
    assert!(animation.paintable().is_some());
    let background = find(preview.upcast_ref(), "background");
    let stack = background
        .first_child()
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    assert_eq!(
        stack.visible_child(),
        Some(animation.clone().upcast()),
        "Procedural must be the actual background, not an overlay"
    );
    capture(&preview, "animation-starfield");
    get("settings-media-tabs")
        .downcast::<gtk::Stack>()
        .unwrap()
        .set_visible_child_name("rest");
    pump(800);
    assert!(
        find(preview.upcast_ref(), "procedural-background")
            .downcast::<gtk::Picture>()
            .unwrap()
            .paintable()
            .is_some()
    );
    let rest_stack = find(preview.upcast_ref(), "background")
        .first_child()
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while rest_stack.is_transition_running() && Instant::now() < deadline {
        pump(20);
    }
    assert!(!rest_stack.is_transition_running());
    let mut child = rest_stack.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if Some(&widget) != rest_stack.visible_child().as_ref() {
            assert!(!widget.is_mapped(), "Previous media must stop rendering");
        }
    }
    capture(&preview, "animation-rest");
    capture(window.upcast_ref(), "procedural-library");
    click(get("settings-save"));
    pump(200);
    let saved = Config::load(Some(&path)).unwrap();
    assert_eq!(
        saved.background_pool,
        Some(vec![decklock::procedural::path("starfield")])
    );
    assert_eq!(
        saved.idle_pool,
        Some(vec![decklock::procedural::path("lissajous")])
    );
    assert_eq!(saved.procedurals.starfield.color, "#abcdef");
    assert_eq!(saved.procedurals.starfield.speed, 0.6);
    // Pool removal leaves the library item and its parameters intact.
    let pool = get("settings-background-pool")
        .downcast::<gtk::ListBox>()
        .unwrap();
    pool.select_row(pool.row_at_index(0).as_ref());
    click(get("settings-background-remove"));
    assert!(pool.row_at_index(0).is_none());
    assert!(library.row_at_index(0).is_some());
    window.destroy();
    preview.destroy();
    pump(100);
    println!(
        "PASS: procedural library, eye/gear, draft validation, exclusive background/pools, persistence and lifecycle"
    );
}
