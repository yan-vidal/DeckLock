//! Unlock authorization independent of widgets, themes and compositor callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Preview,
    Waiting,
    Locked,
    Authenticating(u64),
    Unlocked,
    Terminated,
}

pub struct Session {
    state: State,
    generation: u64,
}

impl Session {
    pub fn new(preview: bool) -> Self {
        Self {
            state: if preview {
                State::Preview
            } else {
                State::Waiting
            },
            generation: 0,
        }
    }

    /// Called only after the compositor confirms ownership of the session lock.
    pub fn mark_locked(&mut self) -> bool {
        if self.state != State::Waiting {
            return false;
        }
        self.state = State::Locked;
        true
    }

    pub fn begin_auth(&mut self) -> Option<u64> {
        if self.state != State::Locked {
            return None;
        }
        self.generation = self.generation.checked_add(1)?;
        self.state = State::Authenticating(self.generation);
        Some(self.generation)
    }

    /// A true return is the sole authorization to request compositor unlock.
    pub fn complete_auth(&mut self, attempt: u64, success: bool) -> bool {
        if self.state != State::Authenticating(attempt) {
            return false;
        }
        self.state = if success {
            State::Unlocked
        } else {
            State::Locked
        };
        success
    }

    /// Close, signal and compositor failure invalidate all outstanding attempts.
    pub fn terminate(&mut self) {
        self.state = State::Terminated;
    }

    pub fn is_authenticating(&self) -> bool {
        matches!(self.state, State::Authenticating(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_unconfirmed_lock_cannot_authenticate_or_unlock() {
        for preview in [true, false] {
            let mut session = Session::new(preview);
            assert_eq!(session.begin_auth(), None);
            assert!(!session.complete_auth(0, true));
        }
        assert!(!Session::new(true).mark_locked());
    }

    #[test]
    fn only_current_success_unlocks_once() {
        let mut session = Session::new(false);
        assert!(session.mark_locked());
        let first = session.begin_auth().unwrap();
        assert!(session.is_authenticating());
        assert_eq!(session.begin_auth(), None);
        assert!(!session.complete_auth(first, false));
        let second = session.begin_auth().unwrap();
        assert!(!session.complete_auth(first, true));
        assert!(session.complete_auth(second, true));
        assert!(!session.complete_auth(second, true));
        assert_eq!(session.begin_auth(), None);
    }

    #[test]
    fn termination_never_authorizes_unlock_even_after_pending_success() {
        let mut session = Session::new(false);
        session.mark_locked();
        let attempt = session.begin_auth().unwrap();
        session.terminate();
        assert!(!session.complete_auth(attempt, true));
        assert!(!session.mark_locked());
        assert_eq!(session.begin_auth(), None);
    }
}
