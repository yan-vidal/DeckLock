//! An ordinary, unprivileged X client, used by scripts/test-lock-x11.py to test
//! the X11 lock backend from the outside. It never authenticates and never talks
//! to DeckLock: everything here is what any other program in the session can do.
//!
//!   report <title>        locate a window by title and describe it, as key=value
//!   grab [seconds]        try to take keyboard and pointer, optionally holding them
//!   stack <xid> [samples] how often that window is the topmost viewable one
//!   focus                 the window holding the X input focus
//!   raise <seconds>       map a window and keep raising it over everything
//!   cover <seconds>       map a window over everything once, then leave it there
//!   type <token>...       XTest keys: lowercase words, Return, Escape, BackSpace
//!   keylog <seconds>      XI2 raw key events, the exposure #17 warns about
//!   blanked               screen saver state: 0 off, 1 on (blanked), 2 disabled

use gdk4_x11::x11::{xinput2, xlib, xss, xtest};
use std::{
    ffi::{CStr, CString},
    mem,
    os::raw::{c_int, c_uint},
    ptr,
    time::{Duration, Instant},
};

struct X {
    xlib: xlib::Xlib,
    display: *mut xlib::Display,
    root: xlib::Window,
}

impl X {
    fn open() -> Self {
        let xlib = xlib::Xlib::open().expect("libX11");
        let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
        assert!(!display.is_null(), "cannot open DISPLAY");
        let root = unsafe { (xlib.XDefaultRootWindow)(display) };
        Self {
            xlib,
            display,
            root,
        }
    }

    fn attributes(&self, window: xlib::Window) -> xlib::XWindowAttributes {
        unsafe {
            let mut attributes: xlib::XWindowAttributes = mem::zeroed();
            (self.xlib.XGetWindowAttributes)(self.display, window, &mut attributes);
            attributes
        }
    }

    fn children(&self, window: xlib::Window) -> (xlib::Window, Vec<xlib::Window>) {
        let (mut root, mut parent, mut list, mut count) = (0, 0, ptr::null_mut(), 0 as c_uint);
        unsafe {
            (self.xlib.XQueryTree)(
                self.display,
                window,
                &mut root,
                &mut parent,
                &mut list,
                &mut count,
            );
            if list.is_null() {
                return (parent, Vec::new());
            }
            let children = std::slice::from_raw_parts(list, count as usize).to_vec();
            (self.xlib.XFree)(list.cast());
            (parent, children)
        }
    }

    fn title(&self, window: xlib::Window) -> String {
        unsafe {
            let mut name = ptr::null_mut();
            if (self.xlib.XFetchName)(self.display, window, &mut name) == 0 || name.is_null() {
                return String::new();
            }
            let text = CStr::from_ptr(name).to_string_lossy().into_owned();
            (self.xlib.XFree)(name.cast());
            text
        }
    }

    /// The largest window carrying that title, searching every descendant of the
    /// root so a window manager's frames do not hide the client window inside
    /// them. Size decides because GTK gives its 1x1 client leader window the same
    /// title as the lock screen.
    fn find(&self, title: &str) -> Option<xlib::Window> {
        let mut pending = self.children(self.root).1;
        let mut best: Option<(i64, xlib::Window)> = None;
        while let Some(window) = pending.pop() {
            pending.extend(self.children(window).1);
            if self.title(window) != title {
                continue;
            }
            let attributes = self.attributes(window);
            let area = i64::from(attributes.width) * i64::from(attributes.height);
            if best.is_none_or(|(largest, _)| area > largest) {
                best = Some((area, window));
            }
        }
        best.map(|(_, window)| window)
    }

    fn topmost_viewable(&self) -> xlib::Window {
        self.children(self.root)
            .1
            .into_iter()
            .rev()
            .find(|window| self.attributes(*window).map_state == xlib::IsViewable)
            .unwrap_or(0)
    }

    fn focus(&self) -> xlib::Window {
        let (mut window, mut revert) = (0, 0);
        unsafe { (self.xlib.XGetInputFocus)(self.display, &mut window, &mut revert) };
        window
    }

    fn grab(&self) -> (c_int, c_int) {
        unsafe {
            let keyboard = (self.xlib.XGrabKeyboard)(
                self.display,
                self.root,
                xlib::True,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            );
            let pointer = (self.xlib.XGrabPointer)(
                self.display,
                self.root,
                xlib::True,
                xlib::ButtonPressMask as c_uint,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                0,
                0,
                xlib::CurrentTime,
            );
            (keyboard, pointer)
        }
    }
}

fn check(condition: bool, message: &str) {
    assert!(condition, "{message}");
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let x = X::open();
    match arguments[0].as_str() {
        "report" => {
            let Some(window) = x.find(&arguments[1]) else {
                println!("found=false");
                return;
            };
            let attributes = x.attributes(window);
            let (parent, _) = x.children(window);
            println!(
                "found=true xid={window} override_redirect={} parent_is_root={} map_state={} \
                 width={} height={} x={} y={} topmost={} focused={}",
                attributes.override_redirect != 0,
                parent == x.root,
                attributes.map_state,
                attributes.width,
                attributes.height,
                attributes.x,
                attributes.y,
                x.topmost_viewable() == window,
                x.focus() == window,
            );
        }
        "grab" => {
            let (keyboard, pointer) = x.grab();
            println!(
                "keyboard={} pointer={} (0=GrabSuccess 1=AlreadyGrabbed)",
                keyboard, pointer
            );
            // Holding the grabs models a program that took the keyboard before
            // DeckLock started; the grabs go away when this process exits.
            if let Some(seconds) = arguments.get(1).and_then(|value| value.parse().ok()) {
                std::thread::sleep(Duration::from_secs(seconds));
            }
        }
        "stack" => {
            let target: xlib::Window = arguments[1].parse().expect("xid");
            let samples: usize = arguments
                .get(2)
                .and_then(|value| value.parse().ok())
                .unwrap_or(20);
            let mut on_top = 0;
            let mut other = 0;
            for _ in 0..samples {
                let top = x.topmost_viewable();
                if top == target {
                    on_top += 1;
                } else {
                    other = top;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            println!("on_top={on_top} samples={samples} other_top={other}");
        }
        "focus" => println!("focus={}", x.focus()),
        "blanked" => unsafe {
            // Xvfb has no DPMS extension, so a real monitor power-down cannot be
            // tested here; the core screen saver is the observable blank.
            let xss = xss::Xss::open().expect("libXss");
            let (mut event, mut error) = (0, 0);
            check(
                (xss.XScreenSaverQueryExtension)(x.display, &mut event, &mut error) != 0,
                "the X server has no MIT-SCREEN-SAVER extension",
            );
            let info = (xss.XScreenSaverAllocInfo)();
            (xss.XScreenSaverQueryInfo)(x.display, x.root, info);
            println!("state={}", (*info).state);
            (x.xlib.XFree)(info.cast());
        },
        "cover" | "raise" => unsafe {
            let screen = (x.xlib.XDefaultScreen)(x.display);
            let window = (x.xlib.XCreateSimpleWindow)(
                x.display,
                x.root,
                100,
                100,
                400,
                300,
                0,
                0,
                (x.xlib.XWhitePixel)(x.display, screen),
            );
            let title = CString::new("decklock-x11-intruder").unwrap();
            (x.xlib.XStoreName)(x.display, window, title.as_ptr());
            (x.xlib.XMapRaised)(x.display, window);
            (x.xlib.XSync)(x.display, xlib::False);
            // Focusing a window that is not viewable yet is a BadMatch, and
            // Xlib's default handler would end this process mid-loop.
            for _ in 0..100 {
                if x.attributes(window).map_state == xlib::IsViewable {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let deadline =
                Instant::now() + Duration::from_secs(arguments[1].parse().expect("seconds"));
            let repeat = arguments[0] == "raise";
            let mut raises = 0;
            loop {
                (x.xlib.XRaiseWindow)(x.display, window);
                (x.xlib.XSetInputFocus)(x.display, window, xlib::RevertToParent, xlib::CurrentTime);
                (x.xlib.XSync)(x.display, xlib::False);
                raises += 1;
                // "cover" raises once and then simply stays mapped, which is how
                // long the lock screen may take to come back is measured.
                if !repeat {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
                if Instant::now() >= deadline {
                    break;
                }
            }
            println!("mapped={window} raises={raises}");
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
        },
        "type" => unsafe {
            let xtest = xtest::Xf86vmode::open().expect("libXtst");
            for token in &arguments[1..] {
                let keys: Vec<String> = match token.as_str() {
                    "Return" | "Escape" | "BackSpace" => vec![token.clone()],
                    word => word.chars().map(String::from).collect(),
                };
                for key in keys {
                    let name = CString::new(key).unwrap();
                    let code = (x.xlib.XKeysymToKeycode)(
                        x.display,
                        (x.xlib.XStringToKeysym)(name.as_ptr()),
                    ) as c_uint;
                    (xtest.XTestFakeKeyEvent)(x.display, code, xlib::True, 0);
                    (xtest.XTestFakeKeyEvent)(x.display, code, xlib::False, 0);
                    (x.xlib.XSync)(x.display, xlib::False);
                    std::thread::sleep(Duration::from_millis(30));
                }
            }
            println!("typed={}", arguments[1..].join(" "));
        },
        "keylog" => unsafe {
            let xinput = xinput2::XInput2::open().expect("libXi");
            let (mut opcode, mut event, mut error) = (0, 0, 0);
            let name = CString::new("XInputExtension").unwrap();
            (x.xlib.XQueryExtension)(
                x.display,
                name.as_ptr(),
                &mut opcode,
                &mut event,
                &mut error,
            );
            let mut bits = [0u8; 4];
            xinput2::XISetMask(&mut bits, xinput2::XI_RawKeyPress);
            let mut mask = xinput2::XIEventMask {
                deviceid: xinput2::XIAllMasterDevices,
                mask_len: bits.len() as c_int,
                mask: bits.as_mut_ptr(),
            };
            (xinput.XISelectEvents)(x.display, x.root, &mut mask, 1);
            (x.xlib.XSync)(x.display, xlib::False);
            println!("listening=true");
            let deadline =
                Instant::now() + Duration::from_secs(arguments[1].parse().expect("seconds"));
            let mut captured = Vec::new();
            while Instant::now() < deadline {
                while (x.xlib.XPending)(x.display) > 0 {
                    let mut raw: xlib::XEvent = mem::zeroed();
                    (x.xlib.XNextEvent)(x.display, &mut raw);
                    let mut cookie = raw.generic_event_cookie;
                    if cookie.type_ == xlib::GenericEvent
                        && cookie.extension == opcode
                        && (x.xlib.XGetEventData)(x.display, &mut cookie) != 0
                    {
                        if cookie.evtype == xinput2::XI_RawKeyPress {
                            let event = &*(cookie.data as *const xinput2::XIRawEvent);
                            let symbol =
                                (x.xlib.XkbKeycodeToKeysym)(x.display, event.detail as u8, 0, 0);
                            let text = (x.xlib.XKeysymToString)(symbol);
                            if !text.is_null() {
                                captured.push(CStr::from_ptr(text).to_string_lossy().into_owned());
                            }
                        }
                        (x.xlib.XFreeEventData)(x.display, &mut cookie);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("captured={}", captured.join(" "));
        },
        other => panic!("unknown command {other}"),
    }
    unsafe { (x.xlib.XCloseDisplay)(x.display) };
}
