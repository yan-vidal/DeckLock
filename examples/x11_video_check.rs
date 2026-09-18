//! Video on X11 (#18): frames keep arriving, through the path this build is
//! expected to take. Run twice by the gates, against the same display: the
//! default build has no X11 GL support in the sink and must fall back to the
//! software path, and the build with the `x11` feature must accelerate.
//!
//!   x11_video_check <video> <accelerated|software>
use gtk::{gio, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn pump(seconds: u64) {
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn main() {
    let video = std::env::args_os().nth(1).expect("pass the video to play");
    let expected = std::env::args()
        .nth(2)
        .expect("pass the expected path: accelerated or software");
    assert!(
        matches!(expected.as_str(), "accelerated" | "software"),
        "unknown expected path {expected}"
    );
    gtk::init().unwrap();
    assert!(
        matches!(
            gtk::gdk::Display::default().unwrap().backend(),
            gtk::gdk::Backend::X11
        ),
        "this check is about the X11 display protocol"
    );
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.X11VideoCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = gtk::ApplicationWindow::new(&app);
    let picture = gtk::Picture::new();
    window.set_child(Some(&picture));
    window.set_default_size(640, 360);
    window.present();
    pump(1);

    let playback = decklock::media::Playback::new(std::path::Path::new(&video), &picture)
        .expect("the bundled video plays");
    let paintable = picture.paintable().expect("the sink provides a paintable");
    let frames = Rc::new(Cell::new(0u32));
    let counter = frames.clone();
    paintable.connect_invalidate_contents(move |_| counter.set(counter.get() + 1));
    pump(3);

    assert!(
        frames.get() > 10,
        "video delivered {} frames in three seconds on X11",
        frames.get()
    );
    assert!(paintable.intrinsic_width() > 0, "the video never decoded");
    let path = if playback.accelerated() {
        "accelerated"
    } else {
        "software"
    };
    assert_eq!(
        path, expected,
        "this build took the {path} path on X11, not the {expected} one"
    );
    println!(
        "PASS: video on X11 delivered {} frames through the {path} path",
        frames.get()
    );
}
