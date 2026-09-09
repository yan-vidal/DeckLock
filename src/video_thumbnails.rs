//! Serial background decoding with a bounded, per-settings-window cache.
use gtk::{gdk, gio, prelude::*};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime},
};

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    stamp: Option<(u64, SystemTime)>,
}
enum Entry {
    Pending(Vec<glib::WeakRef<gtk::Picture>>),
    Ready(Option<gdk::Texture>),
}
#[derive(Default)]
struct State {
    entries: HashMap<Key, Entry>,
    queue: VecDeque<Key>,
    completed: VecDeque<Key>,
    running: bool,
}
#[derive(Clone, Default)]
pub(crate) struct Cache(Rc<RefCell<State>>);
impl Cache {
    pub(crate) fn widget(&self, path: &Path) -> gtk::Stack {
        let stack = gtk::Stack::new();
        let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
        icon.set_pixel_size(36);
        stack.add_child(&icon);
        let picture = gtk::Picture::new();
        picture.set_widget_name("video-thumbnail");
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(88, 58);
        stack.add_child(&picture);
        stack.set_visible_child(&icon);
        let key = Key {
            path: path.to_owned(),
            stamp: std::fs::metadata(path)
                .ok()
                .and_then(|m| Some((m.len(), m.modified().ok()?))),
        };
        let mut state = self.0.borrow_mut();
        match state.entries.get_mut(&key) {
            Some(Entry::Ready(texture)) => display(&picture, texture.as_ref()),
            Some(Entry::Pending(widgets)) => {
                widgets.retain(|w| w.upgrade().is_some());
                widgets.push(picture.downgrade());
            }
            None => {
                state
                    .entries
                    .insert(key.clone(), Entry::Pending(vec![picture.downgrade()]));
                state.queue.push_back(key);
            }
        }
        drop(state);
        start_next(&self.0);
        stack
    }
}
fn display(picture: &gtk::Picture, texture: Option<&gdk::Texture>) {
    if let Some(texture) = texture {
        picture.set_paintable(Some(texture));
        if let Some(stack) = picture.parent().and_downcast::<gtk::Stack>() {
            stack.set_visible_child(picture);
        }
    }
}
fn start_next(state: &Rc<RefCell<State>>) {
    let mut data = state.borrow_mut();
    if data.running {
        return;
    }
    let Some(key) = data.queue.pop_front() else {
        return;
    };
    data.running = true;
    drop(data);
    let weak = Rc::downgrade(state);
    let path = key.path.clone();
    // One decoder at a time, never a thread per row. Dropping the catalog drops
    // queued work; the active decode finishes without retaining GTK widgets.
    let task = gio::spawn_blocking(move || {
        crate::media::decode_frame(&path, 176, Duration::from_millis(400))
    });
    glib::MainContext::default().spawn_local(async move {
        let result = task.await;
        let Some(state) = weak.upgrade() else {
            return;
        };
        let texture = result.ok().and_then(Result::ok).map(|f| f.texture());
        let mut data = state.borrow_mut();
        if let Some(Entry::Pending(widgets)) = data.entries.remove(&key) {
            for picture in widgets.into_iter().filter_map(|w| w.upgrade()) {
                display(&picture, texture.as_ref());
            }
        }
        data.entries.insert(key.clone(), Entry::Ready(texture));
        data.completed.push_back(key);
        // Cache failures too, so a broken file is not decoded on every refresh.
        while data.completed.len() > 128 {
            if let Some(old) = data.completed.pop_front() {
                data.entries.remove(&old);
            }
        }
        data.running = false;
        drop(data);
        start_next(&state);
    });
}
