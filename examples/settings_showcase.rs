//! Timed settings-only showcase for documentation captures. Never saves or locks.
use gtk::{gio, prelude::*};
fn find(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if widget.widget_name() == name {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if let Some(found) = find(&node, name) {
            return Some(found);
        }
    }
    None
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.SettingsShowcase"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let path = std::env::args_os()
        .nth(1)
        .expect("Config path (read only)")
        .into();
    let window =
        decklock::settings::build(&app, path, Some("en-US"), std::env::current_exe().unwrap())
            .unwrap();
    let selector = find(window.upcast_ref(), "settings-theme-selector")
        .unwrap()
        .downcast::<gtk::DropDown>()
        .unwrap();
    let tabs = find(window.upcast_ref(), "settings-media-tabs")
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    selector.set_selected(1);
    window.present();
    let main_loop = glib::MainLoop::new(None, false);
    let stop = main_loop.clone();
    let step = std::cell::Cell::new(0);
    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        let n = step.get() + 1;
        step.set(n);
        match n {
            7 => tabs.set_visible_child_name("rest"),
            12 => selector.set_selected(2),
            20 => {
                window.destroy();
                stop.quit();
                return glib::ControlFlow::Break;
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });
    main_loop.run();
}
