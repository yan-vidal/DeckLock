//! Best-effort reading of the local pam_faillock policy, for display only.
//!
//! Nothing here authorizes, denies or delays authentication: PAM remains the
//! only authority. Every source is optional, and anything unexpected yields
//! `None` so the lock screen stays silent rather than showing a wrong number.
use crate::i18n::I18n;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Values read from faillock.conf. Absent fields are never replaced by the
/// module's compiled-in defaults, which we cannot observe.
#[derive(Debug, Default, PartialEq)]
pub struct Policy {
    pub deny: Option<u32>,
}

/// Failures currently counted against the account, as reported by faillock(8).
/// Only the count is kept: the table prints local civil time, and converting it
/// would risk a wrong countdown to gain less than the minute PAM already gives.
#[derive(Debug, PartialEq)]
pub struct Failures {
    pub valid: u32,
}

/// What the lock screen may tell the user. Every field is independent.
#[derive(Debug, Default, PartialEq)]
pub struct Advice {
    /// PAM reported the account as locked, whether or not it said for how long.
    pub locked: bool,
    /// Remaining lockout, when PAM reported one.
    pub locked_for: Option<Duration>,
    /// Attempts before the lockout, only when both deny and failures are known.
    pub attempts_left: Option<u32>,
    /// PAM text we did not recognize, already sanitized for display.
    pub raw: Option<String>,
}

/// Read the documented keys from faillock.conf. Anything else is left unset.
pub fn parse_policy(config: &str) -> Policy {
    let mut policy = Policy::default();
    for line in config.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "deny" {
            policy.deny = value.trim().parse().ok();
        }
    }
    policy
}

/// Count the failures faillock(8) still considers valid. The table is the
/// module's own rendering of its private tally file, so an unexpected shape
/// means we say nothing rather than guess.
pub fn parse_failures(table: &str) -> Option<Failures> {
    let mut rows = table.lines().map(str::trim).filter(|line| !line.is_empty());
    rows.find(|line| line.starts_with("When") && line.ends_with("Valid"))?;
    let mut valid = 0;
    for row in rows {
        let (date, rest) = row.split_once(' ')?;
        if date.len() != 10 || !date.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        match rest.split_whitespace().next_back()? {
            "V" => valid += 1,
            "I" => {}
            _ => return None,
        }
    }
    Some(Failures { valid })
}

/// Turn what PAM said, plus whatever local policy we could confirm, into text
/// for the lock screen. Display only: nothing here gates an attempt.
pub fn assess(notice: Option<&str>, policy: &Policy, failures: Option<&Failures>) -> Advice {
    let mut advice = Advice {
        attempts_left: policy
            .deny
            .zip(failures)
            .map(|(deny, failures)| deny.saturating_sub(failures.valid)),
        ..Default::default()
    };
    if let Some(text) = notice {
        // pam_faillock's own wording, read with the helper pinned to LC_ALL=C.
        advice.locked = text.contains("account is locked");
        if advice.locked {
            advice.locked_for = remaining_minutes(text).map(|m| Duration::from_secs(m * 60));
        } else {
            advice.raw = Some(text.to_string());
        }
    }
    advice
}

/// Read "(7 minutes left to unlock)", singular included.
fn remaining_minutes(text: &str) -> Option<u64> {
    let tail = text.split_once(" minute")?.0;
    tail.rsplit(|c: char| !c.is_ascii_digit())
        .next()
        .filter(|digits| !digits.is_empty())
        .and_then(|digits| digits.parse().ok())
}

/// One line for the lock screen, or `None` when we have nothing to add to the
/// ordinary failure message. `remaining` is the live countdown, which the caller
/// decrements; it is ignored unless PAM actually reported a lockout.
pub fn describe(advice: &Advice, remaining: Option<Duration>, strings: &I18n) -> Option<String> {
    if advice.locked {
        return Some(match remaining.filter(|left| !left.is_zero()) {
            Some(left) => {
                let seconds = left.as_secs();
                strings
                    .text("auth-locked-in")
                    .replace("%s", &format!("{}:{:02}", seconds / 60, seconds % 60))
            }
            None => strings.text("auth-locked"),
        });
    }
    match advice.attempts_left {
        // Announcing "0 left" without a lockout PAM never reported would be us
        // predicting policy instead of repeating it.
        Some(0) => None,
        Some(1) => Some(strings.text("auth-attempts-left-one")),
        Some(left) => Some(
            strings
                .text("auth-attempts-left")
                .replace("%n", &left.to_string()),
        ),
        None => advice.raw.clone(),
    }
}

/// Read faillock.conf, treating an absent or unreadable file as "unknown".
pub fn policy_at(path: &Path) -> Policy {
    parse_policy(&std::fs::read_to_string(path).unwrap_or_default())
}

/// Ask faillock(8) for the current tally. Any failure means "unknown".
fn failures_via(command: &mut Command) -> Option<Failures> {
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| parse_failures(&String::from_utf8_lossy(&output.stdout)))?
}

/// Everything we can confirm about the local policy for this user. Reading it
/// costs one short-lived process and never blocks authentication.
pub fn local(user: &str) -> (Policy, Option<Failures>) {
    (
        policy_at(Path::new("/etc/security/faillock.conf")),
        failures_via(Command::new("faillock").args(["--user", user])),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() -> I18n {
        I18n::new(Some("en-US"), None).expect("built-in catalog")
    }

    #[test]
    fn a_countdown_is_rendered_as_minutes_and_seconds() {
        let advice = Advice {
            locked: true,
            locked_for: Some(Duration::from_secs(420)),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, Some(Duration::from_secs(419)), &english()).as_deref(),
            Some("Account locked by failed attempts. Try again in 6:59.")
        );
    }

    #[test]
    fn a_long_lockout_keeps_counting_in_minutes_rather_than_hours() {
        let advice = Advice {
            locked: true,
            locked_for: Some(Duration::from_secs(5400)),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, Some(Duration::from_secs(5400)), &english()).as_deref(),
            Some("Account locked by failed attempts. Try again in 90:00.")
        );
    }

    #[test]
    fn an_expired_countdown_drops_back_to_the_plain_lockout_notice() {
        let advice = Advice {
            locked: true,
            locked_for: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, Some(Duration::ZERO), &english()).as_deref(),
            Some("Account locked by failed attempts.")
        );
    }

    #[test]
    fn a_lockout_without_a_reported_time_still_says_it_is_locked() {
        let advice = Advice {
            locked: true,
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, None, &english()).as_deref(),
            Some("Account locked by failed attempts.")
        );
    }

    #[test]
    fn remaining_attempts_are_announced_while_the_account_still_works() {
        let advice = Advice {
            attempts_left: Some(2),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, None, &english()).as_deref(),
            Some("2 attempts left before the account is locked.")
        );
    }

    #[test]
    fn a_lockout_replaces_the_remaining_attempts_line() {
        let advice = Advice {
            locked: true,
            locked_for: Some(Duration::from_secs(120)),
            attempts_left: Some(0),
            raw: None,
        };
        assert_eq!(
            describe(&advice, Some(Duration::from_secs(120)), &english()).as_deref(),
            Some("Account locked by failed attempts. Try again in 2:00.")
        );
    }

    #[test]
    fn a_single_remaining_attempt_uses_the_singular_message() {
        let advice = Advice {
            attempts_left: Some(1),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, None, &english()).as_deref(),
            Some("1 attempt left before the account is locked.")
        );
    }

    #[test]
    fn no_attempts_left_without_a_reported_lockout_stays_quiet() {
        // PAM did not say the account is locked, so we do not claim it is.
        let advice = Advice {
            attempts_left: Some(0),
            ..Default::default()
        };
        assert_eq!(describe(&advice, None, &english()), None);
    }

    #[test]
    fn unrecognized_module_text_is_shown_as_it_came() {
        let advice = Advice {
            raw: Some("Your password will expire in 2 days.".into()),
            ..Default::default()
        };
        assert_eq!(
            describe(&advice, None, &english()).as_deref(),
            Some("Your password will expire in 2 days.")
        );
    }

    #[test]
    fn nothing_to_say_adds_no_line() {
        assert_eq!(describe(&Advice::default(), None, &english()), None);
    }

    #[test]
    fn a_missing_configuration_file_is_not_an_error() {
        assert_eq!(
            policy_at(Path::new("/nonexistent/decklock/faillock.conf")),
            Policy::default()
        );
    }

    #[test]
    fn a_present_configuration_file_is_read() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("faillock.conf");
        std::fs::write(&path, "deny = 5\n").unwrap();
        assert_eq!(policy_at(&path), Policy { deny: Some(5) });
    }

    #[test]
    fn the_tally_comes_from_the_tools_own_output() {
        let table = "yan:\nWhen                Type  Source  Valid\n\
                     2026-09-09 08:00:00 TTY   tty2    V\n";
        assert_eq!(
            failures_via(Command::new("/bin/sh").args(["-c", &format!("printf '{table}'")])),
            Some(Failures { valid: 1 })
        );
    }

    #[test]
    fn a_missing_or_failing_tool_leaves_the_tally_unknown() {
        for command in [
            vec!["/does/not/exist"],
            vec!["/bin/sh", "-c", "exit 1"],
            vec!["/bin/sh", "-c", "printf 'unexpected'"],
        ] {
            assert_eq!(
                failures_via(Command::new(command[0]).args(&command[1..])),
                None,
                "{command:?}"
            );
        }
    }

    #[test]
    fn policy_reads_documented_keys_and_ignores_the_rest() {
        let config = "# comment\ndeny = 4\nunlock_time=900\nnodelay\naudit\n";
        assert_eq!(parse_policy(config), Policy { deny: Some(4) });
    }

    #[test]
    fn policy_stays_empty_when_the_file_says_nothing_usable() {
        for config in [
            "",
            "# only comments\n",
            "deny\n",
            "deny = many\n",
            "deny = -1\n",
        ] {
            assert_eq!(parse_policy(config), Policy::default(), "{config:?}");
        }
    }

    #[test]
    fn failures_count_only_valid_rows() {
        let table = "yan:\n\
             When                Type  Source            Valid\n\
             2026-09-09 08:00:00 TTY   tty2              V\n\
             2026-09-09 08:01:30 TTY   tty2              I\n\
             2026-09-09 08:02:00 TTY   tty2              V\n";
        assert_eq!(parse_failures(table), Some(Failures { valid: 2 }));
    }

    #[test]
    fn failures_report_an_empty_but_well_formed_table() {
        let table = "yan:\nWhen                Type  Source            Valid\n";
        assert_eq!(parse_failures(table), Some(Failures { valid: 0 }));
    }

    #[test]
    fn failures_reject_output_that_is_not_the_expected_table() {
        for table in ["", "faillock: Unknown option\n", "yan:\ngarbage row here\n"] {
            assert_eq!(parse_failures(table), None, "{table:?}");
        }
    }

    #[test]
    fn locked_message_becomes_a_countdown() {
        let advice = assess(
            Some("The account is locked due to 3 failed logins. (7 minutes left to unlock)"),
            &Policy::default(),
            None,
        );
        assert!(advice.locked);
        assert_eq!(advice.locked_for, Some(Duration::from_secs(7 * 60)));
        assert_eq!(advice.raw, None);
    }

    #[test]
    fn a_single_remaining_minute_uses_the_singular_message() {
        let advice = assess(
            Some("The account is locked due to 3 failed logins. (1 minute left to unlock)"),
            &Policy::default(),
            None,
        );
        assert_eq!(advice.locked_for, Some(Duration::from_secs(60)));
    }

    #[test]
    fn a_lockout_without_a_remaining_time_reports_no_countdown() {
        let advice = assess(
            Some("The account is locked due to 3 failed logins."),
            &Policy::default(),
            None,
        );
        assert!(advice.locked);
        assert_eq!(advice.locked_for, None);
        assert_eq!(advice.raw, None);
    }

    #[test]
    fn attempts_left_need_both_the_threshold_and_the_failures() {
        let failures = Failures { valid: 1 };
        let known = assess(None, &Policy { deny: Some(3) }, Some(&failures));
        assert_eq!(known.attempts_left, Some(2));
        let without_threshold = assess(None, &Policy::default(), Some(&failures));
        assert_eq!(without_threshold.attempts_left, None);
        let without_failures = assess(None, &Policy { deny: Some(3) }, None);
        assert_eq!(without_failures.attempts_left, None);
    }

    #[test]
    fn a_reached_threshold_never_reports_a_negative_countdown_of_attempts() {
        let advice = assess(
            None,
            &Policy { deny: Some(3) },
            Some(&Failures { valid: 5 }),
        );
        assert_eq!(advice.attempts_left, Some(0));
    }

    #[test]
    fn unrecognized_pam_text_is_passed_through_for_display() {
        let advice = assess(
            Some("Your password will expire in 2 days."),
            &Policy::default(),
            None,
        );
        assert_eq!(
            advice.raw.as_deref(),
            Some("Your password will expire in 2 days.")
        );
        assert!(!advice.locked);
        assert_eq!(advice.locked_for, None);
    }

    #[test]
    fn no_pam_text_and_no_policy_says_nothing() {
        assert_eq!(assess(None, &Policy::default(), None), Advice::default());
    }
}
