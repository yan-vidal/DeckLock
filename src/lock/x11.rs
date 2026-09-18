//! X11 lock backend: one override-redirect window per monitor, plus keyboard and
//! pointer grabs taken on GDK's own connection.
//!
//! Neither guarantee the Wayland backend provides holds here, and nothing in this
//! module may pretend otherwise: if this process dies the session unlocks, and any
//! client in the session can read what is typed. [`super::Protocol::guarantees`]
//! states that, and the lock screen says it at lock time.
//!
//! A window and grabs are not enough. `docs/probes/x11-lock/README.md` records the
//! measurements behind each mechanism here: override-redirect set on the realized
//! but unmapped window, `owner_events` on the keyboard grab, re-raising driven by
//! `SubstructureNotify` on the root window rather than `VisibilityNotify`, and
//! taking X focus back from the window manager.

use super::{Guarantees, LockBackend};
use crate::{
    session::Session,
    ui::{self, Settings, View},
};
use gdk4_x11::{X11Display, X11Surface, x11::xlib};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    mem,
    os::raw::{c_int, c_uint},
    ptr,
    rc::Rc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(super) const GUARANTEES: Guarantees = Guarantees {
    survives_process_exit: false,
    isolates_input: false,
};

/// How long the first attempt keeps trying. Another client can legitimately hold
/// the keyboard for a moment (a menu closing), but a lock screen nobody grabbed
/// for is not a lock screen: the attempt fails instead of showing an unguarded
/// password box. It applies only before the session is locked.
const GRAB_DEADLINE: Duration = Duration::from_secs(5);
const GRAB_RETRY: Duration = Duration::from_millis(50);
/// Re-asserts grabs, stacking and focus for the states that arrive without an
/// event this process can observe: a VT switch back and a DPMS wake-up.
const WATCHDOG: Duration = Duration::from_secs(1);

/// Whether an X11 lock is possible in the running session.
pub(super) fn usable() -> bool {
    // Never downgrade a Wayland session. Under XWayland GDK reports an X11
    // display although the session is Wayland: locking the X11 clients would
    // leave every Wayland client, including the desktop itself, untouched.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("WAYLAND_SOCKET").is_some()
    {
        return false;
    }
    gdk::Display::default().is_some_and(|display| display.is::<X11Display>())
}

pub(super) fn configure(
    app: &gtk::Application,
    settings: Rc<Settings>,
    session: Rc<RefCell<Session>>,
    views: Rc<RefCell<Vec<View>>>,
    submit: Rc<dyn Fn(Zeroizing<String>)>,
    exit_code: Rc<Cell<i32>>,
) -> X11 {
    let display = gdk::Display::default()
        .and_downcast::<X11Display>()
        .expect("detect() accepted an X11 session");
    let xlib = xlib::Xlib::open().expect("libX11 is already loaded by GDK's X11 backend");
    // Xlib's default error handler exits the process, and a locker that exits
    // unlocks the session. A monitor unplugged between a query and its use is
    // enough to produce BadWindow, so errors are reported and survived instead.
    // SAFETY: sets a process-wide handler before any request below is issued.
    unsafe { (xlib.XSetErrorHandler)(Some(report_error)) };
    // SAFETY: GDK owns this connection for the lifetime of the display, and every
    // call below runs on the GTK thread that drives it.
    let dpy = unsafe { display.xdisplay() };
    X11 {
        backend: Rc::new(Backend {
            root: display.xrootwindow(),
            xlib,
            dpy,
            display,
            app: app.downgrade(),
            settings,
            session,
            views,
            submit,
            exit_code,
            screens: RefCell::new(Vec::new()),
            held: Cell::new(false),
            finished: Cell::new(false),
        }),
    }
}

/// Never terminates the process: see the handler installed in [`configure`].
///
/// SAFETY: called by Xlib with a valid error event while a request is being
/// processed, on the thread that issued it.
unsafe extern "C" fn report_error(
    _display: *mut xlib::Display,
    error: *mut xlib::XErrorEvent,
) -> c_int {
    // SAFETY: Xlib guarantees a valid pointer for the duration of this call.
    let (code, request) = unsafe { ((*error).error_code, (*error).request_code) };
    eprintln!("X11 request {request} failed with error {code}; the lock stays up");
    0
}

pub(super) struct X11 {
    backend: Rc<Backend>,
}

impl LockBackend for X11 {
    fn lock(&self) {
        self.backend.clone().start();
    }

    fn unlock(&self) {
        self.backend.release();
    }
}

struct Screen {
    monitor: gdk::Monitor,
    window: gtk::ApplicationWindow,
    xid: xlib::Window,
}

struct Backend {
    xlib: xlib::Xlib,
    dpy: *mut xlib::Display,
    root: xlib::Window,
    display: X11Display,
    app: glib::WeakRef<gtk::Application>,
    settings: Rc<Settings>,
    session: Rc<RefCell<Session>>,
    views: Rc<RefCell<Vec<View>>>,
    submit: Rc<dyn Fn(Zeroizing<String>)>,
    exit_code: Rc<Cell<i32>>,
    screens: RefCell<Vec<Screen>>,
    held: Cell<bool>,
    finished: Cell<bool>,
}

impl Backend {
    fn start(self: Rc<Self>) {
        self.sync_screens();
        if self.screens.borrow().is_empty() {
            self.fail("no monitor to cover");
            return;
        }
        // Weak: the display and these windows outlive nothing here, but holding
        // the backend from a signal it owns would be a cycle that never drops.
        let monitors = Rc::downgrade(&self);
        self.display
            .monitors()
            .connect_items_changed(move |_, _, _, _| {
                if let Some(backend) = monitors.upgrade() {
                    backend.sync_screens();
                }
            });
        self.watch_stacking_and_focus();
        self.clone()
            .take_input(Some(Instant::now() + GRAB_DEADLINE));
        let watchdog = self.clone();
        glib::timeout_add_local(WATCHDOG, move || {
            if watchdog.finished.get() {
                return glib::ControlFlow::Break;
            }
            if watchdog.held.get() {
                watchdog.reassert_input();
            }
            watchdog.keep_on_top();
            glib::ControlFlow::Continue
        });
    }

    /// Gives every monitor a lock screen and takes away the ones whose monitor is
    /// gone, so unplugging a monitor while locked cannot leave part of the desktop
    /// uncovered when it comes back.
    fn sync_screens(self: &Rc<Self>) {
        if self.finished.get() {
            return;
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let monitors: Vec<gdk::Monitor> = self
            .display
            .monitors()
            .into_iter()
            .filter_map(|item| item.ok().and_downcast::<gdk::Monitor>())
            .filter(gdk::Monitor::is_valid)
            .collect();
        let covered: Vec<gdk::Monitor> = self
            .screens
            .borrow()
            .iter()
            .map(|screen| screen.monitor.clone())
            .collect();
        let (open, close) = super::reconcile(&monitors, &covered);
        for monitor in open {
            self.open(&app, &monitor);
        }
        for monitor in close {
            self.close_screen(&monitor);
        }
    }

    fn open(self: &Rc<Self>, app: &gtk::Application, monitor: &gdk::Monitor) {
        let view = ui::build(app, self.settings.clone(), self.submit.clone());
        // #17: the weaker guarantee is stated on the lock screen itself, at lock
        // time, so a user who configured DeckLock under Wayland cannot receive it
        // silently after logging into an X11 session.
        view.warn_about(&GUARANTEES);
        let window = view.window.clone();
        window.set_decorated(false);
        let geometry = monitor.geometry();
        window.set_default_size(geometry.width(), geometry.height());
        // The X window exists once the widget is realized and before GTK maps it,
        // which is the only moment override-redirect can still be set on it.
        WidgetExt::realize(&window);
        let Some(xid) = window
            .surface()
            .and_downcast::<X11Surface>()
            .map(|surface| surface.xid())
        else {
            self.fail("GTK gave the lock window no X11 surface");
            return;
        };
        // SAFETY: xid is GDK's window on GDK's own connection, still unmapped.
        unsafe {
            let mut attributes: xlib::XSetWindowAttributes = mem::zeroed();
            attributes.override_redirect = xlib::True;
            (self.xlib.XChangeWindowAttributes)(
                self.dpy,
                xid,
                xlib::CWOverrideRedirect,
                &mut attributes,
            );
            // GDK4 takes focus events through XI2, so core FocusOut reaches this
            // process only because of this mask.
            let mask = self.attributes(xid).your_event_mask | xlib::FocusChangeMask;
            (self.xlib.XSelectInput)(self.dpy, xid, mask);
        }
        self.views.borrow_mut().push(view);
        self.screens.borrow_mut().push(Screen {
            monitor: monitor.clone(),
            window: window.clone(),
            xid,
        });
        // An unmapped window cannot be raised, focused or grabbed for.
        let mapped = Rc::downgrade(self);
        window.connect_map(move |_| {
            if let Some(backend) = mapped.upgrade() {
                backend.place(xid);
            }
        });
        let resized = Rc::downgrade(self);
        monitor.connect_notify_local(Some("geometry"), move |monitor, _| {
            if let Some(backend) = resized.upgrade() {
                backend.place_monitor(monitor);
            }
        });
        window.present();
    }

    fn close_screen(&self, monitor: &gdk::Monitor) {
        let mut screens = self.screens.borrow_mut();
        let Some(index) = screens.iter().position(|screen| &screen.monitor == monitor) else {
            return;
        };
        let screen = screens.remove(index);
        self.views
            .borrow_mut()
            .retain(|view| view.window != screen.window);
        screen.window.destroy();
    }

    fn place_monitor(&self, monitor: &gdk::Monitor) {
        let xid = self
            .screens
            .borrow()
            .iter()
            .find(|screen| &screen.monitor == monitor)
            .map(|screen| screen.xid);
        if let Some(xid) = xid {
            self.place(xid);
        }
    }

    /// Covers the monitor, raises and focuses. The window manager is not involved:
    /// an override-redirect window is positioned by its own client.
    fn place(&self, xid: xlib::Window) {
        let Some(geometry) = self
            .screens
            .borrow()
            .iter()
            .find(|screen| screen.xid == xid)
            .map(|screen| screen.monitor.geometry())
        else {
            return;
        };
        // SAFETY: GDK's connection, on the GTK thread.
        unsafe {
            (self.xlib.XMoveResizeWindow)(
                self.dpy,
                xid,
                geometry.x(),
                geometry.y(),
                geometry.width().max(1) as c_uint,
                geometry.height().max(1) as c_uint,
            );
            (self.xlib.XRaiseWindow)(self.dpy, xid);
            (self.xlib.XSetInputFocus)(self.dpy, xid, xlib::RevertToParent, xlib::CurrentTime);
        }
    }

    /// Re-raising is driven by root `SubstructureNotify`, which any client may
    /// select. `VisibilityNotify` cannot do this job: while the server has the
    /// Composite extension, which Xorg enables by default, it never reports the
    /// lock window as obscured.
    fn watch_stacking_and_focus(self: &Rc<Self>) {
        // SAFETY: GDK's connection; the root mask keeps GDK's own selections.
        unsafe {
            let mask = self.attributes(self.root).your_event_mask | xlib::SubstructureNotifyMask;
            (self.xlib.XSelectInput)(self.dpy, self.root, mask);
            let weak = Rc::downgrade(self);
            self.display.connect_xevent(move |_, event| {
                let Some(backend) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if backend.finished.get() {
                    return glib::Propagation::Proceed;
                }
                match (*event).get_type() {
                    xlib::MapNotify | xlib::ConfigureNotify => backend.keep_on_top(),
                    xlib::FocusOut => {
                        let window = (*event).focus_change.window;
                        if backend.screens.borrow().iter().any(|s| s.xid == window) {
                            backend.take_focus();
                        }
                    }
                    _ => {}
                }
                glib::Propagation::Proceed
            });
        }
    }

    /// Raises every lock screen when a window this process does not own covers
    /// them. Unmapped windows cover nothing, and GTK's own popups are not
    /// intruders: counting either one turns this into a raise loop.
    fn keep_on_top(&self) {
        if self.finished.get() {
            return;
        }
        let Some(top) = self.topmost_viewable() else {
            return;
        };
        let screens = self.screens.borrow();
        if screens.iter().any(|screen| screen.xid == top)
            || X11Surface::lookup_for_display(&self.display, top).is_some()
        {
            return;
        }
        for screen in screens.iter() {
            // SAFETY: GDK's connection, on the GTK thread.
            unsafe { (self.xlib.XRaiseWindow)(self.dpy, screen.xid) };
        }
    }

    fn take_focus(&self) {
        let Some(xid) = self.screens.borrow().first().map(|screen| screen.xid) else {
            return;
        };
        // SAFETY: GDK's connection, on the GTK thread.
        unsafe {
            (self.xlib.XSetInputFocus)(self.dpy, xid, xlib::RevertToParent, xlib::CurrentTime)
        };
    }

    /// Takes the keyboard and pointer, retrying until [`GRAB_DEADLINE`]. The
    /// session counts as locked only once both are held: until then the lock
    /// screen is on screen but `Session` refuses every authentication attempt.
    fn take_input(self: Rc<Self>, give_up: Option<Instant>) {
        let mut announced = false;
        glib::timeout_add_local(GRAB_RETRY, move || {
            if self.finished.get() {
                return glib::ControlFlow::Break;
            }
            let Some(xid) = self.screens.borrow().first().map(|screen| screen.xid) else {
                return glib::ControlFlow::Continue;
            };
            if self.grab(xid) {
                self.held.set(true);
                self.session.borrow_mut().mark_locked();
                return glib::ControlFlow::Break;
            }
            // Only the first attempt may give up. Once the session is locked,
            // losing the grabs is not a reason to stop: quitting would unlock the
            // screen, which is exactly what a program holding the keyboard would
            // want. Keep trying instead, and say so once.
            match give_up {
                Some(deadline) if Instant::now() > deadline => {
                    self.fail("another program holds the keyboard or the pointer");
                    glib::ControlFlow::Break
                }
                Some(_) => glib::ControlFlow::Continue,
                None => {
                    if !announced {
                        announced = true;
                        eprintln!("Lost the keyboard or pointer grab; retrying while locked");
                    }
                    glib::ControlFlow::Continue
                }
            }
        });
    }

    /// A VT switch or a DPMS wake-up can leave this process without the grabs and
    /// without an event saying so, so they are simply taken again. Re-issuing a
    /// grab this process already holds succeeds and changes nothing.
    fn reassert_input(self: &Rc<Self>) {
        let Some(xid) = self.screens.borrow().first().map(|screen| screen.xid) else {
            return;
        };
        if !self.grab(xid) {
            self.held.set(false);
            self.clone().take_input(None);
        }
    }

    fn grab(&self, xid: xlib::Window) -> bool {
        // SAFETY: GDK's connection, on the GTK thread.
        unsafe {
            // owner_events must stay true: with it false GTK receives no key at
            // all, because GDK reads events for its own windows.
            let keyboard = (self.xlib.XGrabKeyboard)(
                self.dpy,
                xid,
                xlib::True,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            );
            let pointer = (self.xlib.XGrabPointer)(
                self.dpy,
                xid,
                xlib::True,
                (xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask)
                    as c_uint,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                // No confine_to: with several monitors it would trap the pointer
                // on one of them. Every event still arrives through the grab.
                0,
                0,
                xlib::CurrentTime,
            );
            keyboard == xlib::GrabSuccess && pointer == xlib::GrabSuccess
        }
    }

    fn topmost_viewable(&self) -> Option<xlib::Window> {
        let (mut root, mut parent, mut children, mut count) = (0, 0, ptr::null_mut(), 0 as c_uint);
        // SAFETY: GDK's connection; the returned list is freed below.
        unsafe {
            if (self.xlib.XQueryTree)(
                self.dpy,
                self.root,
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            ) == 0
                || children.is_null()
            {
                return None;
            }
            let windows = std::slice::from_raw_parts(children, count as usize).to_vec();
            (self.xlib.XFree)(children.cast());
            windows
                .into_iter()
                .rev()
                .find(|window| self.attributes(*window).map_state == xlib::IsViewable)
        }
    }

    /// SAFETY: the caller must run on the GTK thread that owns GDK's connection.
    unsafe fn attributes(&self, window: xlib::Window) -> xlib::XWindowAttributes {
        unsafe {
            let mut attributes: xlib::XWindowAttributes = mem::zeroed();
            (self.xlib.XGetWindowAttributes)(self.dpy, window, &mut attributes);
            attributes
        }
    }

    /// Releases the lock. Reached only from [`LockBackend::unlock`], which main
    /// calls after `Session::complete_auth` authorized it.
    fn release(&self) {
        if self.finished.replace(true) {
            return;
        }
        self.ungrab();
        if let Some(app) = self.app.upgrade() {
            app.quit();
        }
    }

    fn ungrab(&self) {
        // SAFETY: GDK's connection, on the GTK thread.
        unsafe {
            (self.xlib.XUngrabKeyboard)(self.dpy, xlib::CurrentTime);
            (self.xlib.XUngrabPointer)(self.dpy, xlib::CurrentTime);
        }
        self.held.set(false);
    }

    /// The lock could not be established. This never unlocks a locked session: it
    /// terminates the session so no attempt can authorize an unlock afterwards.
    fn fail(&self, reason: &str) {
        if self.finished.replace(true) {
            return;
        }
        self.session.borrow_mut().terminate();
        self.exit_code.set(1);
        self.ungrab();
        eprintln!("Session lock failed; no unlock requested: {reason}");
        if let Some(app) = self.app.upgrade() {
            app.quit();
        }
    }
}
