//! Session lock backends.
//!
//! One rule binds every backend: no Drop implementation and no signal or
//! window-close callback may unlock. [`LockBackend::unlock`] is the only path,
//! and it is called only after [`Session::complete_auth`] returns true.
//!
//! Backends do not all promise the same things. Each protocol states what it
//! actually provides in [`Guarantees`], so callers read the guarantee from the
//! protocol instead of assuming every backend behaves like `ext-session-lock-v1`.

mod wayland;

use crate::{
    session::Session,
    ui::{Settings, View},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zeroize::Zeroizing;

/// What holds while the session is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guarantees {
    /// The session stays locked if this process exits, crashes or is killed.
    pub survives_process_exit: bool,
    /// Ordinary clients in the session cannot read what is typed on the lock screen.
    pub isolates_input: bool,
}

/// The session protocol a lock is taken through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// `ext-session-lock-v1`: the compositor owns the lock state and gives input
    /// only to the lock surfaces.
    Wayland,
}

impl Protocol {
    /// The protocol that can lock the running session, or None when none can.
    /// Call after `gtk::init`; the first call may block for a Wayland roundtrip.
    pub fn detect() -> Option<Self> {
        gtk4_session_lock::is_supported().then_some(Self::Wayland)
    }

    pub fn guarantees(self) -> Guarantees {
        match self {
            Self::Wayland => Guarantees {
                survives_process_exit: true,
                isolates_input: true,
            },
        }
    }

    /// Connects the lock lifecycle to `session` and builds a view for each
    /// monitor as it is locked. Nothing is locked before [`LockBackend::lock`].
    pub fn configure(
        self,
        app: &gtk::Application,
        settings: Rc<Settings>,
        session: Rc<RefCell<Session>>,
        views: Rc<RefCell<Vec<View>>>,
        submit: Rc<dyn Fn(Zeroizing<String>)>,
        exit_code: Rc<Cell<i32>>,
    ) -> Rc<dyn LockBackend> {
        match self {
            Self::Wayland => Rc::new(wayland::configure(
                app, settings, session, views, submit, exit_code,
            )),
        }
    }
}

/// A lock configured for one protocol.
pub trait LockBackend {
    /// Starts acquiring the lock. Ownership counts only once the backend calls
    /// [`Session::mark_locked`]; a failure terminates the session instead.
    fn lock(&self);

    /// Releases the lock. Call only after [`Session::complete_auth`] returned true.
    fn unlock(&self);
}
