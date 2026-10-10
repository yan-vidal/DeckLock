//! Offline Markdown source shared with the package and a future static website.
use crate::i18n::I18n;
use gtk::{gdk, prelude::*};
use std::rc::Rc;
fn documents(strings: &I18n) -> [&'static str; 2] {
    if strings.text("help-language") == "pt-BR" {
        [
            include_str!("../docs/guide/pt-BR/general.md"),
            include_str!("../docs/guide/pt-BR/advanced.md"),
        ]
    } else {
        [
            include_str!("../docs/guide/en-US/general.md"),
            include_str!("../docs/guide/en-US/advanced.md"),
        ]
    }
}
/// The short guide of a lock or login screen (the preview shows the lock's).
fn screen_document(strings: &I18n, login: bool) -> &'static str {
    match (login, strings.text("help-language") == "pt-BR") {
        (true, true) => include_str!("../docs/guide/pt-BR/login.md"),
        (true, false) => include_str!("../docs/guide/en-US/login.md"),
        (false, true) => include_str!("../docs/guide/pt-BR/lock.md"),
        (false, false) => include_str!("../docs/guide/en-US/lock.md"),
    }
}
fn page(source: &str, selectable: bool) -> gtk::ScrolledWindow {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for setter in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        setter(&body, 24);
    }
    let mut code = false;
    let mut block = String::new();
    for line in source.lines().chain(std::iter::once("")) {
        if line.starts_with("```") || (!code && line.is_empty()) {
            if !block.is_empty() {
                let (text, class) = if code {
                    (block.as_str(), "monospace")
                } else if let Some(t) = block.strip_prefix("# ") {
                    (t, "title-1")
                } else if let Some(t) = block.strip_prefix("## ") {
                    (t, "title-2")
                } else {
                    (block.as_str(), "body")
                };
                // Plain labels deliberately do not interpret HTML/Pango or execute links.
                let label = gtk::Label::new(Some(text.trim_end()));
                label.set_xalign(0.);
                label.set_selectable(selectable);
                if !selectable {
                    // A screen's short guide keeps lines of a readable length.
                    label.set_max_width_chars(72);
                }
                label.set_wrap(true);
                label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                label.add_css_class(class);
                body.append(&label);
                block.clear();
            }
            if line.starts_with("```") {
                code = !code;
            }
        } else {
            if !block.is_empty() {
                block.push(if code || line.starts_with("- ") {
                    '\n'
                } else {
                    ' '
                });
            }
            block.push_str(line);
        }
    }
    gtk::ScrolledWindow::builder()
        .child(&body)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build()
}
fn populate(window: &gtk::Window, strings: &I18n) {
    window.set_title(Some(&strings.text("help-title")));
    let notebook = window
        .child()
        .and_downcast::<gtk::Notebook>()
        .expect("Help notebook");
    let selected = notebook.current_page().unwrap_or(0);
    while notebook.n_pages() > 0 {
        notebook.remove_page(Some(0));
    }
    for (source, key) in documents(strings)
        .into_iter()
        .zip(["help-general", "help-advanced"])
    {
        notebook.append_page(
            &page(source, true),
            Some(&gtk::Label::new(Some(&strings.text(key)))),
        );
    }
    notebook.set_current_page(Some(selected));
}
pub fn refresh_all(strings: &I18n) {
    for window in gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
    {
        if window.widget_name() == "decklock-help" {
            populate(&window, strings);
        }
    }
}
fn open(parent: &gtk::Window, strings: &I18n) {
    if let Some(window) = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.widget_name() == "decklock-help" && w.transient_for().as_ref() == Some(parent))
    {
        populate(&window, strings);
        window.present();
        return;
    }
    let window = gtk::Window::builder()
        .transient_for(parent)
        .destroy_with_parent(true)
        .default_width(840)
        .default_height(680)
        .build();
    window.set_widget_name("decklock-help");
    window.add_css_class("settings");
    crate::window_chrome::install(&window);
    let notebook = gtk::Notebook::new();
    notebook.set_widget_name("help-tabs");
    window.set_child(Some(&notebook));
    populate(&window, strings);
    let key = gtk::EventControllerKey::new();
    let weak = window.downgrade();
    key.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key);
    // destroy-with-parent is only a window-manager hint, and GTK4 defers "destroy"
    // until the last reference drops; unmapping the parent is what actually happens.
    let orphan = window.downgrade();
    parent.connect_unmap(move |_| {
        if let Some(window) = orphan.upgrade() {
            window.destroy();
        }
    });
    window.present();
}
/// The short guide of a lock, login or preview screen and what opens it. A
/// session lock displays no other window, so the guide is part of the screen's
/// own surface; the full guide stays in settings.
pub struct ScreenHelp {
    pub button: gtk::Button,
    /// Hidden until F1 or the button shows it; Esc or its close button hide it.
    pub panel: gtk::Box,
    /// Caller owns the controller's lifetime, including preview rebuild cleanup.
    pub key: gtk::EventControllerKey,
}
pub fn screen(strings: &I18n, login: bool) -> ScreenHelp {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_widget_name("help-panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    panel.set_visible(false);
    let close = gtk::Button::with_label("×");
    close.set_widget_name("help-close");
    close.set_tooltip_text(Some(&strings.text("help-close")));
    close.set_halign(gtk::Align::End);
    let weak = panel.downgrade();
    close.connect_clicked(move |_| {
        if let Some(panel) = weak.upgrade() {
            panel.set_visible(false);
        }
    });
    panel.append(&close);
    let content = page(screen_document(strings, login), false);
    content.set_vexpand(false);
    content.set_propagate_natural_width(true);
    content.set_propagate_natural_height(true);
    content.set_max_content_width(720);
    content.set_max_content_height(560);
    panel.append(&content);
    let button = gtk::Button::with_label("?");
    button.set_widget_name("help-button");
    button.set_tooltip_text(Some(&strings.text("help-tooltip")));
    let weak = panel.downgrade();
    button.connect_clicked(move |_| {
        if let Some(panel) = weak.upgrade() {
            panel.set_visible(!panel.is_visible());
        }
    });
    let key = gtk::EventControllerKey::new();
    key.set_name(Some("decklock-help-shortcut"));
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = panel.downgrade();
    key.connect_key_pressed(move |_, key, _, _| {
        let Some(panel) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        if key == gdk::Key::F1 {
            panel.set_visible(!panel.is_visible());
            return glib::Propagation::Stop;
        }
        // Esc closes the help before it hides the keyboard or closes a preview.
        if key == gdk::Key::Escape && panel.is_visible() {
            panel.set_visible(false);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    ScreenHelp { button, panel, key }
}
/// Caller owns the controller's lifetime, including preview rebuild cleanup.
pub fn controls(
    window: &impl IsA<gtk::Window>,
    strings: Rc<I18n>,
) -> (gtk::Button, gtk::EventControllerKey) {
    // A plain question mark, independent of which icons the theme provides.
    let button = gtk::Button::with_label("?");
    button.set_widget_name("help-button");
    button.set_tooltip_text(Some(&strings.text("help-tooltip")));
    let parent = window.as_ref().downgrade();
    let action_strings = strings.clone();
    button.connect_clicked(move |_| {
        if let Some(parent) = parent.upgrade() {
            open(&parent, &action_strings);
        }
    });
    let key = gtk::EventControllerKey::new();
    key.set_name(Some("decklock-help-shortcut"));
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let parent = window.as_ref().downgrade();
    key.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::F1 {
            if let Some(parent) = parent.upgrade() {
                open(&parent, &strings);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    (button, key)
}
