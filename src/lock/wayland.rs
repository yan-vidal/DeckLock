//! `ext-session-lock-v1` through gtk4-session-lock. The compositor owns the lock
//! state, so the session stays locked if this process exits.

use super::LockBackend;
use crate::{
    session::Session,
    ui::{self, Settings, View},
};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zeroize::Zeroizing;

pub(super) struct Wayland(gtk4_session_lock::Instance);

impl LockBackend for Wayland {
    fn lock(&self) {
        // An immediate failure is also reported through ::failed, which
        // terminates the session, so the returned flag adds nothing.
        self.0.lock();
    }

    fn unlock(&self) {
        self.0.unlock();
    }
}

pub(super) fn configure(
    app: &gtk::Application,
    settings: Rc<Settings>,
    session: Rc<RefCell<Session>>,
    views: Rc<RefCell<Vec<View>>>,
    submit: Rc<dyn Fn(Zeroizing<String>)>,
    exit_code: Rc<Cell<i32>>,
) -> Wayland {
    let lock = gtk4_session_lock::Instance::new();
    let locked_session = session.clone();
    lock.connect_locked(move |_| {
        locked_session.borrow_mut().mark_locked();
    });
    let failed_app = app.downgrade();
    lock.connect_failed(move |_| {
        session.borrow_mut().terminate();
        exit_code.set(1);
        eprintln!("Session lock failed; no unlock requested");
        if let Some(app) = failed_app.upgrade() {
            app.quit();
        }
    });
    let unlocked_app = app.downgrade();
    lock.connect_unlocked(move |_| {
        if let Some(app) = unlocked_app.upgrade() {
            app.quit();
        }
    });
    let app = app.downgrade();
    lock.connect_monitor(move |lock, monitor| {
        let Some(app) = app.upgrade() else {
            return;
        };
        let view = ui::build(&app, settings.clone(), submit.clone());
        let window = view.window.clone();
        views.borrow_mut().push(view);
        let views = views.clone();
        let weak_window = window.downgrade();
        monitor.connect_invalidate(move |_| {
            if let Some(window) = weak_window.upgrade() {
                views.borrow_mut().retain(|v| v.window != window);
            }
            // gtk4-session-lock unmaps/destroys this monitor's window itself.
        });
        lock.assign_window_to_monitor(&window, monitor);
    });
    Wayland(lock)
}
