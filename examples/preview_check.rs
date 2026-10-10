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
    os::unix::fs::PermissionsExt,
    path::Path,
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

fn check_adaptive_controls(app: &gtk::Application) {
    for (width, height, factor) in [(1280, 800, 1.0_f32), (800, 500, 0.625_f32)] {
        let view = ui::build(
            app,
            Rc::new(ui::Settings {
                config: Config {
                    system_keyboard: false,
                    window_decorations: false,
                    background_pool: Some(vec![]),
                    ..Config::default()
                },
                theme: Theme::load(None).unwrap(),
                strings: I18n::new(Some("pt-BR"), None).unwrap(),
                preview: true,
                show_keyboard: false,
                start_idle: false,
                username: "Fixed fixture".into(),
                greeter: false,
                power_supply: None,
            }),
            Rc::new(|_| panic!("Adaptive preview must never authenticate")),
        );
        view.window.set_titlebar(None::<&gtk::Widget>);
        view.window.set_decorated(false);
        view.window.set_default_size(width, height);
        if width == 1280 {
            view.window.add_tick_callback(|window, _| {
                window.fullscreen();
                glib::ControlFlow::Break
            });
        }
        view.window.present();
        let deadline = Instant::now() + Duration::from_secs(3);
        while (view.window.width() != width || view.window.height() != height)
            && Instant::now() < deadline
        {
            for _ in 0..100 {
                if !glib::MainContext::default().iteration(false) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!((view.window.width(), view.window.height()), (width, height));
        let named = |name: &str| {
            descendants(view.window.upcast_ref())
                .into_iter()
                .find(|widget| widget.widget_name() == name)
                .unwrap()
        };
        let avatar = named("avatar");
        let bounds = avatar.compute_bounds(&view.window).unwrap();
        assert!(
            (bounds.width() - 108.0 * factor).abs() < 1.0,
            "Controls must fit {width}x{height}: avatar width {} instead of {}",
            bounds.width(),
            108.0 * factor
        );
        let background = named("background").compute_bounds(&view.window).unwrap();
        assert_eq!(
            (
                background.width().round() as i32,
                background.height().round() as i32
            ),
            (width, height),
            "Media must keep filling the entire surface"
        );
        view.keyboard.set_visible(true);
        let button = key(&view, "a");
        let deadline = Instant::now() + Duration::from_secs(2);
        while button.width() == 0 && Instant::now() < deadline {
            for _ in 0..100 {
                if !glib::MainContext::default().iteration(false) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let center = button
            .compute_point(
                &view.window,
                &gtk::graphene::Point::new(
                    button.width() as f32 / 2.0,
                    button.height() as f32 / 2.0,
                ),
            )
            .unwrap();
        let picked = view
            .window
            .pick(
                center.x() as f64,
                center.y() as f64,
                gtk::PickFlags::DEFAULT,
            )
            .unwrap();
        assert!(
            picked == button || picked.is_ancestor(&button),
            "Scaled keyboard lost pointer hit testing"
        );
        button.emit_clicked();
        assert_eq!(view.entry.text(), "a");
        let weak_entry = view.entry.downgrade();
        view.window.destroy();
        drop(view);
        drop(button);
        drop(picked);
        drop(avatar);
        for _ in 0..100 {
            if !glib::MainContext::default().iteration(false) {
                break;
            }
        }
        assert!(
            weak_entry.upgrade().is_none(),
            "Adaptive controls retained a destroyed view"
        );
    }
}

fn pump_for(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Battery level from a fixed power-supply fixture, never the host's battery.
fn check_battery(app: &gtk::Application) {
    let supplies = tempfile::tempdir().unwrap();
    let supply = |name: &str, fields: &[(&str, &str)]| {
        let path = supplies.path().join(name);
        std::fs::create_dir(&path).unwrap();
        for (field, value) in fields {
            std::fs::write(path.join(field), format!("{value}\n")).unwrap();
        }
    };
    supply("ACAD", &[("type", "Mains"), ("online", "0")]);
    supply(
        "BAT1",
        &[
            ("type", "Battery"),
            ("present", "1"),
            ("capacity", "67"),
            ("status", "Discharging"),
        ],
    );
    // A controller reports its own battery; it is not the system's.
    supply(
        "controller-battery",
        &[
            ("type", "Battery"),
            ("scope", "Device"),
            ("capacity", "5"),
            ("status", "Discharging"),
        ],
    );
    let no_battery = tempfile::tempdir().unwrap();
    for (greeter, root, start_idle, expected) in [
        (true, Some(supplies.path()), false, Some("67%")),
        (false, Some(supplies.path()), false, Some("67%")),
        (true, Some(no_battery.path()), false, None),
        (false, None, false, None),
        // Rest mode hides the corner controls, the battery with them.
        (false, Some(supplies.path()), true, None),
    ] {
        let view = ui::build(
            app,
            Rc::new(ui::Settings {
                config: Config {
                    system_keyboard: false,
                    window_decorations: false,
                    background_pool: Some(vec![]),
                    idle_pool: Some(vec![]),
                    ..Config::default()
                },
                theme: Theme::load(None).unwrap(),
                strings: I18n::new(Some("pt-BR"), None).unwrap(),
                preview: true,
                show_keyboard: false,
                start_idle,
                username: "Fixed fixture".into(),
                greeter,
                power_supply: root.map(Path::to_path_buf),
            }),
            Rc::new(|_| panic!("Battery check must never authenticate")),
        );
        view.window.set_titlebar(None::<&gtk::Widget>);
        view.window.set_decorated(false);
        view.window.set_default_size(1280, 800);
        view.window.present();
        pump_for(400);
        let widgets = descendants(view.window.upcast_ref());
        let battery = widgets.iter().find(|w| w.widget_name() == "battery");
        let case = format!("greeter={greeter} root={root:?} idle={start_idle}");
        match expected {
            None => assert!(
                !battery.is_some_and(|b| b.is_visible()),
                "No battery level expected: {case}"
            ),
            Some(level) => {
                let battery = battery.expect("Battery level").clone();
                assert!(battery.is_visible(), "Battery level hidden: {case}");
                let label = descendants(&battery)
                    .into_iter()
                    .find_map(|w| w.downcast::<gtk::Label>().ok())
                    .expect("Battery percentage");
                assert_eq!(label.text(), level, "{case}");
                let power = widgets.iter().find(|w| w.widget_name() == "power").unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                while (battery.width() == 0 || power.width() == 0) && Instant::now() < deadline {
                    pump_for(10);
                }
                let b = battery.compute_bounds(&view.window).unwrap();
                let p = power.compute_bounds(&view.window).unwrap();
                assert!(
                    b.y() + b.height() <= p.y(),
                    "Battery must sit above the power buttons: {case}"
                );
                let buttons_right = descendants(power)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .filter_map(|button| button.compute_bounds(&view.window))
                    .map(|bounds| bounds.x() + bounds.width())
                    .fold(0.0_f32, f32::max);
                assert!(
                    (b.x() + b.width() - buttons_right).abs() <= 1.0,
                    "Battery must align with the right edge of the power buttons: {case}"
                );
                assert!(
                    b.y() < 100.0 && b.x() > 640.0,
                    "Battery must be in the top-right corner: {case}"
                );
            }
        }
        view.window.destroy();
        pump_for(30);
    }

    // Themes style the battery through CSS: state classes and the icon size.
    let low = tempfile::tempdir().unwrap();
    let charging = tempfile::tempdir().unwrap();
    for (root, status) in [(&low, "Discharging"), (&charging, "Charging")] {
        let path = root.path().join("BAT0");
        std::fs::create_dir(&path).unwrap();
        for (field, value) in [("type", "Battery"), ("capacity", "15"), ("status", status)] {
            std::fs::write(path.join(field), format!("{value}\n")).unwrap();
        }
    }
    // The layout can show the icon alone; the tooltip still gives the level.
    let icon_only = ui::build(
        app,
        Rc::new(ui::Settings {
            config: Config {
                system_keyboard: false,
                window_decorations: false,
                background_pool: Some(vec![]),
                idle_pool: Some(vec![]),
                ..Config::default()
            },
            theme: Theme {
                layout: decklock::config::Layout {
                    battery_percentage: false,
                    ..decklock::config::Layout::default()
                },
                ..Theme::load(None).unwrap()
            },
            strings: I18n::new(Some("pt-BR"), None).unwrap(),
            preview: true,
            show_keyboard: false,
            start_idle: false,
            username: "Fixed fixture".into(),
            greeter: false,
            power_supply: Some(supplies.path().to_path_buf()),
        }),
        Rc::new(|_| panic!("Battery check must never authenticate")),
    );
    icon_only.window.present();
    pump_for(300);
    let battery = descendants(icon_only.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "battery")
        .expect("Battery level");
    assert!(battery.is_visible(), "Icon-only battery stays visible");
    let parts = descendants(&battery);
    assert!(
        parts
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Image>())
            .any(|icon| icon.is_visible()),
        "Icon-only battery keeps its icon"
    );
    assert!(
        !parts
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Label>())
            .any(|label| label.is_visible()),
        "Icon-only battery hides the percentage"
    );
    assert!(
        battery
            .tooltip_text()
            .is_some_and(|tip| tip.contains("67%")),
        "The tooltip keeps the level when the percentage is hidden"
    );
    icon_only.window.destroy();
    pump_for(30);

    let display = gtk::gdk::Display::default().unwrap();
    for (root, classes, theme_rule, icon_size) in [
        (supplies.path(), [false, false], None, 16),
        (low.path(), [true, false], None, 16),
        (charging.path(), [false, true], None, 16),
        (
            supplies.path(),
            [false, false],
            Some("#battery image { -gtk-icon-size: 32px; }"),
            32,
        ),
    ] {
        let provider = theme_rule.map(|rule| {
            let provider = gtk::CssProvider::new();
            provider.load_from_string(rule);
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        let view = ui::build(
            app,
            Rc::new(ui::Settings {
                config: Config {
                    system_keyboard: false,
                    window_decorations: false,
                    background_pool: Some(vec![]),
                    idle_pool: Some(vec![]),
                    ..Config::default()
                },
                theme: Theme::load(None).unwrap(),
                strings: I18n::new(Some("pt-BR"), None).unwrap(),
                preview: true,
                show_keyboard: false,
                start_idle: false,
                username: "Fixed fixture".into(),
                greeter: false,
                power_supply: Some(root.to_path_buf()),
            }),
            Rc::new(|_| panic!("Battery check must never authenticate")),
        );
        view.window.set_default_size(1280, 800);
        view.window.present();
        pump_for(300);
        let battery = descendants(view.window.upcast_ref())
            .into_iter()
            .find(|w| w.widget_name() == "battery")
            .expect("Battery level");
        let case = format!("root={root:?} rule={theme_rule:?}");
        assert_eq!(
            [
                battery.has_css_class("low"),
                battery.has_css_class("charging")
            ],
            classes,
            "Battery state classes: {case}"
        );
        let icon = descendants(&battery)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::Image>().ok())
            .expect("Battery icon");
        let deadline = Instant::now() + Duration::from_secs(2);
        while icon.width() == 0 && Instant::now() < deadline {
            pump_for(10);
        }
        assert_eq!(
            icon.width(),
            icon_size,
            "Battery icon size follows the theme CSS: {case}"
        );
        view.window.destroy();
        pump_for(30);
        if let Some(provider) = provider {
            gtk::style_context_remove_provider_for_display(&display, &provider);
        }
    }
}

fn main() {
    gtk::init().expect("Wayland display required for the preview check");
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.PreviewCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    ui::apply_css(&Theme::load(None).unwrap().css).unwrap();
    check_adaptive_controls(&app);
    check_battery(&app);
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
        power_supply: None,
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
        power_supply: None,
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
        power_supply: None,
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
        config: Config {
            controller_socket: Some("/unused/greeter-fake.socket".into()),
            system_keyboard: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "PreviewUser".into(),
        greeter: true,
        power_supply: None,
    });
    let greeter_view = ui::build(&app, greeter_settings, Rc::new(|_| {}));
    assert!(
        !greeter_view.window.is_fullscreen(),
        "Login fullscreen must wait for its initial surface frame"
    );
    greeter_view.window.present();
    let fullscreen_deadline = Instant::now() + Duration::from_secs(2);
    while !greeter_view.window.is_fullscreen() && Instant::now() < fullscreen_deadline {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(greeter_view.window.title().unwrap().contains("Login"));
    assert!(
        !greeter_view.window.is_decorated(),
        "Login must never show a close button"
    );
    assert!(
        greeter_view.window.is_fullscreen(),
        "Login must request fullscreen"
    );
    // The login keyboard uses the same pad geometry and releases as the locker.
    // Inject events at the GTK boundary; never connect to a physical daemon.
    greeter_view.keyboard.set_visible(true);
    let allocation_deadline = Instant::now() + Duration::from_secs(2);
    while key(&greeter_view, "d").width() == 0 && Instant::now() < allocation_deadline {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        key(&greeter_view, "d").width() > 0,
        "Login keyboard never received a GTK allocation"
    );
    for (side, name) in [(Side::Left, "LPADTOUCH"), (Side::Right, "RPADTOUCH")] {
        (greeter_view.controller_event)(ControllerEvent::Button {
            name: name.into(),
            pressed: true,
        });
        (greeter_view.controller_event)(ControllerEvent::Pad { side, x: 0, y: 0 });
        (greeter_view.controller_event)(ControllerEvent::Trigger { side, value: 255 });
        (greeter_view.controller_event)(ControllerEvent::Trigger { side, value: 0 });
        (greeter_view.controller_event)(ControllerEvent::Button {
            name: name.into(),
            pressed: false,
        });
    }
    assert_eq!(
        greeter_view.entry.text(),
        "dk",
        "Both login pads must type independently"
    );
    (greeter_view.controller_event)(ControllerEvent::Disconnected);
    assert!(!greeter_view.keyboard.has_css_class("ghost"));
    key(&greeter_view, "a").emit_clicked();
    assert_eq!(
        greeter_view.entry.text(),
        "dka",
        "Login keeps a clickable fallback after disconnect"
    );
    (greeter_view.controller_event)(ControllerEvent::Connected);
    assert!(greeter_view.keyboard.has_css_class("ghost"));
    // On a Steam Deck the A button is Enter. While the keyboard holds the
    // controller, DeckLock receives A itself, so A must submit like Enter.
    for pressed in [true, false] {
        (greeter_view.controller_event)(ControllerEvent::Button {
            name: "A".into(),
            pressed,
        });
    }
    assert_eq!(
        greeter_view.status.text(),
        I18n::new(Some("pt-BR"), None)
            .unwrap()
            .text("preview-greeter-notice"),
        "A must submit the login while the keyboard holds the controller"
    );
    assert!(
        greeter_view.entry.text().is_empty(),
        "A must submit the typed password"
    );
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
    // A private executable proves that clicking the standalone greeter preview
    // cannot dispatch a real power action. Never place host systemctl in PATH.
    let fake_bin = dir.path().join("fake-bin");
    std::fs::create_dir(&fake_bin).unwrap();
    let fake_systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &fake_systemctl,
        "#!/bin/sh\nprintf called > \"$DECKLOCK_ACTION_MARKER\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake_systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let action_marker = dir.path().join("greeter_power_called");
    let original_path = std::env::var_os("PATH");
    // SAFETY: this standalone GTK contract is single-process and restores its
    // environment before running the remaining checks.
    unsafe {
        std::env::set_var("PATH", &fake_bin);
        std::env::set_var("DECKLOCK_ACTION_MARKER", &action_marker);
    }
    for button in &greeter_buttons {
        button.emit_clicked();
    }
    for _ in 0..10 {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !action_marker.exists(),
        "Greeter preview ran a power action"
    );
    // SAFETY: same single-process test, restoring the inherited search path.
    unsafe {
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        }
        std::env::remove_var("DECKLOCK_ACTION_MARKER");
    }
    let all_descendants = descendants(greeter_view.window.upcast_ref());
    assert!(
        all_descendants.iter().any(|w| w.widget_name() == "avatar"),
        "Greeter must display avatar widget"
    );
    assert!(
        !all_descendants
            .iter()
            .any(|w| w.widget_name() == "preview-banner"),
        "Login must not show a title over the background"
    );
    let login_help = all_descendants
        .iter()
        .find(|w| w.widget_name() == "help-button")
        .expect("Login help button")
        .clone()
        .downcast::<gtk::Button>()
        .unwrap();
    assert_eq!(
        login_help.label().as_deref(),
        Some("?"),
        "Login help is a question mark"
    );
    assert_eq!(
        (login_help.halign(), login_help.valign()),
        (gtk::Align::Start, gtk::Align::Start)
    );
    assert!(
        login_help.margin_top() <= 12,
        "Login help takes the top-left corner the title used to share"
    );
    if decklock::greeter::list_system_users().len() > 1 {
        assert!(
            all_descendants
                .iter()
                .any(|w| w.widget_name() == "user-carousel"),
            "Greeter must display user-carousel container when multiple users exist"
        );
        let prev_avatar = all_descendants
            .iter()
            .find(|w| w.widget_name() == "avatar-prev")
            .unwrap()
            .clone()
            .downcast::<gtk::Button>()
            .expect("avatar-prev must be a clickable Button");
        let next_avatar = all_descendants
            .iter()
            .find(|w| w.widget_name() == "avatar-next")
            .unwrap()
            .clone()
            .downcast::<gtk::Button>()
            .expect("avatar-next must be a clickable Button");
        let username_label = all_descendants
            .iter()
            .find(|w| w.widget_name() == "username")
            .unwrap()
            .clone()
            .downcast::<gtk::Label>()
            .expect("username must be a Label");
        let initial_name = username_label.text();
        next_avatar.emit_clicked();
        while glib::MainContext::default().iteration(false) {}
        assert_ne!(
            username_label.text(),
            initial_name,
            "Clicking next avatar must switch user"
        );
        prev_avatar.emit_clicked();
        while glib::MainContext::default().iteration(false) {}
        assert_eq!(
            username_label.text(),
            initial_name,
            "Clicking prev avatar must switch user back"
        );
    }
    let session_box = all_descendants
        .iter()
        .find(|w| w.widget_name() == "session-box")
        .expect("Greeter must display session-box container when sessions exist");
    assert_eq!(
        session_box.valign(),
        gtk::Align::End,
        "Session box must be placed at the bottom footer"
    );
    assert_eq!(
        greeter_view.submit.tooltip_text().as_deref(),
        Some("Entrar")
    );
    greeter_view.window.destroy();
    drop(greeter_view);
    while glib::MainContext::default().iteration(false) {}

    // Exercise the GTK switch button through a fake logind. A greeter is
    // identified by its Class property, independent of list-sessions columns.
    let fake_loginctl = fake_bin.join("loginctl");
    std::fs::write(
        &fake_loginctl,
        "#!/bin/sh\ncase \"$1\" in\n  list-sessions) printf 'c1 1000 locktest seat0 tty1\\nc2 986 greeter seat0 tty2\\n' ;;\n  show-session) if [ \"$2\" = c2 ]; then printf 'greeter\\n'; else printf 'user\\n'; fi ;;\n  activate) printf '%s' \"$2\" > \"$DECKLOCK_ACTION_MARKER\" ;;\n  *) exit 91 ;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake_loginctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let switch_marker = dir.path().join("activated_greeter");
    let original_path = std::env::var_os("PATH");
    // SAFETY: this standalone process restores the search path after the probe.
    unsafe {
        std::env::set_var("PATH", &fake_bin);
        std::env::set_var("DECKLOCK_ACTION_MARKER", &switch_marker);
    }
    let switch_view = ui::build(
        &app,
        Rc::new(ui::Settings {
            config: Config::default(),
            theme: Theme::load(None).unwrap(),
            strings: I18n::new(Some("en-US"), None).unwrap(),
            preview: false,
            show_keyboard: false,
            start_idle: false,
            username: "locktest".into(),
            greeter: false,
            power_supply: None,
        }),
        Rc::new(|_| {}),
    );
    switch_view.window.present();
    let power = descendants(switch_view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "power")
        .unwrap();
    let switch_button = descendants(&power)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .next()
        .unwrap();
    switch_button.emit_clicked();
    let timeout = Instant::now() + Duration::from_secs(2);
    while !switch_marker.exists() && Instant::now() < timeout {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(&switch_marker).unwrap(), "c2");
    std::fs::write(
        &fake_loginctl,
        "#!/bin/sh\ncase \"$1\" in\n  list-sessions) printf 'c2 986 greeter seat0 tty2\\n' ;;\n  show-session) printf 'greeter\\n' ;;\n  activate) exit 42 ;;\n  *) exit 91 ;;\nesac\n",
    )
    .unwrap();
    switch_button.emit_clicked();
    assert!(
        switch_view
            .status
            .text()
            .contains("Failed to activate greeter session")
    );
    std::fs::write(
        &fake_loginctl,
        "#!/bin/sh\ncase \"$1\" in\n  list-sessions) printf 'c1 1000 locktest seat0 tty1\\n' ;;\n  show-session) printf 'user\\n' ;;\n  *) exit 91 ;;\nesac\n",
    )
    .unwrap();
    switch_button.emit_clicked();
    assert!(
        switch_view
            .status
            .text()
            .contains("No active greeter session")
    );
    switch_view.window.destroy();
    drop(switch_view);
    unsafe {
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        }
        std::env::remove_var("DECKLOCK_ACTION_MARKER");
    }
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
        power_supply: None,
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
    // Selection is the key under the pad's point, not the one the fade lights up: a
    // centred left pad lands on d and a centred right pad on k.
    let type_with_both_pads = || {
        for (side, name) in [(Side::Left, "LPADTOUCH"), (Side::Right, "RPADTOUCH")] {
            (view.controller_event)(ControllerEvent::Button {
                name: name.into(),
                pressed: true,
            });
            (view.controller_event)(ControllerEvent::Pad { side, x: 0, y: 0 });
            // An earlier check leaves the left trigger held, so release before pulling.
            (view.controller_event)(ControllerEvent::Trigger { side, value: 0 });
            (view.controller_event)(ControllerEvent::Trigger { side, value: 255 });
            (view.controller_event)(ControllerEvent::Trigger { side, value: 0 });
        }
    };
    type_with_both_pads();
    assert_eq!(view.entry.text(), "a AAadk");
    (view.controller_event)(ControllerEvent::Disconnected);
    assert_eq!(key(&view, "a").opacity(), 1.0);
    assert!(!view.keyboard.has_css_class("ghost"));
    (view.controller_event)(ControllerEvent::Connected);
    assert!(
        view.keyboard.has_css_class("ghost"),
        "reconnected keyboard must restore pad geometry"
    );
    assert_eq!(key(&view, "a").opacity(), 0.0);
    type_with_both_pads();
    assert_eq!(
        view.entry.text(),
        "a AAadkdk",
        "both pads must select the same keys after reconnect as before the outage"
    );
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
