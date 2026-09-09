# Verification and regression boundaries

## Assessment

DeckLock has meaningful behavior tests, now connected to one required pipeline.
This is a regression baseline, not a claim that the experimental locker is
security-audited or mature across Linux desktops. The 0.1 CLI defect demonstrated
why helper tests alone were insufficient: its parser rejected a documented
combination even though config unit tests passed.

## One command

```sh
scripts/check         # format, clippy, Rust tests, real CLI, archive contract
scripts/check --all   # additionally isolated GTK and real Wayland lock protocol
```

Run `cargo fetch --locked` (or `scripts/cargo-local fetch --locked`) and
`scripts/bootstrap-native --tests` first. Full GTK checks require `xvfb-run`, Xvfb,
xauth and `dbus-run-session`, in addition to the normal build dependencies/codecs.
On Arch these are provided by `xorg-server-xvfb`, `xorg-xauth` and `dbus`.
Missing tools fail the gate; none of the required suites silently skip.

The gate creates private HOME, XDG config/data/cache/runtime directories and clears
the inherited desktop sockets. Cargo/Rustup caches remain usable. Logs and exit
codes are retained under `target/check-logs`; each failure stops the pipeline.
Subprocess deadlines terminate the owned process group rather than leaking clients.

GTK widget tests use a private Xvfb display and D-Bus session. This tests shared
GTK behavior without touching the desktop; it does **not** add X11 lock support.
The separate protocol test explicitly uses gtk4-layer-shell's pinned mock Wayland
compositor, its own socket and no PAM authentication. They cover different layers.

## Contracts

| Boundary | Evidence | Detects |
| --- | --- | --- |
| Unlock authorization | `src/session.rs`, `tests/session_invariants.rs` | No unlock before ownership, in preview, after termination, on stale/failed replies or twice |
| Event ordering | All 7-operation sequences of length 7 in both session modes | 1,647,086 fixed sequences; no random seed or timing |
| CLI executable | `tests/cli_contract.rs`, `src/main.rs` parser regression | Help without a display, alternate paths before/after subcommands, option round trips, invalid-write protection, import/reset |
| Configuration | `src/config.rs`, `src/config_cli.rs` | Schema/ranges, path resolution, defaults, preservation and atomic saves |
| Controller protocol | `src/controller.rs`, shortcut mock | Fragmented lines, no unsolicited capture, capture/release and no separate OSK |
| Keyboard | `src/keyboard.rs`, `examples/preview_check.rs` | Shift/Caps/Alt, Unicode/dead keys, ghost opacity and pad release |
| GTK settings | `examples/settings_check.rs`, `settings_live_check.rs` | Theme contrast providers, language, preview identity, idle clock, draft errors/persistence, cleanup |
| Media | `src/library.rs`, invariants, `media_check`, `video_live_check` | Seeded photo/video selection, imports without overwrites, viewer reuse, video retained on palette changes |
| Lock protocol | `scripts/test-lock-isolated.py` | Ownership, output hotplug/remove/re-add, SIGTERM without unlock, second-lock refusal |
| Package boundary | `scripts/test-package.py`, Arch package CI | Archive paths/checksums, executable identity, original media, settings launcher and actual packaged CLI |

Counts are not a coverage target. Add a contract test when a behavior can break;
do not mirror implementation lines, pad the count or regenerate snapshots to hide
a regression. The previously published CLI problem now has both parser and
subprocess regression tests, including the packaged executable.

## Determinism and honest limits

Pure state/config/keyboard tests have fixed inputs. Media selection is exercised
with explicit seeds. Protocol tests use fake peers and read complete lines, not
assumed socket packet boundaries. GTK tests have bounded waits for asynchronous
work; scheduling and native libraries can still expose timing failures. Diagnose
those instead of adding blanket retries.

Cargo.lock, Rust 1.93.0 in the compatibility gate, action commit SHAs, an Arch image
digest, and the native mock archive/hash are pinned. OS security updates and Arch
runtime packages still change; this is not a bit-for-bit reproducible build claim.
The Ubuntu gate tests compatibility; publishable binaries are built separately
on Arch and include their linked-library provenance.

## GitHub flow

- PR targeting main: required **DeckLock checks** and **Arch package contract**.
  Download the `arch-package` candidate from the workflow's artifacts if needed.
- Main: the same checks after merge. No public release just because a PR exists.
- Version/revision tag: `Release` invokes the checks and Arch package build, then
  verifies the tag matches Cargo/package metadata and the commit belongs to main.
  Only then are checksummed assets published as an experimental prerelease.
- CI has read-only repository permissions except the final tag-publishing job.
  PR code receives no publishing token. Actions are pinned; checkout credentials
  are not persisted. Existing release tags/assets are never overwritten.

`AGENTS.md` is guidance, not a security boundary. Required GitHub checks and branch
protection enforce the merge policy. A repository administrator can still change
these policies; changes to safeguards must be reviewed as such.

Cargo `0.2.0` / package revision `1` are in preparation on the draft PR.
No release tag or publication is authorized yet. See `docs/releases.md`.

Procedural media add deterministic frame/validation tests, actual CLI
round trips and a private GTK contract for texture updates, bounded dimensions,
unmap/drop cleanup, library/pool selection, per-item eye/gear drafts, settings saves and rest preview. Rendering performance and
battery use on physical GPUs still require measurement.

## Manual validation still required

Real-session PAM (including account policies), suspend/hibernate integration,
compositor behavior beyond the mock, controller recovery/haptics, accessibility,
real-device ergonomics and visual quality need explicit UAT. Use a recoverable
test environment for session-lock/PAM work. Never use an active desktop or actual
power actions as an agent's automatic test target.
