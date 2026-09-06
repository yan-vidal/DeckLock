//! GUI integration check. Preview only; never authenticates or takes a lock.
use decklock::{
    config::{Config, Theme},
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
        config: Config::default(),
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
    view.entry.select_region(0, -1);
    key(&view, "⌫").emit_clicked();
    assert_eq!(view.entry.text(), "");
    key(&view, "´").emit_clicked();
    key(&view, "e").emit_clicked();
    assert_eq!(view.entry.text(), "é");
    key(&view, "⌫").emit_clicked();
    assert_eq!(view.entry.text(), "");
    key(&view, "q").emit_clicked();
    key(&view, "↵").emit_clicked();
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
    println!(
        "PASS: clickable keyboard, shift, accents, Unicode deletion, preview isolation, widget cleanup"
    );
}
