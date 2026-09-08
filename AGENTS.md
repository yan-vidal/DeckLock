# DeckLock: change and verification contract

Read `.specs/project/STATE.md` and `docs/testing.md` before changing behavior.
This project is a session locker: UI success is not evidence of unlock safety.

## Non-negotiable behavior

- Only a successful result for the current authentication attempt, after compositor
  lock ownership is confirmed, may authorize unlock. Preview, stale results,
  signals, window close, compositor failure and termination must never authorize it.
- Preview/settings must not acquire a lock, invoke PAM/power actions or capture a
  controller automatically. Tests must use the isolated gate, never the user's
  Wayland socket, PAM credentials, power actions or real controller daemon.
- Invalid config/theme edits must preserve valid saved data. Saving must retain
  unrelated options. Media removal from a pool must not delete source files.
- Preserve CLI and GUI parity, existing theme/config compatibility, keyboard
  Shift/Caps/Alt behavior, pad release and the single embedded keyboard.

## Working rules

1. Add a failing regression for a behavioral bug before implementing its fix.
   Test the public boundary that failed (real CLI binary, GTK, or protocol), not
   just an internal helper. Documentation-only edits need no artificial unit tests.
2. Run `scripts/check` for code changes. Run `scripts/check --all` for GTK,
   controller, media or session changes. CI requires the full suite for every PR.
   Missing dependencies and timeouts are failures, not skips or passing results.
3. Use fixed inputs/seeds, temporary HOME/XDG directories, fake daemons and bounded
   waits. Never rely on the developer's media, desktop, current language or mouse.
4. Do not delete, ignore, loosen or retry a failing assertion just to get green.
   If a user-approved behavior changes, update its documented contract and tests
   together and explain the change. Diagnose infrastructure failures separately.
5. `target/check-logs/results.json` and step logs are verification evidence. Report
   the exact tested commit, failures and untested boundaries honestly. Screenshots
   and showcase scripts are documentation, not substitutes for assertions.
6. Changes to tests, CI, release scripts and this file are reviewable product
   changes. Explain any reduction of coverage; never bypass required checks.

## Merge and release

Use a branch/PR. Required checks on main must pass; do not force-push, directly
push to main, disable protection, or use administrator bypass to land changes.
No second person's approval is required by this file; preserve user-authorized
changes and let CI enforce the checks.

PRs produce candidate artifacts, not public releases. Releases come from version
or package-revision tags after merge. `release.yml` reruns checks, builds on Arch,
checks artifact hashes and publishes. Never reuse a local/unverified executable,
move an existing tag, overwrite a published asset, or invent an artwork license.
If emergency manual publishing is explicitly requested, record why and validate
that exact package before upload.

A passing suite does not establish real-session PAM, every compositor, hardware
controller behavior, visual aesthetics or absence of security vulnerabilities.
Keep those validation limits explicit. See `docs/testing.md` for coverage and UAT.
