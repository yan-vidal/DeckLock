//! Fit foreground controls to the logical viewport without scaling the media.
use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct Adaptive {
        pub child: RefCell<Option<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Adaptive {
        const NAME: &'static str = "DeckLockAdaptiveControls";
        type Type = super::Adaptive;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Adaptive {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Adaptive {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            // Lock surfaces receive their size from the compositor. Do not let
            // the unscaled keyboard force a larger surface on small displays.
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            if width <= 0 || height <= 0 {
                return;
            }
            if let Some(child) = self.child.borrow().as_ref() {
                // The Steam Deck login at scale 1 is the reference layout.
                // Larger viewports keep their existing control size.
                let scale = (width as f32 / 1280.0).min(height as f32 / 800.0).min(1.0);
                child.allocate(
                    (width as f32 / scale).round() as i32,
                    (height as f32 / scale).round() as i32,
                    -1,
                    Some(gtk::gsk::Transform::new().scale(scale, scale)),
                );
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(child) = self.child.borrow().as_ref() {
                self.obj().snapshot_child(child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct Adaptive(ObjectSubclass<imp::Adaptive>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Adaptive {
    pub(super) fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let widget: Self = glib::Object::new();
        child.as_ref().set_parent(&widget);
        widget.imp().child.replace(Some(child.as_ref().clone()));
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget
    }
}
