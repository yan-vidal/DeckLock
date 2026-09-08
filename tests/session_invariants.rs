//! Bounded exhaustive event sequences. Fixed enumeration, no random seed or timing.
use decklock::session::Session;
#[test]
fn only_a_current_success_after_confirmed_ownership_can_unlock() {
    // Seven operations, all sequences of length seven, in preview and real mode.
    for preview in [true, false] {
        for encoded in 0..7usize.pow(7) {
            let mut sequence = encoded;
            let mut session = Session::new(preview);
            let (mut confirmed, mut terminated, mut unlocked) = (false, false, false);
            let mut pending = None;
            let mut last_issued = 0;
            for _ in 0..7 {
                let operation = sequence % 7;
                sequence /= 7;
                match operation {
                    0 => {
                        let allowed = !preview && !confirmed && !terminated && !unlocked;
                        assert_eq!(session.mark_locked(), allowed, "sequence {encoded}");
                        confirmed |= allowed;
                    }
                    1 => {
                        let attempt = session.begin_auth();
                        assert_eq!(
                            attempt.is_some(),
                            confirmed && !terminated && !unlocked && pending.is_none(),
                            "sequence {encoded}"
                        );
                        if let Some(id) = attempt {
                            assert!(id > last_issued);
                            last_issued = id;
                            pending = Some(id);
                        }
                    }
                    2..=5 => {
                        let attempt = match operation {
                            2 | 3 => last_issued,
                            4 => last_issued.saturating_sub(1),
                            _ => u64::MAX,
                        };
                        let success = operation != 3;
                        let current =
                            pending == Some(attempt) && confirmed && !terminated && !unlocked;
                        let granted = session.complete_auth(attempt, success);
                        assert_eq!(
                            granted,
                            current && success,
                            "sequence {encoded}, op {operation}"
                        );
                        if current {
                            pending = None;
                            unlocked |= success;
                        }
                    }
                    _ => {
                        session.terminate();
                        terminated = true;
                        pending = None;
                    }
                }
                assert_eq!(
                    session.is_authenticating(),
                    pending.is_some() && !terminated && !unlocked
                );
            }
        }
    }
}
#[test]
fn every_media_choice_stays_in_its_selected_playback_mode() {
    use decklock::library::Selection;
    let pool: Vec<_> = ["a.png", "b.mp4", "c.jpg", "d.webm"]
        .into_iter()
        .map(Into::into)
        .collect();
    for seed in 0..64 {
        let mut selection = Selection::new(pool.clone(), seed);
        let first = selection.current().unwrap().to_owned();
        for _ in 0..12 {
            if seed % 2 == 1 {
                assert!(!selection.advance());
                assert_eq!(selection.current(), Some(first.as_path()));
            } else {
                assert!(selection.advance());
                assert!(matches!(
                    selection.current().unwrap().extension().unwrap().to_str(),
                    Some("png" | "jpg")
                ));
            }
        }
        assert_eq!(selection.current(), Some(first.as_path()));
    }
}
