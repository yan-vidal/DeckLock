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
fn page(source: &str) -> gtk::ScrolledWindow {
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
                label.set_selectable(true);
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
            &page(source),
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
/// Caller owns the controller's lifetime, including preview rebuild cleanup.
pub fn controls(
    window: &impl IsA<gtk::Window>,
    strings: Rc<I18n>,
) -> (gtk::Button, gtk::EventControllerKey) {
    let button = gtk::Button::from_icon_name("help-browser-symbolic");
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
