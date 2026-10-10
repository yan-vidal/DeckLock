//! Help, language and decorations on the gate's private display; no real lock.
use decklock::{
    config::{Config, Theme},
    greeter::UserEntry,
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
fn press(window: &impl IsA<gtk::Widget>, key: gdk::Key) -> bool {
    let controllers = window.observe_controllers();
    let controller = (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .find(|c| c.name().as_deref() == Some("decklock-help-shortcut"))
        .expect("Help key controller");
    controller.emit_by_name::<bool>("key-pressed", &[&key, &0u32, &gdk::ModifierType::empty()])
}
fn f1(window: &impl IsA<gtk::Widget>) {
    assert!(press(window, gdk::Key::F1));
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
    let help_button = find(window.upcast_ref(), "help-button")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    assert_eq!(
        help_button.label().as_deref(),
        Some("?"),
        "Help is a question mark"
    );
    help_button.emit_clicked();
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

    // Lock, login and preview show a short guide inside their own surface: a
    // session lock displays no other window, so their help never opens one. The
    // full guide stays in settings. Building lock widgets does not acquire a
    // compositor lock (main owns that), and the lock still has no window controls.
    for (greeter, preview, locale, heading) in [
        (false, false, "en-US", "Unlocking"),
        (false, false, "pt-BR", "Como desbloquear"),
        (false, true, "en-US", "Unlocking"),
        (true, false, "en-US", "Signing in"),
        (true, true, "pt-BR", "Como entrar"),
    ] {
        let case = format!("greeter={greeter} preview={preview} {locale}");
        // The guide follows the configured language, as the rest of the screen.
        let config = Config {
            locale: Some(locale.into()),
            background_pool: Some(vec![]),
            idle_pool: Some(vec![]),
            ..Default::default()
        };
        let settings = Rc::new(ui::Settings {
            theme: Theme::from_config(&config).unwrap(),
            config,
            strings: I18n::new(Some(locale), None).unwrap(),
            preview,
            show_keyboard: false,
            start_idle: false,
            username: "alice".into(),
            greeter,
            power_supply: None,
        });
        let view = if greeter {
            ui::build_greeter_with_users(
                &app,
                settings,
                vec![UserEntry {
                    username: "alice".into(),
                    display_name: "alice".into(),
                    uid: 1000,
                    icon_path: None,
                }],
                Rc::new(|_| panic!("Help must not authenticate")),
            )
        } else {
            ui::build(
                &app,
                settings,
                Rc::new(|_| panic!("Help must not authenticate")),
            )
        };
        assert_eq!(view.window.is_decorated(), preview && !greeter, "{case}");
        if !preview {
            assert!(view.window.titlebar().is_none(), "{case}");
        }
        view.window.present();
        pump(50);
        let button = find(view.window.upcast_ref(), "help-button")
            .unwrap_or_else(|| panic!("Help button: {case}"))
            .downcast::<gtk::Button>()
            .unwrap();
        assert_eq!(button.label().as_deref(), Some("?"), "{case}");
        let panel = find(view.window.upcast_ref(), "help-panel")
            .unwrap_or_else(|| panic!("In-screen help: {case}"));
        assert!(!panel.is_visible(), "Help starts closed: {case}");
        let windows = gtk::Window::list_toplevels().len();
        f1(&view.window);
        pump(50);
        assert!(
            panel.is_visible(),
            "F1 shows the help in the screen: {case}"
        );
        assert_eq!(
            gtk::Window::list_toplevels().len(),
            windows,
            "Help must not open another window: {case}"
        );
        let content = text(&panel);
        assert!(content.contains(heading), "Short guide missing: {case}");
        for full in ["Using DeckLock", "Usando o DeckLock", "Inside DeckLock"] {
            assert!(
                !content.contains(full),
                "The full guide stays in settings: {case}"
            );
        }
        // Plain words for whoever is at the screen: no commands or system paths.
        for technical in ["sudo", "/etc/", "/var/", "PAM", "greetd", "systemctl"] {
            assert!(
                !content.contains(technical),
                "Help mentions {technical}: {case}"
            );
        }
        assert!(
            press(&view.window, gdk::Key::Escape),
            "Esc closes the help first: {case}"
        );
        pump(20);
        assert!(!panel.is_visible(), "Esc closes the help: {case}");
        assert!(
            view.window.is_visible(),
            "Esc on help keeps the screen: {case}"
        );
        button.emit_clicked();
        pump(20);
        assert!(panel.is_visible(), "The ? button shows the help: {case}");
        find(&panel, "help-close")
            .unwrap_or_else(|| panic!("Help close button: {case}"))
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        pump(20);
        assert!(
            !panel.is_visible(),
            "The close button hides the help: {case}"
        );
        view.window.destroy();
        pump(30);
    }

    // Rest mode hides the help with the other controls.
    let config = Config {
        background_pool: Some(vec![]),
        idle_pool: Some(vec![]),
        ..Default::default()
    };
    let resting = ui::build(
        &app,
        Rc::new(ui::Settings {
            theme: Theme::from_config(&config).unwrap(),
            config,
            strings: I18n::new(Some("en-US"), None).unwrap(),
            preview: false,
            show_keyboard: false,
            start_idle: true,
            username: "alice".into(),
            greeter: false,
            power_supply: None,
        }),
        Rc::new(|_| panic!("Help must not authenticate")),
    );
    resting.window.present();
    pump(400);
    assert!(
        !find(resting.window.upcast_ref(), "help-button")
            .unwrap()
            .is_visible(),
        "Rest mode hides the help button"
    );
    resting.window.destroy();
    pump(30);
    println!(
        "PASS: F1, bilingual chapters, live language, help reuse/cleanup, window chrome, GUI preference save, and the short in-screen guides of lock, login and preview"
    );
}
