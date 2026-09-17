//! DISPOSABLE PROBE for #16: can a GTK4 window become an X11 lock surface?
//!
//! Realize the window (the X window exists but is unmapped), set
//! override-redirect through Xlib on GDK's own connection, then map it and
//! grab keyboard and pointer from that same connection so GTK keeps
//! receiving input. Every observation is printed as a `LOCK` line.
//! Not product code: it never authenticates; typing "secret" ends it.
//!
//! Defaults are the combination that passed. Variants reproduce what failed:
//!   PROBE_RAISE=substructure|visibility|timer|none  keep the lock on top
//!   PROBE_FOCUS=reassert|keep                       take X focus back on FocusOut
//!   PROBE_KB_OWNER=1|0                              owner_events of the keyboard grab
//!   PROBE_OVERRIDE=1|0, PROBE_GRAB=1|0, PROBE_REGRAB=0|1, PROBE_TIMEOUT=seconds

use gdk4_x11::{X11Display, X11Surface, x11::xlib};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::Cell,
    mem,
    os::raw::{c_int, c_uint},
    ptr,
    rc::Rc,
    time::{Duration, Instant},
};

struct X {
    xlib: xlib::Xlib,
    dpy: *mut xlib::Display,
    root: xlib::Window,
}

fn flag(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn grab_name(code: c_int) -> &'static str {
    match code {
        xlib::GrabSuccess => "GrabSuccess",
        xlib::AlreadyGrabbed => "AlreadyGrabbed",
        xlib::GrabInvalidTime => "GrabInvalidTime",
        xlib::GrabNotViewable => "GrabNotViewable",
        xlib::GrabFrozen => "GrabFrozen",
        _ => "unknown",
    }
}

impl X {
    fn children(&self, window: xlib::Window) -> (xlib::Window, Vec<xlib::Window>) {
        let (mut root, mut parent, mut list, mut count) = (0, 0, ptr::null_mut(), 0 as c_uint);
        unsafe {
            (self.xlib.XQueryTree)(
                self.dpy,
                window,
                &mut root,
                &mut parent,
                &mut list,
                &mut count,
            );
            let children = if list.is_null() {
                Vec::new()
            } else {
                let v = std::slice::from_raw_parts(list, count as usize).to_vec();
                (self.xlib.XFree)(list.cast());
                v
            };
            (parent, children)
        }
    }

    /// Children of root are returned bottom-to-top. Unmapped windows (such as
    /// a hidden popover's X window) sit in the stack too but cover nothing.
    fn topmost(&self) -> xlib::Window {
        self.children(self.root)
            .1
            .into_iter()
            .rev()
            .find(|w| self.attributes(*w).map_state == xlib::IsViewable)
            .unwrap_or(0)
    }

    /// Topmost viewable window that belongs to another client, if it is above
    /// every lock window. Our own popups (GDK knows their XIDs) do not count.
    fn covering(&self, display: &X11Display, locks: &[xlib::Window]) -> Option<xlib::Window> {
        let top = self.topmost();
        (!locks.contains(&top) && X11Surface::lookup_for_display(display, top).is_none())
            .then_some(top)
    }

    fn focus(&self) -> xlib::Window {
        let (mut focus, mut revert) = (0, 0);
        unsafe { (self.xlib.XGetInputFocus)(self.dpy, &mut focus, &mut revert) };
        focus
    }

    fn attributes(&self, window: xlib::Window) -> xlib::XWindowAttributes {
        unsafe {
            let mut attrs: xlib::XWindowAttributes = mem::zeroed();
            (self.xlib.XGetWindowAttributes)(self.dpy, window, &mut attrs);
            attrs
        }
    }

    fn grab_keyboard(&self, window: xlib::Window) -> c_int {
        unsafe {
            (self.xlib.XGrabKeyboard)(
                self.dpy,
                window,
                if flag("PROBE_KB_OWNER", "1") == "1" {
                    xlib::True
                } else {
                    xlib::False
                },
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            )
        }
    }

    fn grab_pointer(&self, window: xlib::Window) -> c_int {
        unsafe {
            (self.xlib.XGrabPointer)(
                self.dpy,
                window,
                xlib::True,
                (xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask)
                    as c_uint,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                window,
                0,
                xlib::CurrentTime,
            )
        }
    }
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.decklock.X11LockProbe")
        .build();
    app.connect_activate(activate);
    app.run_with_args::<&str>(&[])
}

fn activate(app: &gtk::Application) {
    let display = gdk::Display::default()
        .and_downcast::<X11Display>()
        .expect("not running on GDK's X11 backend");
    let xlib = xlib::Xlib::open().expect("libX11");
    let dpy = unsafe { display.xdisplay() };
    let x = Rc::new(X {
        root: display.xrootwindow(),
        xlib,
        dpy,
    });
    println!(
        "LOCK start override={} grab={} kb_owner_events={} focus={} raise={} regrab={}",
        flag("PROBE_OVERRIDE", "1"),
        flag("PROBE_GRAB", "1"),
        flag("PROBE_KB_OWNER", "1"),
        flag("PROBE_FOCUS", "reassert"),
        flag("PROBE_RAISE", "substructure"),
        flag("PROBE_REGRAB", "0")
    );

    let monitors = display.monitors();
    let mut xids = Vec::new();
    for index in 0..monitors.n_items() {
        let monitor = monitors.item(index).and_downcast::<gdk::Monitor>().unwrap();
        xids.push(lock_monitor(app, &x, &monitor, index == 0));
    }
    let xids = Rc::new(xids);

    if flag("PROBE_RAISE", "substructure") == "visibility" {
        let (x, xids) = (x.clone(), xids.clone());
        unsafe {
            display.connect_xevent(move |_, event| {
                if (*event).get_type() == xlib::VisibilityNotify {
                    let visibility = (*event).visibility;
                    if xids.contains(&visibility.window) {
                        println!(
                            "LOCK visibility window=0x{:x} state={}",
                            visibility.window, visibility.state
                        );
                        if visibility.state != xlib::VisibilityUnobscured {
                            (x.xlib.XRaiseWindow)(x.dpy, visibility.window);
                            println!("LOCK raised-on-visibility");
                        }
                    }
                }
                glib::Propagation::Proceed
            });
        }
    }
    if flag("PROBE_RAISE", "substructure") == "substructure" {
        // SubstructureNotify on root is shareable (only SubstructureRedirect is
        // exclusive to the window manager). OR it into GDK's own root mask.
        unsafe {
            let mask = x.attributes(x.root).your_event_mask | xlib::SubstructureNotifyMask;
            (x.xlib.XSelectInput)(x.dpy, x.root, mask);
        }
        let (x, xids) = (x.clone(), xids.clone());
        unsafe {
            display.connect_xevent(move |display, event| {
                let kind = (*event).get_type();
                if kind == xlib::MapNotify || kind == xlib::ConfigureNotify {
                    if let Some(top) = x.covering(display, &xids) {
                        for xid in xids.iter() {
                            (x.xlib.XRaiseWindow)(x.dpy, *xid);
                        }
                        println!(
                            "LOCK raised-on-substructure kind={kind} above=0x{top:x} focus=0x{:x}",
                            x.focus()
                        );
                    }
                }
                if kind == xlib::FocusOut && xids.contains(&(*event).focus_change.window) {
                    println!(
                        "LOCK focus-out window=0x{:x} focus-now=0x{:x}",
                        (*event).focus_change.window,
                        x.focus()
                    );
                    if flag("PROBE_FOCUS", "reassert") == "reassert" {
                        (x.xlib.XSetInputFocus)(
                            x.dpy,
                            xids[0],
                            xlib::RevertToParent,
                            xlib::CurrentTime,
                        );
                        println!("LOCK focus-reasserted");
                    }
                }
                glib::Propagation::Proceed
            });
        }
    }
    if flag("PROBE_RAISE", "substructure") == "timer" {
        let (x, xids, display) = (x.clone(), xids.clone(), display.clone());
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if let Some(top) = x.covering(&display, &xids) {
                for xid in xids.iter() {
                    unsafe { (x.xlib.XRaiseWindow)(x.dpy, *xid) };
                }
                println!("LOCK raised-on-timer above=0x{top:x}");
            }
            glib::ControlFlow::Continue
        });
    }

    let seconds: u64 = flag("PROBE_TIMEOUT", "60").parse().unwrap();
    let app = app.downgrade();
    glib::timeout_add_local_once(Duration::from_secs(seconds), move || {
        println!("LOCK timeout");
        if let Some(app) = app.upgrade() {
            app.quit();
        }
    });
}

fn lock_monitor(
    app: &gtk::Application,
    x: &Rc<X>,
    monitor: &gdk::Monitor,
    primary: bool,
) -> xlib::Window {
    let geometry = monitor.geometry();
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("decklock-x11-probe"));
    window.set_decorated(false);
    window.set_default_size(geometry.width(), geometry.height());

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    let frames = Rc::new(Cell::new(0u64));
    let area = gtk::DrawingArea::new();
    area.set_content_width(400);
    area.set_content_height(40);
    {
        let frames = frames.clone();
        area.set_draw_func(move |_, cr, width, height| {
            let position = (frames.get() % width.max(1) as u64) as f64;
            cr.set_source_rgb(0.1, 0.1, 0.1);
            let _ = cr.paint();
            cr.set_source_rgb(0.9, 0.5, 0.1);
            cr.rectangle(position, 0.0, 20.0, height as f64);
            let _ = cr.fill();
        });
    }
    let entry = gtk::PasswordEntry::new();
    entry.set_width_chars(24);
    content.append(&area);
    content.append(&entry);
    window.set_child(Some(&content));

    // An autohide popover grabs input through GDK when shown and ungrabs when
    // hidden; the question is whether that ungrab also drops our core grabs.
    let popover = gtk::Popover::new();
    popover.set_child(Some(&gtk::Label::new(Some("popover"))));
    popover.set_parent(&entry);
    popover.connect_visible_notify(|p| println!("LOCK popover visible={}", p.is_visible()));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let popover = popover.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            println!("LOCK key={}", key.name().unwrap_or_default());
            if key == gdk::Key::F2 {
                popover.popup();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    window.add_controller(keys);

    if primary {
        let ticking = frames.clone();
        area.add_tick_callback(move |area, _| {
            ticking.set(ticking.get() + 1);
            area.queue_draw();
            glib::ControlFlow::Continue
        });
        let (frames, mut last, mut at) = (frames.clone(), 0u64, Instant::now());
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let now = frames.get();
            println!(
                "LOCK fps={:.1}",
                (now - last) as f64 / at.elapsed().as_secs_f64()
            );
            (last, at) = (now, Instant::now());
            glib::ControlFlow::Continue
        });
    }

    // The X window exists after realize but is not mapped yet, so the window
    // manager has not seen a MapRequest and override-redirect still applies.
    WidgetExt::realize(&window);
    let xid = window.surface().and_downcast::<X11Surface>().unwrap().xid();
    if flag("PROBE_OVERRIDE", "1") == "1" {
        unsafe {
            let mut attrs: xlib::XSetWindowAttributes = mem::zeroed();
            attrs.override_redirect = xlib::True;
            (x.xlib.XChangeWindowAttributes)(x.dpy, xid, xlib::CWOverrideRedirect, &mut attrs);
        }
    }
    unsafe {
        let mask =
            x.attributes(xid).your_event_mask | xlib::VisibilityChangeMask | xlib::FocusChangeMask;
        (x.xlib.XSelectInput)(x.dpy, xid, mask);
    }
    println!(
        "LOCK realized xid=0x{xid:x} monitor={}x{}+{}+{}",
        geometry.width(),
        geometry.height(),
        geometry.x(),
        geometry.y()
    );

    {
        let entry_window = window.clone();
        let x = x.clone();
        entry.connect_activate(move |entry| {
            let text = entry.text();
            println!("LOCK entry-activate text={text:?}");
            entry.set_text("");
            if text == "secret" {
                unsafe {
                    (x.xlib.XUngrabKeyboard)(x.dpy, xlib::CurrentTime);
                    (x.xlib.XUngrabPointer)(x.dpy, xlib::CurrentTime);
                }
                println!("LOCK unlocked");
                if let Some(app) = entry_window.application() {
                    app.quit();
                }
            }
        });
    }

    {
        let x = x.clone();
        let entry = entry.clone();
        window.connect_map(move |_| {
            unsafe {
                (x.xlib.XMoveResizeWindow)(
                    x.dpy,
                    xid,
                    geometry.x(),
                    geometry.y(),
                    geometry.width() as c_uint,
                    geometry.height() as c_uint,
                );
                (x.xlib.XRaiseWindow)(x.dpy, xid);
            }
            entry.grab_focus();
            if primary && flag("PROBE_GRAB", "1") == "1" {
                start_grabs(x.clone(), xid);
            }
            let x = x.clone();
            glib::timeout_add_local_once(Duration::from_millis(700), move || report(&x, xid));
        });
    }

    window.present();
    xid
}

fn start_grabs(x: Rc<X>, xid: xlib::Window) {
    let started = Instant::now();
    let (keyboard, pointer, attempts) = (Cell::new(false), Cell::new(false), Cell::new(0));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        attempts.set(attempts.get() + 1);
        if !keyboard.get() {
            let code = x.grab_keyboard(xid);
            keyboard.set(code == xlib::GrabSuccess);
            if keyboard.get() || attempts.get() % 20 == 1 {
                println!(
                    "LOCK grab keyboard={} attempt={}",
                    grab_name(code),
                    attempts.get()
                );
            }
        }
        if !pointer.get() {
            let code = x.grab_pointer(xid);
            pointer.set(code == xlib::GrabSuccess);
            if pointer.get() || attempts.get() % 20 == 1 {
                println!(
                    "LOCK grab pointer={} attempt={}",
                    grab_name(code),
                    attempts.get()
                );
            }
        }
        unsafe {
            let mut focus = 0;
            let mut revert = 0;
            (x.xlib.XGetInputFocus)(x.dpy, &mut focus, &mut revert);
            if focus != xid {
                (x.xlib.XSetInputFocus)(x.dpy, xid, xlib::RevertToParent, xlib::CurrentTime);
            }
        }
        if keyboard.get() && pointer.get() {
            println!("LOCK grabs-held after={}ms", started.elapsed().as_millis());
            if flag("PROBE_REGRAB", "0") == "1" {
                let x = x.clone();
                glib::timeout_add_local(Duration::from_millis(250), move || {
                    let (k, p) = (x.grab_keyboard(xid), x.grab_pointer(xid));
                    if k != xlib::GrabSuccess || p != xlib::GrabSuccess {
                        println!(
                            "LOCK regrab keyboard={} pointer={}",
                            grab_name(k),
                            grab_name(p)
                        );
                    }
                    glib::ControlFlow::Continue
                });
            }
            return glib::ControlFlow::Break;
        }
        if started.elapsed() > Duration::from_secs(5) {
            println!(
                "LOCK grab-gave-up keyboard={} pointer={}",
                keyboard.get(),
                pointer.get()
            );
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

fn report(x: &X, xid: xlib::Window) {
    let attrs = x.attributes(xid);
    let (parent, _) = x.children(xid);
    let (mut focus, mut revert) = (0, 0);
    unsafe { (x.xlib.XGetInputFocus)(x.dpy, &mut focus, &mut revert) };
    println!(
        "LOCK report xid=0x{xid:x} override_redirect={} visibility_mask={} map_state={} geometry={}x{}+{}+{} parent_is_root={} topmost={} focus=0x{focus:x}",
        attrs.override_redirect,
        attrs.your_event_mask & xlib::VisibilityChangeMask != 0,
        attrs.map_state,
        attrs.width,
        attrs.height,
        attrs.x,
        attrs.y,
        parent == x.root,
        x.topmost() == xid,
    );
}
