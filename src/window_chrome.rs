//! Decorations for ordinary windows only. Never call this for a lock surface.
use gtk::prelude::*;
pub fn install(window: &impl IsA<gtk::Window>) {
    let window = window.as_ref();
    let header = window
        .titlebar()
        .and_downcast::<gtk::HeaderBar>()
        .unwrap_or_else(|| {
            let header = gtk::HeaderBar::new();
            window.set_titlebar(Some(&header));
            header
        });
    header.set_show_title_buttons(true);
    header.set_decoration_layout(Some(":close"));
    if let Some(parent) = window.transient_for() {
        parent
            .bind_property("decorated", window, "decorated")
            .sync_create()
            .build();
    }
}
