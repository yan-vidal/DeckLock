//! Validated theme working copy; originals are never overwritten by the editor.
use crate::{
    config::{Config, Theme},
    i18n::I18n,
};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
struct State {
    directory: tempfile::TempDir,
    active: Cell<bool>,
    error: RefCell<Option<String>>,
    window: RefCell<Option<gtk::Window>>,
    published: RefCell<Option<PathBuf>>,
}
#[derive(Clone)]
pub struct Drafts(Rc<State>);
impl Drafts {
    pub fn new() -> Result<Self, String> {
        Ok(Self(Rc::new(State {
            directory: tempfile::tempdir().map_err(|e| e.to_string())?,
            active: Cell::new(false),
            error: RefCell::new(None),
            window: RefCell::new(None),
            published: RefCell::new(None),
        })))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.0.error.borrow().clone().map_or(Ok(()), Err)
    }
    pub fn path(&self) -> Option<PathBuf> {
        self.0
            .active
            .get()
            .then(|| self.0.directory.path().to_path_buf())
    }
    pub fn close(&self) {
        self.0.error.borrow_mut().take();
        if let Some(window) = self.0.window.borrow_mut().take() {
            window.destroy();
        }
    }
    pub fn clear(&self) {
        self.close();
        self.0.active.set(false);
        self.0.published.borrow_mut().take();
    }
    pub fn persist(&self, config: &mut Config, config_path: &Path) -> Result<(), String> {
        let Some(source) = self.path() else {
            return Ok(());
        };
        let destination = if let Some(path) = self.0.published.borrow().as_ref() {
            path.clone()
        } else {
            let root = config_path
                .parent()
                .ok_or("Configuration directory missing")?
                .join("themes");
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            tempfile::Builder::new()
                .prefix("edited-")
                .tempdir_in(root)
                .map_err(|e| e.to_string())?
                .keep()
        };
        for name in ["style.css", "theme.toml"] {
            std::fs::copy(source.join(name), destination.join(name)).map_err(|e| e.to_string())?;
        }
        self.0.published.replace(Some(destination.clone()));
        config.theme = Some(destination);
        Ok(())
    }
    pub fn open(
        &self,
        parent: &gtk::ApplicationWindow,
        config: &Config,
        strings: Rc<I18n>,
        on_apply: Rc<dyn Fn(Theme)>,
    ) -> Result<(), String> {
        if let Some(window) = self.0.window.borrow().as_ref() {
            window.present();
            return Ok(());
        }
        let mut theme = Theme::from_config(config)?;
        if let Some(layout) = &config.layout {
            theme.layout = layout.clone();
        }
        let document = theme.document()?;
        let window = gtk::Window::builder()
            .transient_for(parent)
            .destroy_with_parent(true)
            .title(strings.text("theme-editor-title"))
            .default_width(880)
            .default_height(640)
            .build();
        window.add_css_class("settings");
        window.set_widget_name("theme-editor");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        for set in [
            gtk::prelude::WidgetExt::set_margin_top,
            gtk::prelude::WidgetExt::set_margin_bottom,
            gtk::prelude::WidgetExt::set_margin_start,
            gtk::prelude::WidgetExt::set_margin_end,
        ] {
            set(&root, 16);
        }
        window.set_child(Some(&root));
        let hint = gtk::Label::new(Some(&strings.text("theme-editor-hint")));
        hint.set_wrap(true);
        root.append(&hint);
        let tabs = gtk::Notebook::new();
        tabs.set_widget_name("theme-editor-tabs");
        tabs.set_vexpand(true);
        root.append(&tabs);
        let css = gtk::TextBuffer::new(None);
        let source_css = config
            .theme
            .as_ref()
            .and_then(|dir| std::fs::read_to_string(dir.join("style.css")).ok())
            .unwrap_or_else(|| theme.css.clone());
        css.set_text(&source_css);
        let metadata = gtk::TextBuffer::new(None);
        metadata.set_text(&document);
        for (name, buffer) in [("style.css", &css), ("theme.toml", &metadata)] {
            let text = gtk::TextView::with_buffer(buffer);
            text.set_widget_name(name);
            text.set_monospace(true);
            text.set_left_margin(12);
            text.set_top_margin(12);
            let scroll = gtk::ScrolledWindow::builder()
                .child(&text)
                .hexpand(true)
                .vexpand(true)
                .build();
            tabs.append_page(&scroll, Some(&gtk::Label::new(Some(name))));
        }
        let status = gtk::Label::new(None);
        status.set_widget_name("theme-editor-status");
        status.set_wrap(true);
        status.set_xalign(0.0);
        root.append(&status);
        // Where the disk write happens is not obvious from a window that previews
        // live, so say it next to the buttons rather than only in the hint above.
        let note = gtk::Label::new(Some(&strings.text("theme-editor-save-note")));
        note.set_widget_name("theme-editor-save-note");
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        root.append(&note);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let restore = gtk::Button::with_label(&strings.text("restore-defaults"));
        restore.set_widget_name("theme-editor-restore");
        actions.append(&restore);
        root.append(&actions);
        let (restore_css, restore_metadata) = (css.clone(), metadata.clone());
        restore.connect_clicked(move |_| {
            // Editing the buffers is enough: the validation pass already republishes
            // the draft and refreshes the preview from whatever they now hold.
            restore_css.set_text(include_str!("../themes/default/style.css"));
            restore_metadata.set_text(include_str!("../themes/default/theme.toml"));
        });
        let dirty = Rc::new(Cell::new(None::<Instant>));
        for buffer in [&css, &metadata] {
            let changed = dirty.clone();
            let state = Rc::downgrade(&self.0);
            let pending = strings.text("theme-editor-pending");
            buffer.connect_changed(move |_| {
                changed.set(Some(Instant::now()));
                if let Some(state) = state.upgrade() {
                    state.error.replace(Some(pending.clone()));
                }
            });
        }
        let close_requested = Rc::new(Cell::new(false));
        let pending_close = close_requested.clone();
        let close_dirty = dirty.clone();
        let state = Rc::downgrade(&self.0);
        let weak_window = window.downgrade();
        let weak_status = status.downgrade();
        let base = config
            .theme
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        glib::timeout_add_local(Duration::from_millis(100), move || {
            if weak_window.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }
            let Some(state) = state.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if dirty
                .get()
                .is_none_or(|time| time.elapsed() < Duration::from_millis(350))
            {
                return glib::ControlFlow::Continue;
            }
            dirty.set(None);
            let result = (|| -> Result<(), String> {
                let css = css
                    .text(&css.start_iter(), &css.end_iter(), true)
                    .to_string();
                let metadata = metadata
                    .text(&metadata.start_iter(), &metadata.end_iter(), true)
                    .to_string();
                let theme = Theme::from_document(&metadata, &css, &base)?;
                let errors = Rc::new(RefCell::new(Vec::new()));
                let capture = errors.clone();
                let provider = gtk::CssProvider::new();
                provider.connect_parsing_error(move |_, _, error| {
                    capture.borrow_mut().push(error.to_string())
                });
                provider.load_from_string(&theme.css);
                if !errors.borrow().is_empty() {
                    return Err(errors.borrow().join("; "));
                }
                // Export resolved paths, so relative original backgrounds survive the copy.
                std::fs::write(state.directory.path().join("theme.toml"), theme.document()?)
                    .map_err(|e| e.to_string())?;
                std::fs::write(state.directory.path().join("style.css"), css)
                    .map_err(|e| e.to_string())?;
                state.active.set(true);
                on_apply(theme);
                Ok(())
            })();
            state.error.replace(result.as_ref().err().cloned());
            if let Some(status) = weak_status.upgrade() {
                status.set_text(&match result {
                    Ok(()) => strings.text("theme-editor-valid"),
                    Err(e) => e,
                });
            }
            if pending_close.get() {
                state.error.borrow_mut().take();
                if let Some(window) = state.window.borrow_mut().take() {
                    window.destroy();
                }
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        let weak_state = Rc::downgrade(&self.0);
        window.connect_close_request(move |window| {
            if close_dirty.get().is_some() {
                close_requested.set(true);
                return glib::Propagation::Stop;
            }
            window.set_visible(false);
            if let Some(state) = weak_state.upgrade() {
                state.error.borrow_mut().take();
                state.window.borrow_mut().take();
            }
            glib::Propagation::Proceed
        });
        self.0.window.replace(Some(window.clone()));
        window.present();
        Ok(())
    }
}
