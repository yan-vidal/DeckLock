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
#[cfg(feature = "x11")]
mod x11;

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
    /// Override-redirect windows plus input grabs: everything the X11 model can
    /// offer, which is strictly less. Built only with the `x11` feature.
    #[cfg(feature = "x11")]
    X11,
}

impl Protocol {
    /// The protocol that can lock the running session, or None when none can.
    /// Call after `gtk::init`; the first call may block for a Wayland roundtrip.
    pub fn detect() -> Option<Self> {
        if gtk4_session_lock::is_supported() {
            return Some(Self::Wayland);
        }
        // Wayland first, always: X11 is only for a session that has no stronger
        // way to lock, never a downgrade of one that does.
        #[cfg(feature = "x11")]
        if x11::usable() {
            return Some(Self::X11);
        }
        None
    }

    pub fn guarantees(self) -> Guarantees {
        match self {
            Self::Wayland => Guarantees {
                survives_process_exit: true,
                isolates_input: true,
            },
            #[cfg(feature = "x11")]
            Self::X11 => x11::GUARANTEES,
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
            #[cfg(feature = "x11")]
            Self::X11 => Rc::new(x11::configure(
                app, settings, session, views, submit, exit_code,
            )),
        }
    }
}

/// Which monitors still need a lock screen, and which screens no longer have a
/// monitor. Pure, because the display server used by the gates has one screen and
/// no way to add another: see `docs/testing.md`.
#[cfg(any(feature = "x11", test))]
fn reconcile<T: PartialEq + Clone>(monitors: &[T], covered: &[T]) -> (Vec<T>, Vec<T>) {
    let open = monitors
        .iter()
        .filter(|monitor| !covered.contains(monitor))
        .cloned()
        .collect();
    let close = covered
        .iter()
        .filter(|screen| !monitors.contains(screen))
        .cloned()
        .collect();
    (open, close)
}

/// A lock configured for one protocol.
pub trait LockBackend {
    /// Starts acquiring the lock. Ownership counts only once the backend calls
    /// [`Session::mark_locked`]; a failure terminates the session instead.
    fn lock(&self);

    /// Releases the lock. Call only after [`Session::complete_auth`] returned true.
    fn unlock(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitors_arriving_and_leaving_touch_only_their_own_screen() {
        assert_eq!(reconcile(&["a", "b"], &["a"]), (vec!["b"], vec![]));
        assert_eq!(reconcile(&["b"], &["a", "b"]), (vec![], vec!["a"]));
        assert_eq!(reconcile(&["a", "b"], &["a", "b"]), (vec![], vec![]));
        // A monitor swapped for another is one screen opened and one closed.
        assert_eq!(reconcile(&["b"], &["a"]), (vec!["b"], vec!["a"]));
        // The first lock: every monitor needs a screen.
        assert_eq!(reconcile(&["a", "b"], &[]), (vec!["a", "b"], vec![]));
    }

    #[test]
    fn wayland_keeps_both_guarantees() {
        let guarantees = Protocol::Wayland.guarantees();
        assert!(guarantees.survives_process_exit && guarantees.isolates_input);
    }
}
