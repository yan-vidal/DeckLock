//! One reusable, non-blocking transient viewer so the library stays clickable.
use gtk::{gio, prelude::*};
use std::{cell::RefCell, path::Path, rc::Rc};
#[derive(Clone, Default)]
pub struct Viewer(Rc<RefCell<Option<Content>>>);
struct Content {
    window: gtk::Window,
    picture: gtk::Picture,
    title: gtk::Label,
    playback: Rc<RefCell<Option<crate::media::Playback>>>,
}
impl Viewer {
    pub fn close(&self) {
        if let Some(content) = self.0.borrow_mut().take() {
            content.playback.borrow_mut().take();
            content.window.destroy();
        }
    }
    pub fn show(&self, parent: &gtk::ApplicationWindow, path: &Path) {
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
            window.set_titlebar(Some(&header));
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_can_shrink(true);
            picture.set_hexpand(true);
            picture.set_vexpand(true);
            root.append(&picture);
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
                title,
                playback,
            }
        });
        content.playback.borrow_mut().take();
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
            _ => content.picture.set_file(Some(&gio::File::for_path(path))),
        }
        content.window.present();
    }
}
