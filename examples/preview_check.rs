//! GUI integration check. Preview only; never authenticates or takes a lock.
use decklock::{
    config::{Config, Theme},
    controller::{ControllerEvent, Side},
    i18n::I18n,
    ui,
};
use gtk::{gio, prelude::*};
use std::{cell::Cell, rc::Rc};

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
    assert_eq!(buttons.len(), 3);
    assert!(buttons.iter().all(|b| !b.is_sensitive()));
    let weak_entry = view.entry.downgrade();
    view.window.destroy();
    drop(view);
    while glib::MainContext::default().iteration(false) {}
    assert!(
        weak_entry.upgrade().is_none(),
        "Destroyed preview retained password entry"
    );
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
