//! Help, language and decorations on the gate's private display; no real lock.
use decklock::{
    config::{Config, Theme},
    i18n::I18n,
    settings, ui,
};
use gtk::{gdk, gio, prelude::*};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
fn find(w: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if w.widget_name() == name {
        return Some(w.clone());
    }
    let mut child = w.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if let Some(w) = find(&node, name) {
            return Some(w);
        }
    }
    None
}
fn text(w: &gtk::Widget) -> String {
    let mut result = w
        .downcast_ref::<gtk::Label>()
        .map(|l| l.text().to_string())
        .unwrap_or_default();
    let mut child = w.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        result.push_str(&text(&node));
    }
    result
}
fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while Instant::now() < end && glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn help(parent: &gtk::Window) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.widget_name() == "decklock-help" && w.transient_for().as_ref() == Some(parent))
        .expect("Help window")
}
fn f1(window: &impl IsA<gtk::Widget>) {
    let controllers = window.observe_controllers();
    let key = (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .find(|c| c.name().as_deref() == Some("decklock-help-shortcut"))
        .expect("F1 controller");
    assert!(key.emit_by_name::<bool>(
        "key-pressed",
        &[&gdk::Key::F1, &0u32, &gdk::ModifierType::empty()]
    ));
}
fn chrome(window: &gtk::Window) {
    let bar = window
        .titlebar()
        .and_downcast::<gtk::HeaderBar>()
        .expect("Explicit GTK header");
    assert_eq!(bar.decoration_layout().as_deref(), Some(":close"));
}
fn capture(window: &gtk::Window, name: &str) {
    // GTK snapshots are asynchronous after mapping or replacing a whole chapter.
    let end = Instant::now() + Duration::from_secs(3);
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let node = loop {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
        if let Some(node) = snapshot.to_node() {
            break node;
        }
        assert!(
            Instant::now() < end,
            "Help did not render: mapped={} size={}x{}",
            window.is_mapped(),
            window.width(),
            window.height()
        );
        pump(20);
    };
    window
        .renderer()
        .unwrap()
        .render_texture(node, None)
        .save_to_png(format!("target/check-logs/{name}.png"))
        .unwrap();
}

fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.HelpCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config {
        background_pool: Some(vec![]),
        idle_pool: Some(vec![]),
        ..Default::default()
    }
    .save(&path)
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let window = settings::build(
        &app,
        path.clone(),
        Some("en-US"),
        std::env::current_exe().unwrap(),
    )
    .unwrap();
    chrome(window.upcast_ref());
    window.present();
    pump(100);
    f1(&window);
    pump(100);
    let guide = help(window.upcast_ref());
    chrome(&guide);
    let tabs = guide.child().and_downcast::<gtk::Notebook>().unwrap();
    assert_eq!(tabs.n_pages(), 2);
    assert!(text(guide.upcast_ref()).contains("Using DeckLock"));
    capture(&guide, "help-general");
    tabs.set_current_page(Some(1));
    assert!(text(guide.upcast_ref()).contains("Inside DeckLock"));
    find(window.upcast_ref(), "help-button")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(
        help(window.upcast_ref()),
        guide,
        "Reuse one guide per parent"
    );
    let language = find(window.upcast_ref(), "settings-language")
        .unwrap()
        .downcast::<gtk::DropDown>()
        .unwrap();
    language.set_selected(1);
    assert_eq!(guide.title().as_deref(), Some("Ajuda do DeckLock"));
    assert_eq!(tabs.current_page(), Some(1));
    assert!(text(guide.upcast_ref()).contains("Por dentro do DeckLock"));
    assert!(!text(guide.upcast_ref()).contains("Inside DeckLock"));
    pump(100);
    capture(&guide, "help-advanced-pt");
    // The appearance section only joins the widget tree once its expander opens.
    find(window.upcast_ref(), "settings-layout-options")
        .unwrap()
        .downcast::<gtk::Expander>()
        .unwrap()
        .set_expanded(true);
    let decorations = find(window.upcast_ref(), "settings-window-decorations")
        .unwrap()
        .downcast::<gtk::CheckButton>()
        .unwrap();
    decorations.set_active(false);
    assert!(!window.is_decorated() && !guide.is_decorated());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "Help and unsaved preferences must not write config"
    );
    find(window.upcast_ref(), "settings-save")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert!(!Config::load(Some(&path)).unwrap().window_decorations);
    decorations.set_active(true);
    assert!(window.is_decorated() && guide.is_decorated());
    guide.close();
    pump(30);
    f1(&window);
    pump(50);
    let reopened = help(window.upcast_ref());
    assert!(text(reopened.upcast_ref()).contains("Usando o DeckLock"));
    window.destroy();
    pump(50);
    assert!(!reopened.is_visible(), "Help closes with parent");

    // Building lock widgets does not acquire a compositor lock (main owns that).
    // Even a config requesting decorations must not expose help or close there.
    for preview in [false, true] {
        let config = Config {
            background_pool: Some(vec![]),
            idle_pool: Some(vec![]),
            ..Default::default()
        };
        let view = ui::build(
            &app,
            Rc::new(ui::Settings {
                theme: Theme::from_config(&config).unwrap(),
                config,
                strings: I18n::new(Some("en-US"), None).unwrap(),
                preview,
                show_keyboard: false,
                start_idle: false,
                username: "test".into(),
            }),
            Rc::new(|_| panic!("Help must not authenticate")),
        );
        assert_eq!(view.window.is_decorated(), preview);
        assert_eq!(
            find(view.window.upcast_ref(), "help-button").is_some(),
            preview
        );
        if preview {
            chrome(view.window.upcast_ref());
            view.window.present();
            pump(50);
            f1(&view.window);
            pump(50);
            assert!(help(view.window.upcast_ref()).is_visible());
        } else {
            assert!(view.window.titlebar().is_none());
            let controls = view.window.observe_controllers();
            assert!(
                !(0..controls.n_items())
                    .filter_map(|i| controls.item(i).and_downcast::<gtk::EventController>())
                    .any(|c| c.name().as_deref() == Some("decklock-help-shortcut"))
            );
        }
        view.window.destroy();
    }
    println!(
        "PASS: F1, bilingual chapters, live language, help reuse/cleanup, window chrome, GUI preference save and lock UI exclusion"
    );
}
