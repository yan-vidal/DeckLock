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

type Thumbnails = Rc<RefCell<Vec<(glib::WeakRef<gtk::Picture>, String)>>>;
type LibraryViews = Rc<RefCell<Vec<(glib::WeakRef<gtk::ListBox>, Kind)>>>;
#[derive(Clone)]
pub struct Catalog {
    paths: Rc<RefCell<Vec<PathBuf>>>,
    pub procedurals: Rc<RefCell<crate::procedural::Presets>>,
    thumbnails: Thumbnails,
    video_thumbnails: crate::video_thumbnails::Cache,
    strings: Rc<RefCell<Option<Rc<I18n>>>>,
    editor: Rc<RefCell<Option<gtk::Window>>>,
    views: LibraryViews,
    viewer: crate::media_viewer::Viewer,
    parent: Rc<RefCell<glib::WeakRef<gtk::ApplicationWindow>>>,
}
impl Catalog {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths: Rc::new(RefCell::new(paths)),
            procedurals: Default::default(),
            thumbnails: Default::default(),
            video_thumbnails: Default::default(),
            strings: Default::default(),
            editor: Default::default(),
            views: Rc::new(RefCell::new(Vec::new())),
            viewer: Default::default(),
            parent: Default::default(),
        }
    }
    fn configure_action(&self, id: &str) -> Rc<dyn Fn()> {
        let parent = self.parent.clone();
        let strings = self.strings.clone();
        let editor = self.editor.clone();
        let viewer = self.viewer.downgrade();
        let presets = self.procedurals.clone();
        let id = id.to_string();
        let thumbnails = self.thumbnails.clone();
        Rc::new(move || {
            let Some(parent) = parent.borrow().upgrade() else {
                return;
            };
            let Some(strings) = strings.borrow().clone() else {
                return;
            };
            if let Some(window) = editor.borrow_mut().take() {
                window.close();
            }
            let target = crate::procedural::path(&id);
            let callback_id = id.clone();
            let weak_parent = parent.downgrade();
            let viewer = viewer.clone();
            let thumbnails = thumbnails.clone();
            let window = crate::procedural_editor::open(
                &parent,
                &id,
                presets.clone(),
                &strings,
                move |value| {
                    if let Some(parent) = weak_parent.upgrade()
                        && let Some(viewer) = viewer.upgrade()
                    {
                        viewer.refresh_procedural(&parent, &target, value.animation(&callback_id));
                    }
                    thumbnails.borrow_mut().retain(|(weak, id)| {
                        let Some(picture) = weak.upgrade() else {
                            return false;
                        };
                        if id == &callback_id
                            && let Ok(texture) =
                                crate::animation::texture(&value.animation(id), 2., 320, 180)
                        {
                            picture.set_paintable(Some(&texture));
                        }
                        true
                    });
                },
            );
            editor.replace(Some(window));
        })
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
            fill(&list, &self.filtered(*kind), self);
            true
        });
    }
}
pub struct Editor {
    pub widget: gtk::Box,
    pub paths: Rc<RefCell<Vec<PathBuf>>>,
}
fn media_row(path: &Path, catalog: &Catalog) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for setter in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
    ] {
        setter(&row, 6);
    }
    if let Some(id) = crate::procedural::id(path) {
        let picture = gtk::Picture::new();
        picture.set_widget_name("procedural-thumbnail");
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(88, 58);
        if let Ok(texture) = crate::animation::texture(
            &catalog.procedurals.borrow().get(id).animation(id),
            2.,
            320,
            180,
        ) {
            picture.set_paintable(Some(&texture));
        }
        catalog
            .thumbnails
            .borrow_mut()
            .push((picture.downgrade(), id.to_string()));
        row.append(&picture);
    } else if library::kind(path) == Some(Kind::Image)
        && let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 88, 58, true)
    {
        let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
        let picture = gtk::Picture::for_paintable(&texture);
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(88, 58);
        row.append(&picture);
    } else if library::kind(path) == Some(Kind::Video) {
        row.append(&catalog.video_thumbnails.widget(path));
    } else {
        // A file that cannot be decoded in time keeps its generic icon.
        let image = gtk::Image::from_icon_name(if library::kind(path) == Some(Kind::Procedural) {
            "applications-graphics-symbolic"
        } else {
            "video-x-generic-symbolic"
        });
        image.set_pixel_size(36);
        image.set_size_request(88, 58);
        row.append(&image);
    }
    let translated = crate::procedural::id(path).and_then(|id| {
        catalog
            .strings
            .borrow()
            .as_ref()
            .map(|s| s.text(&format!("animation-{id}")))
    });
    let label = gtk::Label::new(
        translated
            .as_deref()
            .or_else(|| path.file_name().and_then(|p| p.to_str())),
    );
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_max_width_chars(24);
    row.append(&label);
    let eye = gtk::Button::from_icon_name("view-reveal-symbolic");
    eye.add_css_class("media-eye");
    eye.set_widget_name("media-eye");
    eye.set_tooltip_text(Some(
        &path.file_name().unwrap_or_default().to_string_lossy(),
    ));
    let viewer = catalog.viewer.clone();
    let parent = catalog.parent.clone();
    let target = path.to_path_buf();
    let presets = catalog.procedurals.clone();
    let strings = catalog.strings.clone();
    let configure = crate::procedural::id(path).map(|id| catalog.configure_action(id));
    eye.connect_clicked(move |_| {
        if let Some(parent) = parent.borrow().upgrade() {
            let animation =
                crate::procedural::id(&target).map(|id| presets.borrow().get(id).animation(id));
            viewer.show_configured(&parent, &target, animation, strings.borrow().clone());
            if let Some(id) = crate::procedural::id(&target)
                && let Some(strings) = strings.borrow().as_ref()
            {
                viewer.set_title(&strings.text(&format!("animation-{id}")));
                if let Some(configure) = configure.as_ref() {
                    viewer.set_configure(configure.clone(), strings);
                }
            }
        }
    });
    row.append(&eye);
    if let Some(id) = crate::procedural::id(path) {
        let gear = gtk::Button::from_icon_name("emblem-system-symbolic");
        gear.set_widget_name("media-configure");
        if let Some(strings) = catalog.strings.borrow().as_ref() {
            gear.set_tooltip_text(Some(&strings.text("procedural-configure")));
        }
        let configure = catalog.configure_action(id);
        gear.connect_clicked(move |_| configure());
        row.append(&gear);
    }
    row.set_tooltip_text(Some(&path.to_string_lossy()));
    row
}
fn fill(list: &gtk::ListBox, paths: &[PathBuf], catalog: &Catalog) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for path in paths {
        list.append(&media_row(path, catalog));
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
    catalog.parent.replace(window.downgrade());
    catalog.strings.replace(Some(strings.clone()));
    let close_editor = catalog.editor.clone();
    window.connect_destroy(move |_| {
        if let Some(window) = close_editor.borrow_mut().take() {
            window.destroy();
        }
    });
    let close_viewer = catalog.viewer.clone();
    window.connect_destroy(move |_| close_viewer.close());
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
    let procedurals = gtk::ListBox::new();
    tabs.set_widget_name(&format!("{name}-tabs"));
    for (list, kind, label) in [
        (&images, Kind::Image, "media-images"),
        (&videos, Kind::Video, "media-videos"),
        (&procedurals, Kind::Procedural, "media-procedurals"),
    ] {
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_widget_name(&format!(
            "{name}-{}",
            match kind {
                Kind::Image => "images",
                Kind::Video => "videos",
                Kind::Procedural => "procedurals",
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
    let transfer = gtk::Box::new(gtk::Orientation::Vertical, 10);
    transfer.set_vexpand(false);
    for before in [true, false] {
        let line = gtk::Separator::new(gtk::Orientation::Vertical);
        line.add_css_class("media-divider");
        line.set_vexpand(true);
        line.set_halign(gtk::Align::Center);
        transfer.append(&line);
        if before {
            transfer.append(&add);
        }
    }
    card.append(&transfer);
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
    fill(&list, &initial, &catalog);
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
    let weak_procedurals = procedurals.downgrade();
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
        let (source, kind) = match tabs.current_page() {
            Some(2) => {
                let Some(list) = weak_procedurals.upgrade() else {
                    return;
                };
                (list, Kind::Procedural)
            }
            Some(1) => (videos, Kind::Video),
            _ => (images, Kind::Image),
        };
        if let Some(row) = source.selected_row()
            && let Some(path) = choices.filtered(kind).get(row.index() as usize)
            && !pool.borrow().contains(path)
        {
            pool.borrow_mut().push(path.clone());
            fill(&list, &pool.borrow(), &choices);
        }
    });
    let weak_list = list.downgrade();
    let choices = catalog.clone();
    let pool = paths.clone();
    remove.connect_clicked(move |_| {
        if let Some(list) = weak_list.upgrade()
            && let Some(row) = list.selected_row()
        {
            pool.borrow_mut().remove(row.index() as usize);
            fill(&list, &pool.borrow(), &choices);
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
