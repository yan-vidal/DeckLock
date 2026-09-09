//! Display-only PAM notices. Do not infer policy from faillock.conf or tally flags.
use crate::i18n::I18n;
use std::time::Duration;

#[derive(Debug, Default, PartialEq)]
pub struct Advice {
    pub locked: bool,
    /// PAM's rounded-up estimate, not a promise that the next attempt succeeds.
    pub locked_for: Option<Duration>,
    /// Unrecognized, already sanitized module text; display without markup.
    pub raw: Option<String>,
}

pub fn assess(notice: Option<&str>) -> Advice {
    let Some(text) = notice else {
        return Advice::default();
    };
    let lock = text
        .split_once("The account is locked due to ")
        .and_then(|(_, tail)| tail.split_once(" failed logins."))
        .filter(|(count, _)| count.parse::<u32>().is_ok());
    let Some((_, tail)) = lock else {
        return Advice {
            raw: Some(text.into()),
            ..Default::default()
        };
    };
    let minutes = tail.trim_start().strip_prefix('(').and_then(|tail| {
        let (number, suffix) = tail.split_once(' ')?;
        (suffix.starts_with("minute left to unlock)")
            || suffix.starts_with("minutes left to unlock)"))
        .then(|| number.parse::<u64>().ok())
        .flatten()
    });
    Advice {
        locked: true,
        locked_for: minutes
            .and_then(|m| m.checked_mul(60))
            .map(Duration::from_secs),
        raw: None,
    }
}

pub fn describe(advice: &Advice, remaining: Option<Duration>, strings: &I18n) -> Option<String> {
    if !advice.locked {
        return advice.raw.clone();
    }
    Some(match remaining.filter(|left| !left.is_zero()) {
        Some(left) => strings.text("auth-locked-in").replace(
            "%s",
            &format!("{}:{:02}", left.as_secs() / 60, left.as_secs() % 60),
        ),
        None => strings.text("auth-locked"),
    })
}

/// Linux's monotonic clock including suspension, without changing system time.
pub fn boot_time() -> Option<Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: valid writable timespec, read-only clock operation.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return None;
    }
    Some(Duration::new(
        time.tv_sec.try_into().ok()?,
        time.tv_nsec.try_into().ok()?,
    ))
}

pub struct Countdown {
    deadline: Duration,
}
impl Countdown {
    pub fn new(now: Duration, total: Duration) -> Option<Self> {
        Some(Self {
            deadline: now.checked_add(total)?,
        })
    }
    /// Round up the displayed second; missed ticks do not extend the deadline.
    pub fn remaining(&self, now: Duration) -> Duration {
        let left = self.deadline.saturating_sub(now);
        Duration::from_secs(
            left.as_secs()
                .saturating_add(u64::from(left.subsec_nanos() != 0)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_notice_cannot_predict_policy_or_remaining_attempts() {
        assert_eq!(assess(None), Advice::default());
    }
    #[test]
    fn only_the_pam_lockout_message_is_interpreted() {
        for (minutes, suffix) in [(1, "minute"), (7, "minutes")] {
            let notice = format!(
                "The account is locked due to 3 failed logins. ({minutes} {suffix} left to unlock)"
            );
            let advice = assess(Some(&notice));
            assert!(advice.locked);
            assert_eq!(advice.locked_for, Some(Duration::from_secs(minutes * 60)));
            assert_eq!(advice.raw, None);
        }
        for text in [
            "The account is locked due to 3 failed logins.",
            "The account is locked due to 3 failed logins. (18446744073709551615 minutes left to unlock)",
            "The account is locked due to 3 failed logins. (bad minutes left to unlock)",
        ] {
            let advice = assess(Some(text));
            assert!(advice.locked);
            assert_eq!(advice.locked_for, None);
        }
    }
    #[test]
    fn unrelated_minutes_and_module_text_are_not_mistaken_for_a_lockout() {
        let notice = "Maintenance in 60 minutes. The account is locked due to 3 failed logins. (1 minute left to unlock)";
        assert_eq!(
            assess(Some(notice)).locked_for,
            Some(Duration::from_secs(60))
        );
        for text in [
            "Your password will expire in 2 days.",
            "1 attempt left.",
            "No account is locked.",
        ] {
            let advice = assess(Some(text));
            assert!(!advice.locked);
            assert_eq!(advice.raw.as_deref(), Some(text));
        }
    }
    #[test]
    fn countdown_uses_elapsed_time_even_when_callbacks_or_suspend_skip_ticks() {
        let timer = Countdown::new(Duration::from_secs(100), Duration::from_secs(120)).unwrap();
        assert_eq!(
            timer.remaining(Duration::from_millis(101100)),
            Duration::from_secs(119)
        );
        assert_eq!(
            timer.remaining(Duration::from_secs(190)),
            Duration::from_secs(30)
        );
        assert_eq!(timer.remaining(Duration::from_secs(300)), Duration::ZERO);
        assert!(Countdown::new(Duration::MAX, Duration::from_secs(1)).is_none());
        assert!(boot_time().is_some());
    }
    #[test]
    fn descriptions_preserve_both_locales_and_do_not_authorize_unlock_at_expiry() {
        let advice = assess(Some(
            "The account is locked due to 3 failed logins. (90 minutes left to unlock)",
        ));
        for locale in ["en-US", "pt-BR"] {
            let strings = I18n::new(Some(locale), None).unwrap();
            assert!(
                describe(&advice, Some(Duration::from_secs(5399)), &strings)
                    .unwrap()
                    .contains("89:59")
            );
            assert_eq!(
                describe(&advice, Some(Duration::ZERO), &strings),
                Some(strings.text("auth-locked"))
            );
            assert_eq!(describe(&Advice::default(), None, &strings), None);
            let raw = Advice {
                raw: Some("Module message".into()),
                ..Default::default()
            };
            assert_eq!(
                describe(&raw, None, &strings).as_deref(),
                Some("Module message")
            );
        }
    }
}
