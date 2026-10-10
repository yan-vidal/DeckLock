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

The separate real Sway/PAM VM gate installs each distribution's own candidate
package and tests actual password rejection, acceptance, account lockout and
fail-closed termination. It runs once per target -- Arch, Fedora and Ubuntu --
each in its own pinned cloud image, so a distribution's authentication stack is
exercised rather than assumed. See [VM testing](vm-testing.md) for the command,
isolation and evidence. It runs after packaging in CI; `scripts/check --all`
remains the lightweight gates listed by `target/check-logs/results.json`.

Run `cargo fetch --locked` (or `scripts/cargo-local fetch --locked`) and
`scripts/bootstrap-native --tests` first. Full GTK checks require `xvfb-run`, Xvfb,
xauth and `dbus-run-session`, in addition to the normal build dependencies/codecs.
On Arch these are provided by `xorg-server-xvfb`, `xorg-xauth` and `dbus`.
The X11 lock gate additionally needs `xfwm4` as the window manager, `xset` from
`xorg-xset`/`x11-xserver-utils`, and the Xtst, Xi and Xss client libraries used by
its helper. Missing tools fail the gate; none of the required suites silently skip.
The `x11` build also requires the Xlib/XCB development bridge (`libx11-xcb-dev`
on Ubuntu, `libX11-devel` and `libxcb-devel` on Fedora, `libx11` and `libxcb` on
Arch). Package library scans retain those runtime dependencies.

The X11 gate builds a small C fixture with `cc` and preloads it into the real
locker only, on its private Xvfb. A private marker delays main-thread `XPending`
calls by 150 ms to model GTK frame work; an assertion confirms the delay actually
ran. The ordinary intruder/WM stay independent. Both the normal and delayed
competition require at least 12 of 20 topmost samples, with no relaxed assertions.
The stacking guard owns a separate worker/connection and is joined before unlock;
GTK retains input grabs, focus and authentication. Owned popups are identified
from the GDK client's server-provided resource range, not copied window properties.

The optional `x11` feature is off for a plain local build; distribution package
jobs build with it enabled. `scripts/check` also lints and builds that feature
into `target/x11`. The VM's X11/PAM assertions exercise the packaged binary.

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
| Greeter controller ownership | `scripts/test-greeter-controller.py`, real CLI/private GTK | Explicit setup opt-in, private daemon/profile, no desktop config changes, no daemon in preview/without greetd/default login, capture only for a visible keyboard, release when hidden, missing dependency/failed startup/timeout fallback, graceful exit, stalled shutdown, SIGTERM and SIGKILL cleanup |
| Adaptive foreground | `examples/preview_check.rs` | Fixed 1280×800 and 800×500 allocations, proportional controls, fullscreen media, transformed keyboard picking/clicks and destroyed-widget cleanup on a private display |
| Combined setup | `tests/cli_contract.rs`, `scripts/vm/greeter.py` | Real CLI dry-run, option forwarding, invalid-input preservation and repeat; packaged VM checks sudo caller HOME, file/backup ownership and permissions instead of root configuration |
| Keyboard | `src/keyboard.rs`, `examples/preview_check.rs` | Shift/Caps/Alt, Unicode/dead keys, ghost opacity, pad release, and A confirming like Enter while the keyboard holds the controller |
| Battery level | `src/battery.rs`, `examples/preview_check.rs` | A fixture power-supply directory, never the host's battery: mains adapters, absent cells and device-scope (controller) batteries ignored, several batteries averaged, capacity bounded, icon per ten and charging state; on lock, login and preview the level sits on its own row above the power buttons, right-aligned with them, and that row also lowers the help button, which lines up with the power buttons; without a battery the row takes no space; the preview banner shares that row; theme CSS margins on `#battery` and `#top-bar` move it; nothing without a battery or without a source; hidden in rest mode; the layout's icon-only mode keeps the level in the tooltip; `.low` and `.charging` classes; icon size from a theme CSS rule rather than a fixed size. The settings preview passes the same system directory as the lock screen; no test reads the host battery |
| Screen help | `examples/preview_check.rs`, `examples/help_check.rs` | No title over the login; a question-mark button in the top-left corner of the lock, login and preview (below the preview banner) whose F1/click shows a short guide inside the same window, never another window, in English and Portuguese, without the full guide's chapters and naming no commands, system paths, PAM, greetd or systemctl; Esc and its × close it without closing a preview; rest mode hides it; the lock stays undecorated; the settings window keeps the full guide behind its own question mark |
| GTK settings | `examples/settings_check.rs`, `settings_live_check.rs` | Theme contrast providers, language, preview identity, idle clock, draft errors/persistence, the battery percentage option with save and restore, cleanup |
| Media | `src/library.rs`, invariants, `media_check`, `video_live_check` | Seeded photo/video selection, imports without overwrites, viewer reuse, video retained on palette changes |
| Lock protocol | `scripts/test-lock-isolated.py` | Ownership, output hotplug/remove/re-add, SIGTERM without unlock, second-lock refusal |
| X11 lock backend | `scripts/test-lock-x11.py`, `examples/x11_intruder` | Override-redirect window covering the screen, grabs another client cannot take, stacking and focus recovered from an intruding window, stacking independent of deliberately slow GTK event processing, blank/wake, refusal when the keyboard cannot be grabbed, and the two gaps X11 leaves |
| Video path on X11 | `examples/x11_video_check.rs`, run by both gates | Frames keep arriving on an X11 display, through the software path without the `x11` feature and the accelerated one with it |
| Reduced-guarantee notice | `examples/guarantee_check.rs` | Catalog text, silence when a backend keeps its guarantees, and a theme that cannot hide it |
| Guest provisioning fixture | `scripts/test-vm-fixture.py` | The readiness decision made before a guest is tested, and the cloud-init status recorded with it, checked against a stubbed cloud-init. The suite selector `scripts/vm/suites.sh`, sourced as `setup.sh` sources it: no selection runs every suite, a selection runs exactly its suites in the guest's own order, and unknown or malformed names are refused; `setup.sh` validates it before any work and runs each suite's commands under that suite's guard only |
| Candidate awaiter | `scripts/test-ci-await-candidate.py` | The real `ci-await-candidate` CLI against a fake `gh`, never GitHub: nothing is downloaded until the named job has succeeded in this run attempt; a job that fails, is cancelled, skipped or times out ends the wait at once; the wait is bounded; only the named job counts, also under a caller's prefix, and two jobs with the name are refused; a transient listing error is retried while persistent ones, a missing `gh` or a missing run identity end it early; `SHA256SUMS` must exist, list files and match; a destination that already holds files is refused |
| VM runner CPU plan | `scripts/test-vm-emulation.py` | The real `test-vm.py` CLI behind PATH stubs and a host-boundary guard: `--print-plan` reports the pinned image and CPU options without reading `/dev/kvm`, starting a process, opening a socket or writing a file; an emulated aarch64 guest gets `max` with pointer authentication enabled and QEMU's implementation-defined algorithm, KVM the host CPU and emulated x86 plain `max`; the comparison's `--tcg-pauth` selector is refused; unpinned targets are refused; execution still requires a candidate, with or without `--package-fetch`, whose command a plan never runs; a guest runs every suite unless `--suites` selects some, the plan names the completion marks owed for them, and a malformed selection is refused. It never starts QEMU |
| Greeter IPC and preview | `src/greeter.rs` protocol tests, `examples/preview_check.rs` | Several greetd prompts, denied session start, JSON parsing, no power actions in greeter preview, and activation of an existing greeter |
| Greeter arrow and Enter keys | `examples/greeter_keys_check.rs`, X11 gate | Left/Right from an empty password entry select accounts through GTK's own key dispatch, wrap at both ends, keep entry focus, and still move the cursor while a password is being typed; Enter submits the typed password |
| Greeter setup command | `tests/cli_contract.rs` | The command `setup greeter` writes starts when run as greetd runs it (`sh -c "exec <command>"`), with a fake Cage. Replacing a greetd config keeps its mode (0644 for a file setup creates), and `config set` keeps the mode of the config it replaces, because the atomic write goes through a private temporary file |
| Greeter state directory and default session | `src/greeter.rs` unit tests, `src/setup.rs` unit tests, `tests/cli_contract.rs` | Under greetd the state file goes to the system directory when it exists (an explicit path and `XDG_STATE_HOME` still win, and outside greetd it is ignored); a uwsm-managed session is preselected over the bare one only when `uwsm` exists; `setup greeter --state-dir` creates the directory with mode 0750, does nothing in a dry run, and a custom `--target` alone creates nothing. The packaged service account is preserved; traversal/writing as that account, rejection of inaccessible ancestors before changing greetd, ownership, HOME=/ startup and the enforcing SELinux display-manager storage label are additionally asserted by the packaged VM gate |
| Shared login media and private appearance | `tests/cli_contract.rs`, `scripts/vm/greeter.py` | Real CLI migrates one canonical library, checks inode/aliases, cross-filesystem bytes, deduplication/recovery, old snapshot conversion/rejection preserving installed appearance, CLI import, empty pools/procedurals, dry-run/repeat, failed-input preservation and appearance override. Packaged guest asserts owner-write/world-read modes, private HOME traversal denial, shared pool paths, SELinux mapping, separate common HOME volume selection/ineligible-parent or named-ACL fallback and a cyan photo rendered by real Cage with HOME=/; root-only selected input is rejected before replacing saved login/appearance |
| Greeter startup and kiosk | `tests/cli_contract.rs`, `examples/preview_check.rs` | Unwritable HOME reaches GTK rather than aborting on media preparation; optional media failures name their path; greeter windows are undecorated; fullscreen is requested after the initial surface frame and acknowledged by a private xfwm4 in the GTK gate |
| Hyprland greeter setup | `tests/cli_contract.rs`, `src/setup.rs`, `scripts/vm/greeter.py` (Arch guest) | Generated Lua rotates output and touchscreen, closes the compositor after DeckLock, preserves a working Hyprland command on repeat, refuses invalid rotation and passes custom storage paths through greetd shell quoting. Real CLI verification uses temporary owner-only HOME/XDG storage that also holds the configuration, even without sudo's runtime environment, excludes desktop/greeter sockets, removes temporary artifacts and preserves saved greetd/Lua on rejection; its verifier is a fake that accepts any caller. Root setup verifies as greetd's account, never as root or a guessed account. The packaged Arch VM runs that setup as root against the installed Hyprland, without a runtime directory and with a TMPDIR private to root. This does not prove GPU or real Hyprland login, and Fedora and Ubuntu guests install no Hyprland |
| Lock setup command | `src/setup.rs` unit tests, `tests/cli_contract.rs` | Editing an existing `hypridle.conf`: exact key matching (`on_lock_cmd` untouched), known lockers replaced in `general` and `listener` blocks, trailing comments kept, custom wrappers and compound commands kept and reported until `--replace-custom` (including shell parameter braces and guards for unrelated processes), listeners that are not the lock command never replaced, inserting missing keys without corrupting shell expressions or losing listener edits, idempotence, atomic replacement preserving file mode, and a failed backup preserving the original. Never touches the real `~/.config`: every case uses `--target` in a temporary directory |
| Real greetd login | `scripts/vm/greeter.py`, run by each distribution VM | Packaged setup command, greetd as a system service, logind runtime directories for the greeter and user sessions, real greetd and PAM rejection/acceptance, Cage Wayland greeter, chosen session running as the authenticated user, and a second account selected by arrow key |
| Lock boundary on further compositors | `scripts/vm/compositor.py`, run by each distribution VM for its `EXTRA_COMPOSITORS` (labwc with pixman, Wayfire with software GLES on vgem) | Input reaching an ordinary client before lock, compositor lock confirmation, real PAM denial with no input leak, one protocol unlock, input restored, and a killed locker leaving the compositor locked. PAM policy cases stay on Sway only |
| aarch64 packages | `package-fedora-aarch64` and `package-ubuntu-aarch64` jobs on `ubuntu-24.04-arm` | The same release build, `cli_contract` and `session_invariants` tests and package contract as x86_64. Ubuntu arm64 is published from 0.3.2; Fedora arm64 stays a candidate until it has a VM target |
| aarch64 VM gate | `vm-ubuntu-aarch64` on `ubuntu-24.04-arm`, three guests, `scripts/test-vm.py --arch aarch64 --allow-emulation --suites ...` | The whole Ubuntu guest suite (Sway with real PAM, X11, labwc, Wayfire, greetd) on the arm64 package, shared between three TCG-emulated guests so that each suite runs in exactly one of them (session; package reinstall with the further compositors; greeter), each guest installing the candidate itself and held to the completion marks of its own suites and no other. Each is a TCG-emulated guest with every bounded wait scaled by 8, after the harness packages are installed on the host (`--provision-native`). The emulated CPU keeps pointer authentication but signs with QEMU's implementation-defined algorithm instead of QARMA5, so the gate is no evidence of pointer-authentication strength; about 18 minutes, down from about 35 with QARMA5 and about 94 before native provisioning. The job starts with the package build instead of after it: the guest boots while `scripts/ci-await-candidate` waits for that job to succeed, and nothing is installed before its artifact is downloaded and checked against `SHA256SUMS`. Fedora arm64 has no VM target yet |
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

- PR targeting main: required **DeckLock checks** (all isolated, package and VM jobs) and **Arch package contract**.
  Fedora and Ubuntu packages also build. Download any `<target>-package`
  candidate from the workflow's artifacts if needed.
- Package builds start alongside the isolated suite. After each package: **Real Wayland and PAM** for each target, using that exact
  package in QEMU/KVM. Every package and VM is required by the final **DeckLock checks** aggregator. Failed, skipped or cancelled gates fail the aggregator.
- Main: the same checks after merge. No public release just because a PR exists.
- Version/revision tag: `Release` invokes the checks and Arch package build, then
  verifies the tag matches Cargo/package metadata and the commit belongs to main.
  Only then are checksummed assets published as an experimental prerelease.
- CI has read-only repository permissions except the final tag-publishing job.
  PR code receives no publishing token. Actions are pinned; checkout credentials
  are not persisted. Existing release tags/assets are never overwritten.

### CI preparation and caches

The native compatibility gate builds a dependency-only Ubuntu 24.04 image from
`.github/ci/native-tests.Dockerfile`. Its Ubuntu and Rust 1.93.0 bases are pinned
by digest. A small build context includes only the recipe, entrypoint and
`bootstrap-native`; no project executable or source is baked into the image.
Docker BuildKit caches those layers in GitHub Actions. The UTC ISO week is a
build argument before apt/native preparation, so the first run of a new week
refreshes packages. Recipe changes also invalidate the affected layers. Cold
caches build normally; image build failure fails the native gate. Cache eviction
only removes the speed benefit.

The gate mounts its exact checkout, creates a container account matching its
owner and runs `scripts/check --all` without root privileges, with private test
displays and a fresh target directory. The pinned native library/mock compositor
comes from the image. Missing dependencies still fail; no suite is skipped.

Cargo caches contain only registry/git downloads under `.ci-cargo`, keyed by
build target, OS/architecture, dependency image/toolchain identity and Cargo.lock.
They never contain `target` or DeckLock executables. Each distribution still
installs its current build stack and builds/tests/packages its own checkout.
VM caches select a single pristine image using `scripts/vm-cache-key`, whose key
includes the runner's pinned image SHA256. The VM runner verifies the hash after
restore. Overlays, prepared disks, SSH keys, logs and the candidate package stay
outside the cache. The candidate and its dependencies are still installed in the
running guest, including the ARM native-harness path.

The three KVM guests wait for their candidate through `needs:`. The emulated ARM
guest takes about as long to prepare and boot as its candidate takes to build, so
that job starts with the build. `scripts/ci-await-candidate` lists the run
attempt's jobs and accepts the artifact only after the job that builds it has
succeeded in this attempt; any other conclusion ends the wait at once and the
wait is bounded. It needs `actions: read`, which `release.yml` grants to the
checks it calls. The costs are a guest booted for nothing when the build fails,
and a dependence on the GitHub API during the wait: thirty consecutive failed
listings fail the job.

An emulated guest would also spend about eight minutes running the suites in
turn, so three ARM guests share them: `session`, `package+compositors` and
`greeter`. Each boots and installs the candidate itself, each suite runs in
exactly one guest, and the three results are required like any other job.
`test-vm.py` fails a guest whose log does not show the completion marks of its
own suites, or shows another's. The KVM guests still run every suite in one
guest, in the original order. The cost is three ARM runners for about as long
as one ran before, and an order of suites the ARM guests no longer share.

All ten prerequisite jobs remain required by **DeckLock checks**, including when
packaging finishes before isolated tests. Release publication still depends on
the whole quality workflow. Superseded PR runs are cancelled; main, tags and
merge-group runs retain their independent evidence. The tradeoffs are cache
storage/transfer, weekly cold preparation and speculative package/VM work if the
parallel isolated job fails. The ARM guest still uses TCG and remains the longest
gate; cached downloads do not remove its assertions or prove physical-device UAT.

`AGENTS.md` is guidance, not a security boundary. Required GitHub checks and branch
protection enforce the merge policy. A repository administrator can still change
these policies; changes to safeguards must be reviewed as such.

Cargo `0.3.0` / package revision `1` describe the `v0.3.0` release.
See `docs/releases.md` before preparing another one.

Procedural media add deterministic frame/validation tests, actual CLI
round trips and a private GTK contract for texture updates, bounded dimensions,
unmap/drop cleanup, library/pool selection, per-item eye/gear drafts, settings saves and rest preview. Rendering performance and
battery use on physical GPUs still require measurement.

### X11 backend boundaries

When a window covers the lock, the gate first waits for the lock back on top,
then requires it to hold the top in 10 consecutive samples within 3 s, and after
that in all 20 of the next samples. xfwm4 restacks its frame more than once for a
new window (one more about 70 ms after the lock is back), and each restack costs
the lock one reaction, slower on a loaded machine. Counting 19 of 20 samples from
the first recovery, as before, made that timing the test's outcome and failed
intermittently. The contract did not get weaker: the lock must still recover, and
once xfwm4 settles no sample may show another window on top.

Accelerated video on X11 is exercised against Xvfb's software OpenGL, which proves
the sink takes the GL path and keeps delivering frames, not that a GPU is faster:
performance on real hardware is still unmeasured.

The X11 gate runs against Xvfb, which has one fixed screen and no DPMS extension,
so three parts of the backend are exercised by nothing here: real monitor power
management, switching virtual terminals, and monitors added, removed or resized
while locked. The last one has its bookkeeping tested as a pure function in
`src/lock.rs`; the GDK signals that feed it are not. Only `xfwm4` stands in for a
window manager, and the gate types no password into PAM. These belong to manual
validation on a real Xorg session, on the list below.

## Manual validation still required

The VM exercises real PAM and Sway with controlled guest policies, the lock
boundary on labwc and Wayfire, plus greetd and Cage login on Arch, Fedora and
Ubuntu. Other PAM stacks, suspend/hibernate integration, compositors other than
Sway, labwc and Wayfire (Hyprland, niri, river, COSMIC), real GPUs and
multi-monitor setups,
aarch64 hardware, controller recovery/haptics, accessibility, real-device
ergonomics and visual quality need explicit UAT. For the X11 backend,
add a real Xorg session: authentication through PAM, monitor power-down and wake,
VT switching, several monitors and hotplug, and window managers other than xfwm4. Use a recoverable
test environment for session-lock/PAM work. Never use an active desktop or actual
power actions as an agent's automatic test target.

### PAM notice regressions

Fake helpers cover inherited stdout after exit, output larger than the pipe,
bounded notice retention, timeout and child reaping. The isolated GTK contract
covers delayed countdown callbacks and replacement by a new authentication.
Injected clock values cover elapsed time across suspension; this is not a real
suspend or PAM integration test. Policy/tally parser tests were removed together
with the unreliable local remaining-attempt inference, replaced by PAM-message
parsing and deadline regressions. No real credentials or account lockouts are used.

### X11 stacking recovery

Root stacking notifications use a separate X connection, watched by GLib at high priority on the GTK thread. GDK can pause its own X event source during frame processing; even the attempted 50-ms polling fallback still failed the existing raising-client assertion (10/20 samples). A default-priority watcher also failed (10/20); diagnostic timestamps showed event handling waiting behind 150-ms software-rendered frames. The high-priority event connection reacts before another ready frame without that paused queue, preserves GTK-owned popups and is closed with its source at release/failure/drop. Input grabs, authentication and the one-second watchdog are unchanged. The original sampling assertions remain intact.

### X11 VM display readiness

The VM Xvfb uses `-noreset` and waits for an actual client connection before GTK starts. A probe disconnecting as the final client otherwise resets the server generation; the Arch VM failed GTK initialization during that race before the X11 backend ran. `scripts/test-vm-x11-fixture.py` runs the exact fixture arguments on a private dynamically allocated display and checks a protocol marker survives closing the last client. Missing Xvfb/xdotool now fails the guest gate instead of skipping X11; PAM and input assertions are unchanged.

### Named ancestor ACL verification

The packaged guest asserts denial by attempting an actual `chdir` as the login
account and requiring `PermissionError`, while retaining the original ACL and
public mode bits. Ubuntu's packaged Rust `test -x` reports success from the mode
bits for this fixture even though the kernel denies traversal; it cannot serve
as the denial oracle. Library fallback and the other media assertions remain
required on every VM target.

The installer's own headless probe opens selected regular media and reads/creates
a private temporary entry in state storage after dropping to the configured
account. It removes that entry immediately; no PAM or display is used. A public
CLI regression covers aliases, missing/nonregular paths, nonblocking FIFO
rejection and absence of leftover entries. The VM additionally requires the
installer's probe to reject the ACL-denied directory.

The packaged setup command must also reject named ACL denial in a custom state
ancestor before replacing the active login command or appearance, preserving the
original ACL. This covers the installer boundary beyond the standalone probe.

### Login configuration activation

The real CLI contract requires setup to explain reboot/service restart instead
of promising that logout reloads greetd. Upstream greetd 0.10.3 reads config once
in [main](https://github.com/kennylevinsen/greetd/blob/0.10.3/greetd/src/main.rs)
and retains the configured greeter command in
[Context](https://github.com/kennylevinsen/greetd/blob/0.10.3/greetd/src/context.rs).
Packaged VM startup uses a fresh service; it does not establish configuration
hot reload. Setup still leaves services running, preserving the active session.

### Greeter pad validation

`scripts/check --all` includes a private Xvfb/D-Bus greeter-controller gate. Its
`scc-daemon` is an executable fake supplied through a temporary PATH; no physical
daemon or controller is used. `examples/preview_check.rs` also checks both pads
typing independently in the login keyboard, disconnect fallback and restored
ghost geometry. Setup CLI checks cover Cage/Hyprland option forwarding, dry-run,
repeat/preservation and explicit disable. The profile is embedded in the binary;
the locally installed sc-controller 0.7.1 parser can load it without starting a
daemon, but CI's fake does not establish compatibility with every sc-controller
version. Packaged VM jobs do not currently install sc-controller or exercise USB
and uinput. [The device UAT procedure](greeter-controller-uat.md) covers those
boundaries, authenticated-session handoff and recovery. Normal exit removes its
private files; abrupt death can leave a small runtime directory until runtime
cleanup, with no password data. The parent-death signal follows
[Linux PR_SET_PDEATHSIG](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html).
