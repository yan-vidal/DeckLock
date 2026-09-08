//! Embedded icons for source builds, also installed in hicolor by the package.
use gtk::{gdk, gio};

pub const APP_ID: &str = "io.github.yan_vidal.DeckLock";
const RESOURCE_PATH: &str = "/io/github/yan_vidal/DeckLock/icons";

/// Called on the GTK thread, after initialization; no filesystem writes.
pub fn install() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| {
        gio::resources_register_include!("icons.gresource")
            .expect("Built-in icon resource must be valid");
    });
    if let Some(display) = gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        if !theme
            .resource_path()
            .iter()
            .any(|path| path == RESOURCE_PATH)
        {
            theme.add_resource_path(RESOURCE_PATH);
        }
    }
    gtk::Window::set_default_icon_name(APP_ID);
}
