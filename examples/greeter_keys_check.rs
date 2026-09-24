//! Greeter account selection by physical arrow keys, through GTK's own event
//! dispatch. Run by scripts/check under a private Xvfb: XTest reaches the view
//! the way a keyboard (or wtype under Cage) does, which a synthesized GTK
//! signal cannot, because the focused text child sees the key first.
//! Preview only; never authenticates, never contacts greetd.
use decklock::{
    config::{Config, Theme},
    greeter::UserEntry,
    i18n::I18n,
    ui,
};
use gdk4_x11::x11::{xlib, xtest};
use gtk::{gio, prelude::*};
use std::{
    ffi::CString,
    os::raw::c_uint,
    ptr,
    rc::Rc,
    time::{Duration, Instant},
};

struct Keys {
    xlib: xlib::Xlib,
    xtest: xtest::Xf86vmode,
    display: *mut xlib::Display,
}

impl Keys {
    fn open() -> Self {
        let xlib = xlib::Xlib::open().expect("libX11");
        let xtest = xtest::Xf86vmode::open().expect("libXtst");
        let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
        assert!(!display.is_null(), "cannot open the private DISPLAY");
        Self {
            xlib,
            xtest,
            display,
        }
    }

    fn focus(&self, window: xlib::Window) {
        unsafe {
            (self.xlib.XSetInputFocus)(
                self.display,
                window,
                xlib::RevertToParent,
                xlib::CurrentTime,
            );
            (self.xlib.XSync)(self.display, xlib::False);
        }
    }

    fn press(&self, keysym: &str) {
        let name = CString::new(keysym).unwrap();
        unsafe {
            let code = (self.xlib.XKeysymToKeycode)(
                self.display,
                (self.xlib.XStringToKeysym)(name.as_ptr()),
            ) as c_uint;
            assert_ne!(code, 0, "No keycode for {keysym}");
            (self.xtest.XTestFakeKeyEvent)(self.display, code, xlib::True, 0);
            (self.xtest.XTestFakeKeyEvent)(self.display, code, xlib::False, 0);
            (self.xlib.XSync)(self.display, xlib::False);
        }
    }
}

fn user(name: &str, uid: u32) -> UserEntry {
    UserEntry {
        username: name.into(),
        display_name: name.into(),
        uid,
        icon_path: None,
    }
}

/// Dispatch a bounded batch of GTK work. The animated background always has
/// another frame pending, so waiting for an idle main context never ends.
fn pump() {
    for _ in 0..64 {
        if !glib::MainContext::default().iteration(false) {
            break;
        }
    }
}

/// Pump GTK until the condition holds, bounded so a lost event fails instead
/// of hanging the gate.
fn settle(label: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "Timed out: {label}");
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let any further handler for the same key run before the next assertion.
    for _ in 0..10 {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: set before GTK starts any thread; the saved selection must come
    // from this fixture, never from the developer's state.
    unsafe {
        std::env::set_var("DECKLOCK_GREETER_STATE", dir.path().join("state.toml"));
    }
    gtk::init().expect("Private X11 display required for the greeter key check");
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.GreeterKeysCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let settings = Rc::new(ui::Settings {
        // No background media: key dispatch must not wait on video frames.
        config: Config {
            system_keyboard: false,
            background_pool: Some(vec![]),
            idle_pool: Some(vec![]),
            idle_enabled: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("en-US"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "alice".into(),
        greeter: true,
    });
    ui::apply_css(&settings.theme.css).unwrap();
    let view = ui::build_greeter_with_users(
        &app,
        settings,
        vec![user("alice", 1000), user("bob", 1001), user("carol", 1002)],
        Rc::new(|_| panic!("Arrow keys must never submit")),
    );
    view.window.present();
    settle("greeter window mapped", || view.window.is_mapped());
    let xid = view
        .window
        .surface()
        .and_downcast::<gdk4_x11::X11Surface>()
        .expect("GDK_BACKEND=x11 is required")
        .xid();
    let keys = Keys::open();
    keys.focus(xid);
    view.entry.grab_focus();
    settle("greeter window focused", || view.window.is_active());
    let selected = || view.selected_user.borrow().clone();
    assert_eq!(selected(), "alice");

    // Keys reach the password entry: this is the control for every step below.
    keys.press("x");
    settle("typed key reaches the password entry", || {
        view.entry.text() == "x"
    });
    // With text present the arrows belong to the entry's cursor.
    settle("cursor after typed key", || view.entry.position() == 1);
    keys.press("Left");
    settle("Left moves the cursor in a non-empty password", || {
        view.entry.position() == 0
    });
    keys.press("Right");
    settle("Right moves the cursor in a non-empty password", || {
        view.entry.position() == 1
    });
    assert_eq!(selected(), "alice", "Arrows switched account while editing");
    assert_eq!(view.entry.text(), "x");
    keys.press("BackSpace");
    settle("password cleared", || view.entry.text().is_empty());

    keys.press("Right");
    settle("Right selects the next account", || selected() == "bob");
    keys.press("Left");
    settle("Left selects the previous account", || {
        selected() == "alice"
    });
    keys.press("Left");
    settle("Left wraps to the last account", || selected() == "carol");
    keys.press("Right");
    settle("Right wraps to the first account", || selected() == "alice");
    // GTK focuses the entry's text child, so check the window's focus widget.
    assert!(
        gtk::prelude::RootExt::focus(&view.window).is_some_and(|w| w
            == *view.entry.upcast_ref::<gtk::Widget>()
            || w.is_ancestor(&view.entry)),
        "Password entry must keep focus"
    );
    view.window.destroy();
    println!("PASS: greeter arrow keys select accounts through GTK dispatch");
}
