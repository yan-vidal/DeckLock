//! GUI integration check. Preview only; never authenticates or takes a lock.
use decklock::{
    config::{Config, Theme},
    controller::{ControllerEvent, Side},
    i18n::I18n,
    ui,
};
use gtk::{gio, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        found.extend(descendants(&node));
        found.push(node);
    }
    found
}

fn key(view: &ui::View, label: &str) -> gtk::Button {
    descendants(view.keyboard.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("Missing key: {label}"))
}

fn main() {
    gtk::init().expect("Wayland display required for the preview check");
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.PreviewCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let settings = Rc::new(ui::Settings {
        config: Config {
            system_keyboard: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: true,
        start_idle: false,
        username: "Preview".into(),
        greeter: false,
    });
    ui::apply_css(&settings.theme.css).unwrap();
    let submitted = Rc::new(Cell::new(false));
    let flag = submitted.clone();
    let view = ui::build(&app, settings, Rc::new(move |_| flag.set(true)));
    view.window.present();
    while glib::MainContext::default().iteration(false) {}
    assert!(!gtk::prelude::EntryExt::is_visible(&view.entry));
    key(&view, "a").emit_clicked();
    assert_eq!(view.entry.text(), "a");
    key(&view, "⇧").emit_clicked();
    key(&view, "A").emit_clicked();
    assert_eq!(view.entry.text(), "aA");
    key(&view, "⇧").emit_clicked();
    key(&view, "⇧").emit_clicked();
    assert!(view.keyboard.has_css_class("caps-active"));
    let caps_label = descendants(view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "caps")
        .unwrap();
    assert!(caps_label.is_visible());
    key(&view, "A").emit_clicked();
    key(&view, "A").emit_clicked();
    assert_eq!(view.entry.text(), "aAAA");
    key(&view, "⇪").emit_clicked();
    assert!(!caps_label.is_visible());
    assert!(!view.keyboard.has_css_class("caps-active"));
    view.entry.select_region(0, -1);
    key(&view, "←").emit_clicked();
    assert_eq!(view.entry.text(), "");
    key(&view, "´").emit_clicked();
    key(&view, "e").emit_clicked();
    assert_eq!(view.entry.text(), "é");
    key(&view, "←").emit_clicked();
    assert_eq!(view.entry.text(), "");
    key(&view, "Alt").emit_clicked();
    key(&view, "¹").emit_clicked();
    assert_eq!(view.entry.text(), "¹");
    key(&view, "←").emit_clicked();
    key(&view, "q").emit_clicked();
    key(&view, "↲").emit_clicked();
    assert_eq!(view.entry.text(), "");
    assert!(
        !submitted.get(),
        "Preview called the authentication callback"
    );
    assert!(view.status.text().contains("desativada"));
    let power = descendants(view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "power")
        .unwrap();
    let buttons: Vec<_> = descendants(&power)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .collect();
    assert_eq!(buttons.len(), 5);
    for button in &buttons {
        assert!(
            button.is_sensitive(),
            "Preview must show hover and tooltips"
        );
        assert!(button.tooltip_text().unwrap().contains("desativadas"));
        // Preview has no systemctl callback, even though hover is enabled.
        button.emit_clicked();
    }
    assert!(view.window.is_visible());
    let weak_entry = view.entry.downgrade();
    view.window.destroy();
    drop(view);
    while glib::MainContext::default().iteration(false) {}
    assert!(
        weak_entry.upgrade().is_none(),
        "Destroyed preview retained password entry"
    );

    // Verify switch-user button in non-preview mode executes switch_user_command,
    // while in preview mode it must NEVER execute the command.
    let dir = tempfile::tempdir().unwrap();
    let preview_marker = dir.path().join("preview_switched");
    let normal_marker = dir.path().join("normal_switched");

    // 1. Preview mode with custom switch_user_command:
    let preview_settings = Rc::new(ui::Settings {
        config: Config {
            switch_user_command: Some(format!("touch {}", preview_marker.display())),
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "Preview".into(),
        greeter: false,
    });
    let preview_view = ui::build(&app, preview_settings, Rc::new(|_| {}));
    preview_view.window.present();
    while glib::MainContext::default().iteration(false) {}
    let preview_power = descendants(preview_view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "power")
        .unwrap();
    let preview_switch_btn = descendants(&preview_power)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .next()
        .unwrap();
    preview_switch_btn.emit_clicked();
    for _ in 0..10 {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !preview_marker.exists(),
        "Preview mode must never execute switch_user_command"
    );
    preview_view.window.destroy();
    drop(preview_view);
    while glib::MainContext::default().iteration(false) {}

    // 2. Normal mode (lock screen) with custom switch_user_command:
    let normal_settings = Rc::new(ui::Settings {
        config: Config {
            switch_user_command: Some(format!("touch {}", normal_marker.display())),
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: false,
        show_keyboard: false,
        start_idle: false,
        username: "User".into(),
        greeter: false,
    });
    let normal_view = ui::build(&app, normal_settings, Rc::new(|_| {}));
    normal_view.window.present();
    while glib::MainContext::default().iteration(false) {}
    let normal_power = descendants(normal_view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "power")
        .unwrap();
    let normal_switch_btn = descendants(&normal_power)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .next()
        .unwrap();
    normal_switch_btn.emit_clicked();
    let timeout = Instant::now() + Duration::from_secs(2);
    while !normal_marker.exists() && Instant::now() < timeout {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        normal_marker.exists(),
        "Normal lock screen must execute switch_user_command when clicked"
    );
    normal_view.window.destroy();
    drop(normal_view);
    while glib::MainContext::default().iteration(false) {}

    // 3. Greeter mode UI validation:
    let greeter_settings = Rc::new(ui::Settings {
        config: Config::default(),
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "PreviewUser".into(),
        greeter: true,
    });
    let greeter_view = ui::build(&app, greeter_settings, Rc::new(|_| {}));
    greeter_view.window.present();
    while glib::MainContext::default().iteration(false) {}
    assert!(greeter_view.window.title().unwrap().contains("Login"));
    let greeter_power = descendants(greeter_view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "power")
        .unwrap();
    let greeter_buttons: Vec<_> = descendants(&greeter_power)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .collect();
    assert_eq!(
        greeter_buttons.len(),
        4,
        "Greeter action bar must show 4 power buttons and omit switch-user"
    );
    let all_descendants = descendants(greeter_view.window.upcast_ref());
    assert!(
        all_descendants.iter().any(|w| w.widget_name() == "avatar"),
        "Greeter must display avatar widget"
    );
    assert!(
        all_descendants
            .iter()
            .any(|w| w.widget_name() == "session-box"),
        "Greeter must display session-box container when sessions exist"
    );
    assert_eq!(
        greeter_view.submit.tooltip_text().as_deref(),
        Some("Entrar")
    );
    greeter_view.window.destroy();
    drop(greeter_view);
    while glib::MainContext::default().iteration(false) {}
    let settings = Rc::new(ui::Settings {
        config: Config {
            controller_socket: Some("/unused-fake-socket".into()),
            system_keyboard: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: true,
        start_idle: false,
        username: "Preview".into(),
        greeter: false,
    });
    let view = ui::build(&app, settings, Rc::new(|_| panic!("Preview authenticated")));
    view.window.present();
    while glib::MainContext::default().iteration(false) {}
    assert_eq!(key(&view, "a").opacity(), 0.0);
    (view.controller_event)(ControllerEvent::Button {
        name: "LPADTOUCH".into(),
        pressed: true,
    });
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Left,
        x: 0,
        y: 0,
    });
    assert!(key(&view, "f").opacity() > 0.0);
    (view.controller_event)(ControllerEvent::Button {
        name: "LPADTOUCH".into(),
        pressed: false,
    });
    // The daemon reports a final neutral coordinate AFTER touch release.
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Left,
        x: 0,
        y: 0,
    });
    assert_eq!(key(&view, "f").opacity(), 0.0);
    (view.controller_event)(ControllerEvent::Button {
        name: "RPADTOUCH".into(),
        pressed: true,
    });
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Right,
        x: 0,
        y: 0,
    });
    assert!(key(&view, "k").opacity() > 0.0);
    (view.controller_event)(ControllerEvent::Button {
        name: "RPADTOUCH".into(),
        pressed: false,
    });
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Right,
        x: 0,
        y: 0,
    });
    assert_eq!(key(&view, "k").opacity(), 0.0);
    for side in [Side::Left, Side::Right] {
        (view.controller_event)(ControllerEvent::Button {
            name: if side == Side::Left {
                "LPADTOUCH"
            } else {
                "RPADTOUCH"
            }
            .into(),
            pressed: true,
        });
        (view.controller_event)(ControllerEvent::Pad { side, x: 0, y: 0 });
    }
    (view.controller_event)(ControllerEvent::Button {
        name: "LPADTOUCH".into(),
        pressed: false,
    });
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Left,
        x: 0,
        y: 0,
    });
    assert!(
        key(&view, "k").opacity() > 0.0,
        "Other finger should remain visible"
    );
    (view.controller_event)(ControllerEvent::Button {
        name: "RPADTOUCH".into(),
        pressed: false,
    });
    (view.controller_event)(ControllerEvent::Pad {
        side: Side::Right,
        x: 0,
        y: 0,
    });
    for widget in descendants(view.keyboard.upcast_ref()) {
        if widget.has_css_class("key") {
            assert_eq!(widget.opacity(), 0.0);
        }
    }
    (view.controller_event)(ControllerEvent::Trigger {
        side: Side::Left,
        value: 255,
    });
    assert_eq!(
        view.entry.text(),
        "",
        "Released pad must not retain a selected key"
    );
    view.entry.set_text("ab");
    view.entry.set_position(-1);
    (view.controller_event)(ControllerEvent::Button {
        name: "LB".into(),
        pressed: true,
    });
    assert_eq!(view.entry.text(), "a");
    (view.controller_event)(ControllerEvent::Button {
        name: "RB".into(),
        pressed: true,
    });
    assert_eq!(view.entry.text(), "a ");
    (view.controller_event)(ControllerEvent::Button {
        name: "LGRIP".into(),
        pressed: true,
    });
    key(&view, "A").emit_clicked();
    key(&view, "A").emit_clicked();
    assert_eq!(view.entry.text(), "a AA");
    (view.controller_event)(ControllerEvent::Button {
        name: "LGRIP".into(),
        pressed: false,
    });
    key(&view, "a").emit_clicked();
    assert_eq!(view.entry.text(), "a AAa");
    (view.controller_event)(ControllerEvent::Disconnected);
    assert_eq!(key(&view, "a").opacity(), 1.0);
    assert!(!view.keyboard.has_css_class("ghost"));
    let weak_entry = view.entry.downgrade();
    view.window.destroy();
    drop(view);
    while glib::MainContext::default().iteration(false) {}
    assert!(
        weak_entry.upgrade().is_none(),
        "Ghost preview retained entry"
    );
    println!(
        "PASS: clickable keyboard, shift, accents, Unicode deletion, preview isolation, double Shift latch, post-release pad coordinates, ghost opacity, controller bindings, fallback, widget cleanup"
    );
}
