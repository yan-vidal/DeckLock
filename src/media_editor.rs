//! Library/pool cards shared by normal and idle settings.
use crate::{
    i18n::I18n,
    library::{self, Kind},
};
use gtk::{gio, prelude::*};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

type LibraryViews = Rc<RefCell<Vec<(glib::WeakRef<gtk::ListBox>, Kind)>>>;
#[derive(Clone)]
pub struct Catalog {
    paths: Rc<RefCell<Vec<PathBuf>>>,
    views: LibraryViews,
}
impl Catalog {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths: Rc::new(RefCell::new(paths)),
            views: Rc::new(RefCell::new(Vec::new())),
        }
    }
    fn filtered(&self, kind: Kind) -> Vec<PathBuf> {
        self.paths
            .borrow()
            .iter()
            .filter(|p| library::kind(p) == Some(kind))
            .cloned()
            .collect()
    }
    fn refresh(&self) {
        self.views.borrow_mut().retain(|(weak, kind)| {
            let Some(list) = weak.upgrade() else {
                return false;
            };
            fill(&list, &self.filtered(*kind));
            true
        });
    }
}
pub struct Editor {
    pub widget: gtk::Box,
    pub paths: Rc<RefCell<Vec<PathBuf>>>,
}
fn media_row(path: &Path) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for setter in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
    ] {
        setter(&row, 6);
    }
    if library::kind(path) == Some(Kind::Image)
        && let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 88, 58, true)
    {
        let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
        let picture = gtk::Picture::for_paintable(&texture);
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(88, 58);
        row.append(&picture);
    } else {
        let image = gtk::Image::from_icon_name("video-x-generic-symbolic");
        image.set_pixel_size(36);
        image.set_size_request(88, 58);
        row.append(&image);
    }
    let label = gtk::Label::new(path.file_name().and_then(|p| p.to_str()));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_max_width_chars(24);
    row.append(&label);
    row.set_tooltip_text(Some(&path.to_string_lossy()));
    row
}
fn fill(list: &gtk::ListBox, paths: &[PathBuf]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for path in paths {
        list.append(&media_row(path));
    }
}
fn scroll(list: &gtk::ListBox) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(list)
        .height_request(170)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build()
}
pub fn build(
    window: &gtk::ApplicationWindow,
    catalog: Catalog,
    initial: Vec<PathBuf>,
    strings: Rc<I18n>,
    name: &str,
) -> Editor {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_widget_name(name);
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    card.add_css_class("media-card");
    root.append(&card);
    let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
    left.set_hexpand(true);
    left.append(&gtk::Label::new(Some(&strings.text("media-library"))));
    let tabs = gtk::Notebook::new();
    tabs.add_css_class("media-tabs");
    tabs.set_hexpand(true);
    left.append(&tabs);
    let images = gtk::ListBox::new();
    let videos = gtk::ListBox::new();
    for (list, kind, label) in [
        (&images, Kind::Image, "media-images"),
        (&videos, Kind::Video, "media-videos"),
    ] {
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_widget_name(&format!(
            "{name}-{}",
            if kind == Kind::Image {
                "images"
            } else {
                "videos"
            }
        ));
        tabs.append_page(
            &scroll(list),
            Some(&gtk::Label::new(Some(&strings.text(label)))),
        );
        catalog.views.borrow_mut().push((list.downgrade(), kind));
    }
    catalog.refresh();
    let import = gtk::Button::with_label(&strings.text("media-import"));
    left.append(&import);
    card.append(&left);
    let add = gtk::Button::with_label(&strings.text("media-add"));
    add.set_valign(gtk::Align::Center);
    add.set_widget_name(&format!("{name}-add"));
    card.append(&add);
    let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
    right.set_hexpand(true);
    let pool_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pool_header.set_halign(gtk::Align::Center);
    let pool_title = gtk::Label::new(Some(&strings.text("media-pool")));
    pool_title.add_css_class("section-title");
    pool_header.append(&pool_title);
    let info = gtk::Button::from_icon_name("dialog-information-symbolic");
    info.add_css_class("info-button");
    info.set_widget_name(&format!("{name}-info"));
    info.set_tooltip_text(Some(&strings.text("media-pool-help")));
    pool_header.append(&info);
    right.append(&pool_header);
    let list = gtk::ListBox::new();
    list.set_widget_name(&format!("{name}-pool"));
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_placeholder(Some(&gtk::Label::new(Some(&strings.text("media-empty")))));
    fill(&list, &initial);
    let pool_scroll = scroll(&list);
    pool_scroll.set_height_request(208);
    right.append(&pool_scroll);
    let remove = gtk::Button::with_label(&strings.text("media-remove"));
    remove.set_widget_name(&format!("{name}-remove"));
    right.append(&remove);
    card.append(&right);
    let paths = Rc::new(RefCell::new(initial));
    let (weak_list, weak_tabs, weak_images, weak_videos) = (
        list.downgrade(),
        tabs.downgrade(),
        images.downgrade(),
        videos.downgrade(),
    );
    let pool = paths.clone();
    let choices = catalog.clone();
    add.connect_clicked(move |_| {
        let (Some(list), Some(tabs), Some(images), Some(videos)) = (
            weak_list.upgrade(),
            weak_tabs.upgrade(),
            weak_images.upgrade(),
            weak_videos.upgrade(),
        ) else {
            return;
        };
        let (source, kind) = if tabs.current_page() == Some(1) {
            (videos, Kind::Video)
        } else {
            (images, Kind::Image)
        };
        if let Some(row) = source.selected_row()
            && let Some(path) = choices.filtered(kind).get(row.index() as usize)
            && !pool.borrow().contains(path)
        {
            pool.borrow_mut().push(path.clone());
            fill(&list, &pool.borrow());
        }
    });
    let weak_list = list.downgrade();
    let pool = paths.clone();
    remove.connect_clicked(move |_| {
        if let Some(list) = weak_list.upgrade()
            && let Some(row) = list.selected_row()
        {
            pool.borrow_mut().remove(row.index() as usize);
            fill(&list, &pool.borrow());
        }
    });
    let message = gtk::Label::new(None);
    message.set_wrap(true);
    message.set_xalign(0.0);
    root.append(&message);
    let weak_window = window.downgrade();
    let weak_message = message.downgrade();
    import.connect_clicked(move |_| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(strings.text("media-import"))
            .modal(true)
            .build();
        let catalog = catalog.clone();
        let message = weak_message.clone();
        dialog.open_multiple(Some(&window), None::<&gio::Cancellable>, move |result| {
            let Ok(files) = result else {
                return;
            };
            let paths: Vec<_> = (0..files.n_items())
                .filter_map(|index| {
                    files
                        .item(index)
                        .and_downcast::<gio::File>()
                        .and_then(|file| file.path())
                })
                .collect();
            let root = library::user_dir();
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let results: Vec<_> = paths
                    .iter()
                    .map(|path| library::import(path, &root))
                    .collect();
                let _ = send.send(results);
            });
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                let results = match receive.try_recv() {
                    Ok(results) => results,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        return glib::ControlFlow::Continue;
                    }
                    Err(_) => return glib::ControlFlow::Break,
                };
                let mut errors = Vec::new();
                for result in results {
                    match result {
                        Ok(path) => catalog.paths.borrow_mut().push(path),
                        Err(error) => errors.push(error),
                    }
                }
                catalog.paths.borrow_mut().sort();
                catalog.paths.borrow_mut().dedup();
                catalog.refresh();
                if let Some(message) = message.upgrade() {
                    message.set_text(&errors.join("\n"));
                }
                glib::ControlFlow::Break
            });
        });
    });
    Editor {
        widget: root,
        paths,
    }
}
