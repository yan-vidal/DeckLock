//! One reusable, non-blocking transient viewer so the library stays clickable.
use gtk::{gio, prelude::*};
use std::{cell::RefCell, path::Path, rc::Rc};
type Configure = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
#[derive(Clone, Default)]
pub struct Viewer(Rc<RefCell<Option<Content>>>);
#[derive(Clone)]
pub struct WeakViewer(std::rc::Weak<RefCell<Option<Content>>>);
impl WeakViewer {
    pub fn upgrade(&self) -> Option<Viewer> {
        self.0.upgrade().map(Viewer)
    }
}
struct Content {
    window: gtk::Window,
    picture: gtk::Picture,
    body: gtk::Box,
    path: std::path::PathBuf,
    title: gtk::Label,
    gear: gtk::Button,
    metrics: gtk::Label,
    configure: Configure,
    playback: Rc<RefCell<Option<crate::media::Playback>>>,
}
impl Viewer {
    pub fn downgrade(&self) -> WeakViewer {
        WeakViewer(Rc::downgrade(&self.0))
    }
    pub fn close(&self) {
        if let Some(content) = self.0.borrow_mut().take() {
            content.playback.borrow_mut().take();
            content.window.destroy();
        }
    }
    pub fn set_configure(&self, callback: std::rc::Rc<dyn Fn()>, strings: &crate::i18n::I18n) {
        if let Some(content) = self.0.borrow().as_ref() {
            content.configure.replace(Some(callback));
            content
                .gear
                .set_tooltip_text(Some(&strings.text("procedural-configure")));
        }
    }
    pub fn set_title(&self, title: &str) {
        if let Some(content) = self.0.borrow().as_ref() {
            content.title.set_text(title);
            content.window.set_title(Some(title));
        }
    }
    pub fn show(&self, parent: &gtk::ApplicationWindow, path: &Path) {
        self.show_configured(parent, path, None);
    }
    pub fn refresh_procedural(
        &self,
        parent: &gtk::ApplicationWindow,
        path: &Path,
        animation: crate::animation::Animation,
    ) {
        let refresh = self
            .0
            .borrow()
            .as_ref()
            .is_some_and(|c| c.window.is_visible() && c.path == path);
        if refresh {
            let title = self.0.borrow().as_ref().unwrap().title.text();
            self.show_configured(parent, path, Some(animation));
            self.set_title(&title);
        }
    }
    pub fn show_configured(
        &self,
        parent: &gtk::ApplicationWindow,
        path: &Path,
        animation: Option<crate::animation::Animation>,
    ) {
        let animation = animation.or_else(|| {
            crate::procedural::id(path)
                .map(|id| crate::procedural::Parameters::default().animation(id))
        });
        let mut state = self.0.borrow_mut();
        let content = state.get_or_insert_with(|| {
            let window = gtk::Window::builder()
                .transient_for(parent)
                .destroy_with_parent(true)
                .default_width(800)
                .default_height(500)
                .build();
            window.add_css_class("settings");
            window.set_widget_name("media-viewer");
            let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
            window.set_child(Some(&root));
            let title = gtk::Label::new(None);
            title.set_margin_top(12);
            title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            let header = gtk::HeaderBar::new();
            header.set_title_widget(Some(&title));
            let gear = gtk::Button::from_icon_name("emblem-system-symbolic");
            gear.set_widget_name("media-viewer-configure");
            header.pack_end(&gear);
            let configure: Configure = Default::default();
            let action = configure.clone();
            gear.connect_clicked(move |_| {
                let callback = action.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
            window.set_titlebar(Some(&header));
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_can_shrink(true);
            picture.set_hexpand(true);
            picture.set_vexpand(true);
            root.append(&picture);
            let metrics = gtk::Label::new(None);
            metrics.set_widget_name("procedural-metrics");
            metrics.set_wrap(true);
            metrics.set_margin_bottom(8);
            root.append(&metrics);
            let playback = Rc::new(RefCell::new(None));
            let stop = playback.clone();
            window.connect_close_request(move |window| {
                stop.borrow_mut().take();
                window.set_visible(false);
                glib::Propagation::Stop
            });
            Content {
                window,
                picture,
                body: root,
                path: Default::default(),
                title,
                gear,
                metrics,
                configure,
                playback,
            }
        });
        content.playback.borrow_mut().take();
        content.path = path.to_path_buf();
        content.body.remove(&content.picture);
        content.gear.set_visible(animation.is_some());
        content.metrics.set_visible(animation.is_some());
        if animation.is_none() {
            content.configure.borrow_mut().take();
        }
        content.picture = if let Some(animation) = animation {
            let strings = crate::i18n::I18n::new(None, None).expect("Built-in locale");
            crate::animation::widget_with_stats(
                animation,
                Some(crate::preview_stats::Metrics::new(
                    &content.metrics,
                    &strings,
                )),
            )
        } else {
            gtk::Picture::new()
        };
        content.picture.set_can_shrink(true);
        content.picture.set_hexpand(true);
        content.picture.set_vexpand(true);
        if crate::procedural::id(path).is_none() {
            content.picture.set_content_fit(gtk::ContentFit::Contain);
        }
        content.body.prepend(&content.picture);
        content.picture.set_paintable(None::<&gtk::gdk::Paintable>);
        content
            .title
            .set_text(&path.file_name().unwrap_or_default().to_string_lossy());
        content.window.set_title(Some(
            &path.file_name().unwrap_or_default().to_string_lossy(),
        ));
        match crate::library::kind(path) {
            Some(crate::library::Kind::Video) => {
                match crate::media::Playback::new(path, &content.picture) {
                    Ok(playback) => {
                        content.playback.replace(Some(playback));
                    }
                    Err(error) => content.title.set_text(&error),
                }
            }
            Some(crate::library::Kind::Procedural) => {}
            _ => content.picture.set_file(Some(&gio::File::for_path(path))),
        }
        content.window.present();
    }
}
