//! Lockout notice on the gate's private display: text, theme hooks, live
//! countdown, and the invariant that it never blocks another attempt.
use decklock::{
    config::{Config, Theme},
    faillock::Advice,
    i18n::I18n,
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

fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.LockoutCheck"),
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
        Rc::new(|_| panic!("A displayed notice must never authenticate")),
    );
    view.window.present();
    pump(50);

    // A plain denial keeps the ordinary message and no theme hook.
    view.deny(&Advice::default());
    assert_eq!(view.status.text(), "Authentication failed. Try again.");
    assert!(!view.status.has_css_class("locked"));
    assert!(!view.status.has_css_class("warning"));

    // A warning explicitly sent by PAM has its own theme hook; no local prediction.
    view.deny(&Advice {
        raw: Some("2 attempts left before the account is locked.".into()),
        ..Default::default()
    });
    assert_eq!(
        view.status.text(),
        "2 attempts left before the account is locked."
    );
    assert!(view.status.has_css_class("warning"));
    assert!(!view.status.has_css_class("locked"));

    // A lockout counts down in place, once per second.
    view.deny(&Advice {
        locked: true,
        locked_for: Some(Duration::from_secs(120)),
        raw: None,
    });
    assert_eq!(
        view.status.text(),
        "Account locked by failed attempts. Estimated wait: 2:00."
    );
    assert!(view.status.has_css_class("locked"));
    assert!(!view.status.has_css_class("warning"));
    // The notice is advice, not a second policy: the user may always try again.
    assert!(
        view.entry.is_sensitive(),
        "Lockout notice disabled the field"
    );
    assert!(view.keyboard.is_sensitive(), "Lockout notice disabled keys");
    pump(1100);
    assert_eq!(
        view.status.text(),
        "Account locked by failed attempts. Estimated wait: 1:59."
    );

    // A later attempt replaces the notice and stops the countdown.
    view.deny(&Advice::default());
    assert_eq!(view.status.text(), "Authentication failed. Try again.");
    assert!(!view.status.has_css_class("locked"));
    pump(1100);
    assert_eq!(view.status.text(), "Authentication failed. Try again.");

    // An exhausted countdown keeps the lockout visible without a fake clock.
    view.deny(&Advice {
        locked: true,
        locked_for: Some(Duration::from_secs(1)),
        ..Default::default()
    });
    pump(1100);
    assert_eq!(
        view.status.text(),
        "Account locked by failed attempts.",
        "Expired countdown must drop the clock, not the warning"
    );
    assert!(view.status.has_css_class("locked"));

    view.deny(&Advice {
        locked: true,
        locked_for: Some(Duration::from_secs(2)),
        ..Default::default()
    });
    // Let real time pass without dispatching any GTK timer callbacks.
    std::thread::sleep(Duration::from_millis(2200));
    pump(100);
    assert_eq!(
        view.status.text(),
        "Account locked by failed attempts.",
        "Delayed callbacks must not prolong the countdown"
    );

    view.deny(&Advice {
        locked: true,
        locked_for: Some(Duration::from_secs(2)),
        ..Default::default()
    });
    view.busy(true, "Authenticating…");
    pump(1100);
    assert_eq!(view.status.text(), "Authenticating…");
    assert!(!view.status.has_css_class("locked"));
    view.busy(false, "");

    view.window.destroy();
    pump(50);
    println!(
        "PASS: plain denial, PAM warning, live countdown, theme hooks, \
         input never blocked, replacement and expiry"
    );
}
