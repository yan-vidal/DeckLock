//! The lock screen states a reduced backend guarantee (#17): the text comes from
//! the catalog, it appears only when a guarantee is missing, and a theme cannot
//! take it off the screen.
use decklock::{
    config::{Config, Theme},
    i18n::I18n,
    lock::Guarantees,
    ui,
};
use gtk::{gio, prelude::*};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while Instant::now() < end && glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Read from the catalog rather than repeated here, so rewording the warning
/// cannot quietly turn this into an assertion that always passes.
fn catalog(key: &str) -> String {
    include_str!("../locales/en-US.ftl")
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key} = ")))
        .unwrap_or_else(|| panic!("the English catalog defines {key}"))
        .to_owned()
}

fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.GuaranteeCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
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
            preview: true,
            show_keyboard: false,
            start_idle: false,
            username: "test".into(),
        }),
        Rc::new(|_| panic!("A guarantee notice must never authenticate")),
    );
    view.window.present();
    pump(50);

    assert!(
        !view.guarantee.is_visible(),
        "nothing is claimed by default"
    );

    // Wayland keeps both guarantees, so the lock screen says nothing about them.
    view.warn_about(&Guarantees {
        survives_process_exit: true,
        isolates_input: true,
    });
    pump(30);
    assert!(
        !view.guarantee.is_visible(),
        "a backend that keeps its guarantees must not warn"
    );

    // X11 keeps neither, and both facts are stated.
    view.warn_about(&Guarantees {
        survives_process_exit: false,
        isolates_input: false,
    });
    pump(50);
    assert!(
        view.guarantee.is_visible(),
        "the reduced guarantee is stated"
    );
    assert_eq!(
        view.guarantee.text(),
        format!(
            "{} {}",
            catalog("guarantee-input-exposed"),
            catalog("guarantee-dies-with-process")
        )
    );
    assert!(view.guarantee.has_css_class("warning"));

    // A theme is CSS at APPLICATION priority; the notice is pinned above it. A
    // stylesheet that tries to make it invisible must not succeed.
    let hostile = gtk::CssProvider::new();
    hostile.load_from_string(
        "#guarantee { color: transparent; font-size: 0px; min-height: 0; min-width: 0; \
         margin: 0; padding: 0; opacity: 0; }",
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &hostile,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    pump(80);
    assert!(
        view.guarantee.color().alpha() > 0.9,
        "a theme made the notice transparent: {:?}",
        view.guarantee.color()
    );
    assert!(
        view.guarantee.height() >= 18,
        "a theme collapsed the notice to {} pixels",
        view.guarantee.height()
    );
    assert!(view.guarantee.is_visible());

    println!("PASS: guarantee notice text, absence on Wayland and resistance to a theme");
}
