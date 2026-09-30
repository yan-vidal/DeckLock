//! Restacking has no GTK, input grab or authentication work. Its owned X
//! connection runs independently of GTK rendering, and stops before unlock.

use gdk4_x11::{X11Display, x11::xlib};
use std::{
    ffi::CStr,
    mem, ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

// The stable prefix of xcb_setup_t, supplied by the server at connection setup.
// Read the actual GDK client's resource range rather than trusting window
// properties that another client could copy. This also preserves GTK popups.
#[repr(C)]
struct SetupPrefix {
    status: u8,
    padding: u8,
    major: u16,
    minor: u16,
    length: u16,
    release: u32,
    resource_base: u32,
    resource_mask: u32,
}

#[link(name = "xcb")]
unsafe extern "C" {
    fn xcb_get_setup(connection: *mut libc::c_void) -> *const SetupPrefix;
}

#[link(name = "X11-xcb")]
unsafe extern "C" {
    #[link_name = "XGetXCBConnection"]
    fn xcb_from_xlib(display: *mut xlib::Display) -> *mut libc::c_void;
}

pub(super) struct Guard {
    screens: Arc<Mutex<Vec<xlib::Window>>>,
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
    worker: Option<JoinHandle<()>>,
}

impl Guard {
    pub(super) fn start(display: &X11Display, windows: Vec<xlib::Window>) -> Result<Self, String> {
        let xlib = xlib::Xlib::open().map_err(|e| e.to_string())?;
        // SAFETY: called on GTK's thread with its live display. Only immutable
        // connection setup and a copied display name cross to the worker. GDK
        // calls XInitThreads before opening its X11 display (GTK's X11 source).
        let (name, base, mask) = unsafe {
            let dpy = display.xdisplay();
            let connection = xcb_from_xlib(dpy);
            if connection.is_null() {
                return Err("cannot inspect the GDK X11 resource range".into());
            }
            let setup = xcb_get_setup(connection);
            if setup.is_null() {
                return Err("missing X11 connection setup".into());
            }
            (
                CStr::from_ptr((xlib.XDisplayString)(dpy)).to_owned(),
                (*setup).resource_base as xlib::Window,
                (*setup).resource_mask as xlib::Window,
            )
        };
        let root = display.xrootwindow();
        let screens = Arc::new(Mutex::new(windows));
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(Mutex::new(None));
        let (ready, initialized) = mpsc::sync_channel::<Result<(), String>>(1);
        let (thread_screens, thread_stop, thread_failure) =
            (screens.clone(), stop.clone(), failure.clone());
        let worker = thread::Builder::new()
            .name("x11-stacking".into())
            .spawn(move || {
                // SAFETY: this connection is created, used and closed exclusively
                // on this worker. GTK's display pointer and objects are never used.
                let dpy = unsafe { (xlib.XOpenDisplay)(name.as_ptr()) };
                if dpy.is_null() {
                    let _ = ready.send(Err("cannot open the X11 stacking connection".into()));
                    return;
                }
                let connection = Connection { xlib, dpy, root };
                unsafe {
                    (connection.xlib.XSelectInput)(dpy, root, xlib::SubstructureNotifyMask);
                    (connection.xlib.XFlush)(dpy);
                }
                let _ = ready.send(Ok(()));
                if let Err(error) = connection.watch(&thread_screens, &thread_stop, base, mask) {
                    *thread_failure.lock().unwrap() = Some(error);
                }
            })
            .map_err(|e| format!("cannot start the X11 stacking guard: {e}"))?;
        let guard = Self {
            screens,
            stop,
            failure,
            worker: Some(worker),
        };
        initialized.recv().map_err(|e| e.to_string())??;
        Ok(guard)
    }

    pub(super) fn update(&self, windows: Vec<xlib::Window>) {
        *self.screens.lock().unwrap() = windows;
    }

    pub(super) fn failure(&self) -> Option<String> {
        self.failure.lock().unwrap().clone()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Connection {
    xlib: xlib::Xlib,
    dpy: *mut xlib::Display,
    root: xlib::Window,
}

impl Connection {
    fn watch(
        &self,
        screens: &Mutex<Vec<xlib::Window>>,
        stop: &AtomicBool,
        base: xlib::Window,
        mask: xlib::Window,
    ) -> Result<(), String> {
        // SAFETY: all Xlib requests here belong to this worker's connection.
        unsafe {
            let mut fd = libc::pollfd {
                fd: (self.xlib.XConnectionNumber)(self.dpy),
                events: libc::POLLIN,
                revents: 0,
            };
            while !stop.load(Ordering::Acquire) {
                let mut changed = false;
                // Bound a notification flood so the guard still responds/stops.
                for _ in 0..256 {
                    if stop.load(Ordering::Acquire) || (self.xlib.XPending)(self.dpy) == 0 {
                        break;
                    }
                    let mut event: xlib::XEvent = mem::zeroed();
                    (self.xlib.XNextEvent)(self.dpy, &mut event);
                    changed |= matches!(event.get_type(), xlib::MapNotify | xlib::ConfigureNotify);
                }
                if changed && !stop.load(Ordering::Acquire) {
                    let windows = screens.lock().unwrap().clone();
                    self.raise_if_covered(&windows, base, mask);
                }
                if (self.xlib.XPending)(self.dpy) != 0 {
                    continue;
                }
                let result = libc::poll(&mut fd, 1, 50);
                if result < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    return Err("cannot poll the X11 stacking connection".into());
                }
                if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return Err("lost the X11 stacking connection".into());
                }
            }
        }
        Ok(())
    }

    unsafe fn raise_if_covered(
        &self,
        screens: &[xlib::Window],
        base: xlib::Window,
        mask: xlib::Window,
    ) {
        unsafe {
            let (mut root, mut parent, mut children, mut count) = (0, 0, ptr::null_mut(), 0);
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
                return;
            }
            let windows = std::slice::from_raw_parts(children, count as usize).to_vec();
            (self.xlib.XFree)(children.cast());
            let top = windows.into_iter().rev().find(|window| {
                let mut attributes: xlib::XWindowAttributes = mem::zeroed();
                (self.xlib.XGetWindowAttributes)(self.dpy, *window, &mut attributes) != 0
                    && attributes.map_state == xlib::IsViewable
            });
            if top.is_none_or(|window| window & !mask == base) {
                return;
            }
            for window in screens {
                (self.xlib.XRaiseWindow)(self.dpy, *window);
            }
            (self.xlib.XFlush)(self.dpy);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: this is the worker's exclusively owned connection.
        unsafe { (self.xlib.XCloseDisplay)(self.dpy) };
    }
}
