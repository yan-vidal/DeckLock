# State

2026-09-06: Rust/GTK4 replaces Python on `main`, explicitly authorized by the user.
Python reference remains in Git history at `7459bb1`; public origin is yan-vidal/DeckLock.
Wayland only; external CSS + declarative TOML layout; Fluent i18n now,
plugins/Lua later. No measured performance claim.

Environment: GTK 4.22.4, GStreamer 1.28.6, Rust 1.96.0, Hyprland.
gtk4-layer-shell 1.3 built locally in ignored `.deps/`; no system installation.
Use `scripts/bootstrap-native --tests` to reproduce the native setup.
Installed sc-controller: 0.7.1; Rust integration uses its external Unix daemon.

Validation passed: 28 unit/integration tests, fmt, clippy with warnings denied,
build, configuration check, GTK preview harness, and isolated compositor test.
The latter covers acquisition, monitor add/remove/re-add, SIGTERM without unlock,
and refusal of a second locker after the first dies. Preview was visually
inspected in Portuguese/English with keyboard and a looping test video.
No real-session lock, real PAM password, physical controller or power action was
used for validation. No automatic replacement of the existing locker.

Follow-up: restored Python composition and SVG-derived key geometry, GDK startup keymap and BR fallback
AltGr, proximity ghost rendering and controller bindings. `--controller` resolves
the daemon socket and registers the PID used by the existing Python shortcut.
The original launcher was tested against a fake daemon without spawning another
OSK. `scripts/import-python-theme` exports legacy colors/media to an external theme.

Next: device UAT, controller recovery/ergonomics, haptics,
dynamic XKB groups/custom keyboard layouts, and standalone OSK parity. Full details and
reproduction commands: `docs/rust-migration.md`, `README.md`.

Follow-up verified: double Shift latch (600 ms) and virtual Caps indicator;
touch-gated pad coordinates prevent final neutral reports reviving ghost keys.
GUI regression covers independent release of both pads and no stale selection.

Alt fallback corrected for two-level system maps. Native --toggle-keyboard
launcher replaces the Python dispatcher in tests; old installed shortcuts remain
compatible. Python runtime modules removed from the current tree; development
utilities may still use Python. No desktop shortcut installation was changed.

2026-09-07: native GTK4 --settings editor added. Config.layout overrides theme
layout; validated atomic config save preserves PAM/idle background/socket fields.
Unsaved previews use a temporary config and an explicit --preview subprocess;
closing settings terminates only that child. File selection uses GTK4 FileDialog.
30 core tests plus settings save/reload/invalid theme/cleanup GUI checks passed.
English primary README, Portuguese README.pt-BR.md and real English screenshots
introduce the application as a general Wayland lock screen.

2026-09-07: default media directories now preserve the legacy XDG_CONFIG_HOME/midias
layout (bloqueio and ocioso, each fotos/videos), created on startup. Explicit config
and theme backgrounds retain priority; empty idle folders fall back to normal.
The editor exposes idle background; Hibernate delegates to systemctl hibernate,
with no power callbacks in preview. 32 tests, clippy/fmt and GTK preview/settings
checks passed. English README GIF uses supplied Osaka footage with real mouse
clicks, cursor, Caps Lock, password visibility and power tooltips. Capture helpers
are development-only; no real power action, PAM or desktop lock was exercised.

2026-09-07: media library/pool cards added for normal and idle backgrounds.
Config keeps optional explicit pools (including intentionally empty pools), separate
photo intervals, idle_enabled and idle_reuse_background; legacy paths still load.
Library imports run off the GTK thread and use create_new to avoid overwrites; pool
removal never deletes media. Default packs live outside the executable, with the
empty author-supplied pack scaffold in assets/media. Imports use XDG_DATA_HOME/
decklock/library; bundled packs use decklock/media. See assets/media/README.md.
Selection chooses video-only looping or image-only slideshow per lock; GTK Stack
crossfades photos. Reuse-idle preserves the current renderer and hides controls.
35 core tests, clippy/fmt, settings/keyboard/media GUI checks and isolated protocol
checks passed. Real editor screenshots are settings-library.png and settings-idle.png.
Original default artwork and its credits/license are awaiting the user's files.

2026-09-07: settings redesigned with adaptive-height Background/Rest tabs,
compact rounded media cards, pool info tooltips and collapsible layout preferences.
Seven built-in palette adaptations (Classic, Catppuccin Mocha/Latte, Dracula, Nord,
Tokyo Night, Gruvbox) share semantic colors across settings and lock/keyboard.
Config.theme_preset selects bundled colors; external Config.theme retains precedence.
Live settings CSS replacement preserves the last valid provider on errors and
removes the provider on window destruction. Empty external selection cannot save.
36 core tests, clippy/fmt and GTK tests passed; GUI validation cycles all presets,
checks color changes and persistence, tabs/tooltips and invalid external folders.
Dark/light settings screenshots inspected; themes/README.md records palette sources.

2026-09-07: dropdown listview styling fixed contrast across dark and light presets.
Disabling idle mode now dynamically hides dependent duration and background reuse options.
Added transient, reusable media viewer modal for inspecting images and muted videos from the library.
Added integrated theme editor with draft isolation, syntax validation, and persistent copies for style.css
and theme.toml. Settings preview now updates live while preserving window identity. Full-screen
showcase on workspace 3 recorded to docs/assets/settings-demo.gif. 36 unit tests, clippy and
live integration checks passed.

2026-09-07 review and capture follow-up: settings-showcase now waits for pointer
acknowledgement before each GTK action; duplicate synthetic clicks removed.
record-settings.py captures only workspace 3's Full HD monitor at native scale,
restores workspace/cursor, and preserves source frames plus timing under the cache.
Desktop bar/watermark are masked in encoded demos. Master and focused GIFs are 1080p.
Palette-only live preview edits preserve the existing video paintable, avoiding
restarts. Light-palette clock/date remain white over arbitrary footage; controls
use semantic surfaces. A subtle woven CSS texture styles the settings background.
Language selector moved to the top and translates current settings without saving
or discarding drafts. Background/Rest tabs pin the settings preview state; the
normal lock still uses real inactivity. Layout.idle_clock_visible defaults true,
independent of clock_visible and idle_reuse_background, and is editable in TOML.
The previous subprocess note describes the initial implementation only: current
settings previews share a process but do not construct a session lock/PAM/controller.
GUI regressions include language, draft preservation, idle clock overrides and
paintable retention. Recording actions are instrumented GTK operations after real
pointer movement, not an assertion that each operation used a physical click.

2026-09-07: headless configuration CLI, release packaging, focused demos, and default media pack.
Added `decklock config` subcommand suite (`show`, `path`, `get`, `set`, `unset`, `import`)
with atomic TOML saves and full validation matching the GTK settings window.
Running without arguments or with `--help` prints the command synopsis with concrete examples.
Large monolithic settings-demo.gif split into four focused 1080p clips (`settings-themes`,
`settings-library`, `settings-rest`, `settings-editor`) mapped to README sections.
Bundled default media pack includes author-supplied `osaka_dotombori.mp4` under `assets/media/videos`,
accompanied by `assets/media/CREDITS.md` with author attribution; no separate license for other uses was specified.
Added `packaging/` directory containing desktop entry, Arch PKGBUILD template, and
`scripts/package-release.py` staging release archives and Arch recipes for version 0.1.
README.md and README.pt-BR.md restructured to prioritize installation and usage,
moving build instructions to the development section. 38 core unit tests and clippy pass.


2026-09-08 release audit: main and v0.1 matched 7fc9b01; all four public assets
matched local hashes and included the original Osaka video. Found an uncovered
Clap conflict rejecting --config PATH with the config subcommand. Removed the
blanket conflict, explicitly reject GUI/lock flags with config, and added parser
regression coverage for --config before/after nested commands. Revision 2 keeps
version 0.1 and preserves the original v0.1 release/tag under a new v0.1-r2 tag.
Corrected the inaccurate CC BY-SA claim above; media credits never granted it.

2026-09-08 regression guardrails (in progress): introduced scripts/check with
isolated HOME/XDG state, bounded subprocesses and retained logs; real executable
CLI tests, archive payload/CLI contract and exhaustive seven-event session traces.
Controller test reads framed lines and acknowledges commands before expecting the
next command. Core gate passed locally (38 library + 1 parser + 6 integration tests).
GitHub workflows add Ubuntu/Rust-1.93 native/GTK/mock checks, an unprivileged Arch
candidate build, and tag-only publishing after checks plus metadata/main ancestry
verification. AGENTS.md and docs/testing.md define invariants and honest UAT limits.
Remote CI and required branch protection still pending validation at this checkpoint.


2026-09-08 verified guardrails handoff: c46d51a passed Checks on main (run
34236755257), including the Arch package. Active mainProtect ruleset requires
PRs and both Actions checks with no bypass; tagProtect prevents version tag
updates/deletion. Earlier pending status above is superseded.

0.2 implementation in progress: user authorized the procedural effects now.
Animation settings are external config.toml tables animation/idle_animation,
not executable theme plugins. Starfield, particles and Lissajous use bounded
transparent raster textures scaled over existing media. Default off; separate
rest settings follow idle reuse. CLI and translated GUI share validation.
See src/animation.rs and examples/animation_check.rs for pure rendering and
mapped/unmapped lifecycle contracts. All eight local gates passed, including animated GTK lifecycle/settings and
Wayland protocol. Preview captures inspected in target/check-logs. Remote CI
and release publication remain pending.


User correction: do not open a release PR or finalize 0.2 yet; more features are
still to be specified. PR #3 was opened prematurely and was closed without
merge. Restore Cargo 0.1.0/package revision 2 and current release installation
links; procedural code stays on feat/procedural-backgrounds-0.2. No new tag or
release was created. Await the remaining scope before reopening a PR for 0.2.


Latest user scope: procedural effects are media library items, not overlays.
They must be added to either pool, have eye/gear actions per item, and replace
images/videos. A selected procedural stays for the session like a video. Shared
parameters live in procedurals.<id>; reserved procedural:<id> media IDs remain
unresolved by filesystem path expansion. Unreleased overlay drafts migrate on
load/save. Draft apply does not write settings until main Save.
Design PR #4 (including Opus's archive-root variable fix/test) was merged alone
as 239928a after both checks passed. No release/tag/version bump. This branch is
for a draft PR only; wait for the rest of 0.2 before merging/releasing.
All eight local gates passed for the media-item implementation, including
reserved-ID rejection, overlay-draft migration, opaque/repeatable frames, stable
pool selection, eye/gear drafts, persistence and previous-media unmapping after
transition. Captures of library, item editor and rest inspected. Main design
post-merge run 34282111819 also passed. Procedural draft CI still to be run.


2026-09-08 preview diagnostics and two new effects: the tick callback is Fn, so
the metrics accumulator is a RefCell like the origin/last cells beside it; the
earlier `mut` capture did not compile. Matrix rain and Doom fire ship as library
items with their own colour presets and bitmap glyphs, no fonts or new
dependencies. Default speed is 1.0 in code, example config and both READMEs.
preview_stats reports drawing CPU (CLOCK_THREAD_CPUTIME_ID around the render)
apart from whole-process CPU and VmRSS, which include GTK and belong to the
settings process, not to the locker; the panel says so and must keep saying so.
Six new catalogue keys added to both locales. animation_check now asserts the
library thumbnail, the viewer gear and that the stats panel reports FPS, memory
and CPU, so isolated-gtk covers them. All eight local gates passed. Still a
draft PR: no merge, no tag, no release.


2026-09-08 frame limiter, denser rain, contained fire, aurora and video stills.
The limiter compared elapsed time against the exact period, so a 60 Hz clock
missing it by a microsecond rendered every third tick: 20 FPS for a 30 FPS
limit. It now renders on the tick nearest a deadline that advances by whole
periods, which also holds rates the clock cannot divide evenly (24 on 60 Hz);
due() is pure and tested across five clock rates and four targets. The film's
typeface is custom and unreleased and fan recreations are personal-use only, so
the glyph set grew to 32 original 5x7 bitmaps with mirrored kana-like strokes;
no font discovery, so frames stay repeatable. Density now means columns for
matrix, flame height for doom-fire (cooling raised to average 2 per row, so 120
keeps the top dark), bands for aurora and particle count elsewhere; it is
documented per effect. Aurora fills with summed-sine bands, outline cost only.
Video library rows show a decoded frame via playbin preroll and convert-sample,
bounded by a deadline and falling back to the icon. Item names are built as
animation-<id>, so matrix and doom-fire were rendering their raw ids; a test now
asserts every ITEMS entry is named in both locales, routed to an effect and
stored apart. All eight local gates passed. Still a draft PR.


2026-09-08 power visibility, restore buttons and two more procedurals. The
upper-right group is suspend/hibernate/restart/shutdown, so power_visible joins
clock_visible, idle_clock_visible and avatar_visible in [layout], defaulting to
visible. The theme editor keeps its existing save path: the settings Save is
still what writes to disk, closing still leaves the draft pending, and a note
beside the new Restore button says so, because a window that previews as you
type does not make that obvious. Restore reloads the built-in theme into both
buffers; the layout group has its own Restore that resets only spacing, padding,
alignment, keyboard scale and the visibility switches. Neither writes on its
own. Flow field integrates streamlines through seeded value noise (the shape
behind Fidenza) and ridgeline stacks occluding ridges from the same noise; both
are polylines, so cost stays with the outline, and both stay stateless, which is
what the repeatability tests require and what rules out flocking. All eight
local gates passed. Still a draft PR.

2026-09-09 review follow-up: video thumbnails now decode serially off the GTK
thread. Catalogs share pending requests and a cache of 128 completed results
(including failures), keyed by path, size and modification time. Only owned
pixels cross back to GTK; queued requests and weak widget references do not keep
closed settings windows alive. The viewer uses the settings' shared I18n catalog,
including subsequent language changes and per-item live refreshes. Every built-in
procedural now defaults to speed 1.0; explicit saved speeds remain unchanged.
Regressions first reproduced the synchronous thumbnail, Portuguese metrics shown
in English and Aurora's 0.6 default. The GTK contract now checks deferred delivery,
shared textures across both libraries and a rebuilt pool, plus metrics language
changes. The real CLI checks every default speed. No version bump or release.
Validation: scripts/check --all passed all eight local gates for this follow-up,
including the new CLI/GTK regressions and existing Wayland protocol contracts.

2026-09-09 PAM notice review follow-up (base d55ebf6): helper output is now
drained nonblocking with bounded memory/work and the existing auth deadline.
Regressions reproduced inherited stdout delaying a completed denial and oversized
output filling the pipe. Both now pass. Removed the separate faillock command and
policy/tally inference: module overrides and failure windows made remaining-attempt
predictions unreliable, and the command could delay delivery of an auth result.
Only PAM-provided notices remain, with existing CSS hooks and translated recognized
lockouts. Countdown uses a CLOCK_BOOTTIME deadline (including suspend), labels the
time as estimated, and stops on the next authentication. A stalled GTK-loop
regression failed before the fix and passes now; injected times cover missed ticks.
Removed parser tests belong to the removed inference feature, not weakened gates.
Both guide languages and testing documentation describe the new boundary.
Validation: all eight scripts/check --all gates passed on this working tree.
Initial sandbox run denied fake controller sockets; the permitted isolated run
passed. Real PAM, physical suspend, device recovery and hardware remain untested.
Keep PR #5 draft; no merge, tag or release.

2026-09-09 scope clarification: the user deferred a dedicated PAM service file.
Keep the existing system PAM integration and configurable pam_service, default
login; do not install /etc/pam.d/decklock or change system policies for 0.2.0.
This supersedes the conversational proposal to switch that default. Future
widgets start with built-in Rust implementations, with new types supplied by
plugins; Lua remains the proposed extension language pending isolation design.
Widgets/plugins, remote media and a greetd greeter remain outside 0.2.0.
See docs/ROADMAP.md. Documentation only; validated with git diff --check.

2026-09-09 real PAM/Wayland VM gate: scripts/test-vm.py boots a pinned Arch cloud
image with KVM, 2 CPUs and 2048 MiB. It installs the CI candidate, runs real Sway
headless and unprivileged DeckLock, types through wtype, reads status through
AT-SPI, and checks independent underlying-client input and protocol traces.
Local final run passed nine assertions: input before lock, real PAM denial,
successful unlock once, input restored, real faillock lockout/visible estimate,
countdown updates, PAM expiry before the estimated UI deadline, account-policy
denial and SIGTERM remaining locked (some assertions combine related checks).
Candidate reinstall/config preservation also passed. Candidate came from run
34408286449 for head 80c6dd8; packaged BUILD-INFO source is GitHub's PR merge
a65f3b96f504c629a2f6dd37727ce1794ee8cbe9. SHA256:
c16b385a69738f1770e1bcb144b7b96abf6b053a4aede5c25ad079cf6a311fdb.
Evidence: target/vm-logs/final and target/check-logs (all eight existing gates
passed). No Rust product changes or authentication shortcuts were required.
The new CI job runs after packaging, and the release workflow inherits it.
Provisioning gotchas: wtype needs time to initialize its virtual keyboard; exact
probe key assertions caught a dropped first event. runuser skips initial login,
so the fixture initializes a user-owned tally with root faillock --reset. A
guest-only short lockout policy and account-deny service are test fixtures, not
new application PAM configuration. The host PAM, desktop, controller and Android
emulator are untouched. VM cleanup removes its disk/key on success and failure.
Final run minimum host available RAM: 1728 MiB; swap 6450 -> 7501 MiB while other
work and local checks also ran; peak full-memory PSI avg10 2.52%. The VM was
stopped after the test. Future runs abort on sustained low available memory or
memory pressure. GPU/controllers/physical power actions and other PAM stacks
remain separate validation boundaries. No merge, tag or release in this task.

2026-09-19 Phase C completed and Phase D out-of-scope decision: Phase C issues #15-#18
are closed. X11 lock backend merged behind optional --features x11 (PR #30), accelerated
video via gst-plugin-gtk4 GLX/EGL merged (PR #32), and per-distribution lockout documentation
plus X11 guarantee limits merged (PR #33). Phase D (other Unix systems: FreeBSD, OpenBSD,
NetBSD, #19) closed as not planned by user decision to keep focus on Linux desktop sessions.
Next: address testing gaps (real PAM on X11 in VM; additional Wayland compositors in VM) and
packaging for x86_64 and ARM architectures.

2026-09-24 greetd greeter delivered (#36; PRs #38-#44): `decklock --greeter` runs
under greetd and Cage with an avatar carousel, a session selector and saved
selection; `decklock setup greeter` writes the plain `cage -s -- decklock
--greeter` command, because greetd runs it as `sh -c "exec <command>"`. Arrow keys
select accounts from an empty password field (capture-phase controller; X11 gate
`examples/greeter_keys_check`). Each distribution VM exercises real greetd/PAM
login, with greetd started as a transient system service so logind registers the
greeter and user sessions.
Reach follow-up: each VM also repeats the lock boundary on labwc
(`scripts/vm/compositor.py`); Hyprland, niri, river, Wayfire and COSMIC stay
protocol-only (GPU render node, no headless mode, or not packaged on Ubuntu
26.04). CI builds and tests aarch64 Fedora and Ubuntu candidate packages on
`ubuntu-24.04-arm`; they are not published and no aarch64 VM gate exists.
Still open: Debian (#25), GPU compositors in VM, aarch64 VM, device UAT.

2026-09-24 reach items (PR #46): Wayfire joins labwc in every VM, rendering
with software GLES on vgem; Hyprland stays protocol-only (Aquamarine needs a
real GPU allocator). The Ubuntu arm64 package passes the whole VM suite in a
TCG-emulated guest with waits scaled by 8 (about 100 minutes), including a
faillock window scaled the same way and an X11 step that waits for the
locker's keyboard grab. `debian-watch.yml` checks Debian weekly for
gtk4-layer-shell >= 1.2 (#25 still blocked at 1.0.4). The X11 gate's stacking
check now requires the lock on top in every sample once xfwm4 settles, instead
of 19 of 20 counted from the first recovery. Still open: Hyprland in VM,
Fedora arm64 VM, publishing arm64 packages, real ARM hardware, device UAT.

0.3.2 release PR: version bump; `release.yml` also publishes the Ubuntu arm64
package, which has its own (emulated) VM gate in the checks the release reruns.
Fedora arm64 stays unpublished until it has a VM target: its Cloud image could
not be pinned from the agent's environment. The user approved publishing on 2026-09-24;
the changelog carries that date and `v0.3.2` is tagged after merge.

2026-09-30 installation hardening (PRs #48 and #51): existing lock commands are
classified without overwriting custom process guards or misreading shell parameter
braces. Greeter startup no longer creates optional media folders; system setup
prepares greeter-owned config/cache/data/state before replacing greetd's command.
The login window requests fullscreen without decorations. Hyprland greeter setup
can generate verified Lua with matching panel/touch transforms, preserves an
existing Hyprland command on repeat, and never restarts the running greetd.
The packaged VM fixture now uses HOME=/, checks actual directory ownership and
starts the exact generated command. The required DeckLock checks aggregator waits
for every existing isolated, package and VM gate. Local/CI evidence and hardware
limits are recorded in docs/testing.md and the PR descriptions; do not infer a
new package release or Steam Deck GPU/touch UAT from these source changes.

Greeter compatibility corrections: fullscreen waits for the initial surface frame
to avoid older Cage/wlroots initialization assertions. The isolated GTK gate uses
a private window manager to assert its acknowledgement. On SELinux systems setup
uses /var/lib/greetd/decklock, restores and verifies the existing xdm_var_lib_t
context before changing greetd, without disabling enforcement. Fedora VM checks
the storage context and remembers both fixture accounts. Device UAT remains separate.

Setup preserves the packaged greetd default_session.user and prepares storage for
that exact account. Fedora uses greetd with a private /var/lib/greetd parent;
forcing a separate greeter account prevented traversal despite correct leaf
ownership and SELinux labels. The VM probes writing as the selected account before
login, while retaining its existing two-account PAM/state assertions.

Before saving system greetd setup, a bounded setpriv (util-linux) probe checks
read/write/traversal as that account without PAM. The VM asserts a root-only
ancestor is rejected while preserving the working command.

2026-09-30 merge follow-up: PR #48 passed every required CI job (including the
Ubuntu ARM VM) and was squash-merged as ba682dd, as required by mainProtect's
linear-history rule. PR #51 was reconciled without force-push; its tree remained
identical to 221c0ea. A fresh local full gate then exposed intermittent X11
stacking recovery (9/20 samples, below the unchanged 12/20 contract). A new
real-CLI/private-Xvfb regression models slow GTK event processing with a
main-thread-only preload and confirms the delay ran. The original backend failed
that regression at 0/20. The observer now owns a separate restacking worker and
X connection; GDK's server-provided resource range preserves owned popups,
input/focus/authentication stay on GTK, and the worker stops before unlock.
The targeted full X11 gate and X11 clippy passed; the new commit still needs the
complete local suite and required package/VM CI. Native Xlib/XCB build/runtime
dependencies are explicit. No release, installation or active session changed.


2026-10-01 shared-media follow-up: the user rejected duplicate media snapshots
and chose read access for every local account, with write access retained by
the desktop owner. Setup shares a dedicated canonical library outside personal
HOME, prefers its common volume when separate from /var/lib, and saves
media_library for CLI/GUI parity. Owned sources and old generated snapshot media
become links; same-filesystem migration keeps the inode, cross-filesystem
transfer is verified/fsynced before replacement, and original config receives a
backup. Small private appearance/theme/state generations remain. The shared
folder uses 0755/0644 with selected-file ACL normalization; HOME permissions
remain unchanged. SELinux uses a persistent xdm_var_lib_t mapping rather than
disabling policy. Source/isolated/packaged checks must still be reported
separately; this change does not add login backends beyond greetd, make encrypted
HOME mounts available before login, establish device UAT or publish a release.

The appearance pointer is published only after legacy conversion and label
verification succeed; rejected unrelated hard links preserve the valid installed
appearance and login command, with a real-CLI regression.

Fedora Cloud CI exposed an ineligible common HOME parent: automatic selection
must check existing ancestor ownership/access before preferring that volume.
It now falls back to /var/lib rather than changing HOME/parent permissions.
The public packaged CLI fixture also makes a common volume ancestor writable
and asserts system-library fallback, unchanged parent modes and readable aliases;
VM evidence records root/HOME/system ancestor mode, UID, GID and device.

Further Fedora evidence identified filesystem root / as 0775 root:root, not an
inaccessible HOME directory. Ancestor validation preserves existing group-0
administrator access while rejecting ordinary group/other/named ACL writes.
Named ancestor ACL traversal denial also causes automatic system-library
fallback; a real CLI/container regression failed before this correction.
The guest now asserts ordinary-group writes and named ACL denial/writes are
rejected for automatic HOME-volume selection without editing those ACLs.

2026-10-01 activation guidance: greetd 0.10.3 caches the login command at daemon
startup and does not reload it on logout. Setup output and the bilingual guides
now require a reboot, or service restart from a separate TTY after leaving the
managed session. A public CLI regression failed on the old logout promise. No
automatic service restart, session termination, authentication or power action
was added; existing packaged VM tests start a fresh service.

2026-10-03 aarch64 VM gate sped up (PR #56): the emulated Ubuntu arm64 gate took 93.7 min
(run 36897087368), of which about 70 were package installation under TCG, not the suites
(13 min) as an earlier estimate had it. It now takes 35.2 min (run 37117073212) with exactly the
same 41 assertions as the baseline. `--provision-native` installs the harness packages (286,
48 s) on the host, in a same-architecture chroot of a raw copy of the pinned image, before the
guest boots; the candidate and its 98 dependencies, the reinstall check, the PAM fixtures and
every suite still run in the booted guest, and setup.sh is not edited. Four things the plan did
not foresee came from the CI probes, each fixed test-first: exercise.py raced Sway's start-up
against swaymsg's 3 s timeout (margin 0.6 s, lost by 0.06 s) and now waits for Sway's own report;
an offline install enables services that start at the guest's first boot (greetd ran and
`usermod` on its account failed), so DISABLE_UNITS disables them after the install; needrestart,
an apt hook of the Ubuntu image, scanned every process for about 14 min after each in-guest
install (28 min), so emulated guests set NEEDRESTART_SUSPEND=1; and apt cost about 7 min per
call under emulation even with nothing to do, so the guest is given the manifest with
PRE_SYNC='' and SYNC_AND_INSTALL='true' appended. Tried and reversed: pre-installing the
candidate's 98 dependencies on the host too (31.0 min), which gave up the evidence that apt
resolves them and that their maintainer scripts run on a live system for four minutes; the user
chose to keep them in the guest and a contract keeps it that way. Containment: all mounting
under `unshare --mount --propagation private` (a bind of /dev under systemd's shared mounts
would reach the host's /dev/pts), the mount point outside the work directory and removed only
with rmdir, the privileged step bounded by `timeout` inside sudo (a subprocess timeout kills
only sudo), the guest's own files restored by an EXIT trap installed first. Any failure of the
host step warns, records `fallback_reason` and takes the unchanged in-guest path: shown end to
end on a throwaway PR (#57, closed): the slow path passed with the same 41 assertions in
59.9 min. The aarch64 job timeout is 210 min (the sum of the script's own bounds on the native
path) and the guest phase is bounded at 600 s x slowdown there; the timeout only caps a hang.
Not done: dpkg force-unsafe-io and cache=unsafe (they would have to win back minutes out of the
4 that installing the candidate and its dependencies takes), masking background services
(target met; masking snapd.seeded risks a stalled first boot). Deferred minors from the final
review: the job-timeout comment omits the image download and the non-VM steps and the fallback
path's bound sum (286 min) exceeds 210; ROOT_LABEL is Ubuntu-only so the flag on Arch or Fedora
attaches a loop device and always falls back; a fallback reason can embed the whole chroot
script; the collected dpkg.log mixes host-time and guest-time entries; the wiring contract pins
source substrings. Untested: real ARM hardware, and whether the hosted runner's sudoers
(use_pty) hangs up a SIGKILLed sudo tree (the in-sudo timeout makes that moot). Next: the
controller reconnection fix, then the 0.3.3 release.

2026-10-03 0.3.3 release PR: version bump with the controller daemon reconnection (the client
retries transport loss every 250 ms and recaptures the pads only while the keyboard is still
active; protocol refusals and an overflowing event queue stay final; connection events reach
every lock surface; a reconnect restores the pad keyboard geometry), the greetd restart guidance
(#55) and the lock/login installation fixes merged since 0.3.2 (#48-#54). The user approved
generating 0.3.3 on 2026-10-02 and asked to hold it until the faster aarch64 VM gate (PR #56)
landed; the changelog carries the release date and `v0.3.3` is tagged after merge. The
reconnection tests failed first against the previous implementation ('client did not
reconnect', 'reconnected keyboard must restore pad geometry'). The GTK test's first expected
value was wrong and was corrected from measurements: the selected key is the one under the pad's
point (centred pads land on d and k; f and k only light up by proximity), and a left trigger left
held by an earlier check had to be released explicitly. Untested: a real sc-controller restart
with a physical controller; the Steam Deck device checks and hardware stay outside what the
suite establishes.

2026-10-09 greeter pad input: resumed on feat/greeter-pad-input from main 26e1bef.
Root's pre-0.3.3 workflow/docs draft was superseded; the old forced UID/runtime
environment workaround was not ported because current greetd/logind service
tests already establish that boundary. Exact staged/unstaged patches and the
untracked prototype are retained under target/reconciliation-2026-10-09 in the
worktree. Root tracked edits are in stash 40cb86326547b03c9763d60199a1cfb6bd335648;
root now follows main. Original notebook/prototype/dependency files are preserved.

setup greeter/all --controller now opt into --greeter-controller; omission
preserves the saved choice and --controller=false disables it. Real greetd only
owns a foreground/alone sc-controller child and a static embedded profile in
private runtime storage, with normal bounded termination/reaping and Linux
parent-death protection. Preview/settings never start it. Missing/failed daemon
startup keeps the existing clickable keyboard. No device rules, groups, host
service configuration, passwords, PAM actions or session-lock behavior changed.
The installed 0.7.1 profile parser was checked without running a daemon; the
old prototype's Profile() constructor lacked its required ActionParser.

Regression first failed at the actual setup CLI rejecting --controller and at
the real binary rejecting --greeter-controller. An additional real Unix protocol
regression reproduced Ready without hardware incorrectly reporting Connected;
it now keeps fallback and restores Connected on device hotplug. GTK checks type
with both pads in login mode and verify disconnect fallback. An initial new GTK
fixture sent pad events before the newly shown keyboard was allocated; it now
waits with a bound and uses a fixed keyboard layout, preserving the assertions.
scripts/check --all adds a real binary/fake daemon lifecycle gate (private Xvfb
and D-Bus). Final-commit local/CI evidence is to be recorded after verification;
physical Steam Deck and sc-controller-in-package VM validation remain pending.
No installation, host daemon restart, release or tag performed. Device procedure:
docs/greeter-controller-uat.md; investigation pointers: .notebook/greeter-pad-input.md.

2026-10-09 device setup follow-up: PR #59 head 424849c passed all 11 CI checks
(run 37981085723). The user installed its Arch candidate on the Steam Deck OLED;
the installed binary hash matches the candidate and the packaged Arch VM evidence.
Dry-run succeeded, but applying setup exposed a separate installer boundary:
Hyprland --verify-config requires XDG_RUNTIME_DIR, which sudo removes. The saved
greetd configuration remained unchanged and the service was not restarted.

A new real-CLI regression failed first with the missing runtime. Verification now
creates owner-only temporary HOME/XDG storage and removes inherited desktop,
D-Bus, Hyprland and greetd sockets for that child; no runtime environment is
forced into the actual greetd session. The regression also checks cleanup, an
inherited runtime left untouched, and saved greetd/Lua preservation on rejection.
The installed Hyprland 0.56.2 reproduced the missing-runtime abort and accepted
the generated Lua with private runtime storage; invalid Lua was still rejected.
This is isolated parser evidence, not root package installation or real login.
The corrected commit needs its own full local/CI checks and a new candidate
before accompanied service activation and physical controller/PAM validation.

2026-10-09 CI preparation: user authorized dependency images, download caches and
concurrent package builds after reviewing their costs. Separate worktree/branch
perf/ci-dependency-cache keeps PR #59's accompanied Deck UAT candidate intact.
Spec: .specs/features/ci-speed/spec.md. Baseline run38000779070 spent745 seconds
installing native dependencies and271 seconds in its isolated gate; overall56m35s,
including35m00s in the ARM VM. Its eleven required jobs all passed at source
21342b4 (the controller/greeter candidate, not this CI change).
New regressions first rejected serial package scheduling and absent cache actions/
metadata CLI. Eight cache contracts now pass, and the real final aggregate step
rejects failure/cancelled/skipped across all ten prerequisites (40 result cases).
Dependency image built locally from its pinned bases. Initial image preparation
failed because the official slim Rust image omits rustfmt/clippy; pinned components
are now added explicitly. The first container invocation then failed at an unset
GID from an apostrophe in a shell parameter error message. Its public entrypoint
regression reproduced that error before correction and additionally requires
private HOME/CARGO_HOME after runuser, argument forwarding and root-UID refusal.
A concurrency regression also rejected main/tag grouping by identical source SHA;
non-PR groups now use run_id. All failure logs retained in target/ci-development.
Full local gate on 9bb22da and separate PR validation in progress; no measured
speed claim or merge/release yet. Native bases/actions remain digest/SHA pinned,
weekly preparation refreshes apt/native layers, Cargo caches downloaded sources
only, and each VM cache is one pristine hash-pinned image. Candidate install and
all assertions remain in the running guest.

2026-10-09 ARM VM emulation experiment (branch perf/arm-vm-emulation, on top of PR #60's
commits; spec .specs/features/arm-vm-performance/spec.md): the aarch64 VM is the only long gate
left. Run 38006310277 (6b444a2, PR #60) took 40m20s; every other job had ended within 10m11s
and the ARM VM job took 36m00s. Its report had no phase times, so they were reconstructed from
that run's job log, serial log, journal, dpkg.log and apt history: host preparation 1.4 min;
boot to guest-ready 9.6 min (apparmor.service 157 s, the four cloud-init stages 188 s, about
30 s per SSH connection); the candidate with its 98 dependencies 7.1 min (dpkg 271 s, apt
resolution 69 s, 83 s after dpkg ended); the reinstall 2.2 min (dpkg 9 s); the suites 13.6 min
(exercise.py 5m03s including the 96 s scaled lockout, labwc 1m24s, Wayfire 1m22s, the greeter
about 6 min). Hypothesis: QEMU 8.2.2, the runner's, signs pointers with QARMA5 under
`-cpu max`, which its documentation calls slow to emulate; QEMU 10.0 made the
implementation-defined algorithm the default for that reason. `--tcg-pauth impdef` selects
`max,pauth=on,pauth-impdef=on,pauth-qarma3=off`; the default profile, KVM and x86 are
unchanged. The ARM job is temporarily a two-profile matrix: each profile runs the whole suite
on the same candidate in its own guest and both are required. result.json now records
cpu_model, qemu_version, host phase durations and guest-clock phase marks. `--print-plan`
prints the pinned image and CPU options without touching the host, and
scripts/test-vm-emulation.py (seven public-CLI cases) is a new `vm-emulation` gate step.
Checked before CI: QEMU 8.2.2 (1:8.2.2+ds-0ubuntu1.18, installed into a throwaway container of
the Ubuntu 24.04 dependency image) initializes `virt` with that CPU string without booting,
refuses impdef together with qarma3 and refuses an unknown property; actionlint 1.7.12 reports only SC2046 on the three x86 `setfacl` lines
main already has, and reports a wrong `needs` or matrix key in the ARM job when one is
introduced. No timing is claimed: the comparison is one CI run with one sample per profile.
The draft PR targets main because checks.yml runs only for pull requests to main, so its diff
carries PR #60's commits until that merges; it is not to be merged as is, since the matrix
doubles ARM runner time. Not adopted: a faster default, shorter waits, sharding. Limits: the
impdef profile is no evidence of pointer-authentication strength or of real ARM hardware.

2026-10-09 ARM VM emulation result and adoption: run 38012670359 (PR #61 at c7fdf8a, CI merge
commit e2fcde4, QEMU 8.2.2) ran both profiles on one candidate (package SHA256
3a2ed010...6a8295) and each passed the same 41 assertions in the same order. The job took
34m05s with QARMA5 and 17m45s with the implementation-defined algorithm; test-vm.py took
2001 s against 1014 s. By phase: boot to guest-ready 535 s against 192 s; the candidate and
its dependencies 390 against 160; the reinstall 134 against 41; Sway, PAM and X11 292 against
208; labwc and Wayfire 163 against 85; the greeter 323 against 193. Host provisioning, which
is not emulated, took 51 s against 87 s: runner variance, and against the faster profile. One
sample per profile on separate hosted runners. The user approved adoption: an emulated aarch64
guest now always gets `max,pauth=on,pauth-impdef=on,pauth-qarma3=off`; the `--tcg-pauth`
selector and the two-profile matrix are gone, and the job and its evidence artifact have their
original names again, so checks.yml differs from main only in the timing comment. KVM (`host`)
and emulated x86 (`max`) are unchanged. The new contracts failed first against the comparison
(13 CLI failures and the workflow's job name) and pass now; scripts/test-vm-emulation.py has six
cases, one refusing the old selector. Kept: `--print-plan`, cpu_model and qemu_version in
result.json, host phase durations and guest phase marks. PR #60 was merged first (4d862a1),
so this branch merges main; main's tree equalled the branch's base. Limits: the gate is no
evidence of pointer-authentication strength or of real ARM hardware, and the 18 minutes rest on
one run until this PR's own run. What is left of the 17m45s: 8.3 min before any suite (host
1.4, boot 3.2, candidate 2.7, reinstall 0.7) and 8.1 min of suites, about 2.6 min of it fixed
sleeps scaled by 8 (the PAM lockout alone is 96 s); cloud-init is down to 65 s and an SSH
connection to 12-16 s. Not done, each needing its own measurement and decision: a lower wait
scale, suites in parallel guests, starting the boot before the package build ends. Deferred
minor: `--print-plan` repeats the memory, CPU-count and wait-scale literals instead of sharing
them with the run path. PR #59 stays a draft: its accompanied Steam Deck validation is pending.

2026-10-10 ARM VM, second round (branch perf/arm-vm-await-candidate): PR #61's own run
38016488097 confirmed the adopted profile (ARM job 16m30s, test-vm.py 957 s, 41 assertions,
workflow 20m37s; isolated gates 5m47s with a warm dependency image). The ARM path is still the
longest: package build 3m55s, then the VM job. Comparing that run's TCG and KVM reports, fixed
scaled sleeps removed, an emulated guest is 4.0-4.3 times slower in the session suites but
11.3 in the greeter, 11.9 installing the candidate and 15.7 reinstalling it. The wait scale of
8 therefore stays: halving it, proposed earlier, would leave the greeter and package bounds
with under half of their KVM margin. Change: the guest's preparation and boot (41 s + 207 s) do
not depend on the candidate and its build takes 235 s, so the ARM VM job no longer has
`needs:` and starts with the build. test-vm.py `--package-fetch COMMAND` starts COMMAND with
the boot, ends the boot wait if it fails, requires exit 0 before anything is copied into the
guest and only then resolves `--package` (a quoted glob). scripts/ci-await-candidate lists the
run attempt's jobs through `gh`, downloads the artifact only after the named job succeeded in
this attempt, ends at once on any other conclusion, is bounded (3000 s in the workflow; thirty
consecutive failed listings) and checks SHA256SUMS. The job gets `actions: read`, which
release.yml now grants to the checks it calls, and its timeout is 240 min (bounds sum to 234).
The KVM jobs keep `needs:`. New `ci-await-candidate` gate step: eleven cases against a fake
`gh`, failed first for the missing script. The CLI, workflow and step-order contracts failed
first too; the order contract was checked against three mutations. report: guest_ready and
candidate_wait phases, package fields filled once the candidate arrives, candidate-fetch.log.
Not established: the release path (only a tag exercises it; the permission grant follows
GitHub's documented rule for called workflows), a real run whose package job fails, an API
outage longer than thirty listings. No timing is claimed until this change's own run. Still
open from the first round: suites in parallel guests, a PAM-lockout-only wait scale, SSH
connection reuse (12-16 s per connection).

2026-10-10 ARM VM, second round, result and second lever (branch perf/arm-vm-parallel-suites,
on top of PR #62's commit): run 38019167502 on 14e6771 took 17m13s against 20m37s. The ARM VM
job started 7 s into the run instead of 3m58s, took 17m01s, and its guest waited 0.003 s for
the candidate, which candidate-fetch.log shows arriving after the builder job succeeded; 41
assertions; one sample. That job is now the whole critical path: about 8m20s before the first
suite and 8m03s of suites in turn (reinstall 41 s, session 205, labwc and Wayfire 84, greeter
194). Change: three ARM guests share the suites, `session`, `package+compositors` and
`greeter`, as a job matrix whose results are all required. scripts/vm/suites.sh, sourced by
setup.sh before any work, names the four suites and refuses unknown names; unset means all of
them in the original order, which is what the KVM guests still run. setup.sh runs each suite's
commands under that suite's guard: apart from guards, indentation and comments its commands
are unchanged (checked by diff). The reinstall check became the `package` suite so that each
of the 41 assertions runs in exactly one ARM guest. test-vm.py `--suites A+B` passes the
selection, records it, and fails a guest whose log lacks a completion mark of its own suites
or shows another's; `--print-plan` names those marks. Contracts failed first: the selector
through the real script (seven accepted and five refused selections), setup.sh's guards, the
CLI (plan and seven malformed selections) and the workflow (the matrix covers each suite
once, guests differ in suites only, KVM jobs take no selection). Costs: three ARM runners for
about as long as one ran before; on ARM the suites no longer follow one another in one guest,
so an effect of one suite on a later one is exercised by the KVM guests only. No timing is
claimed until this change's own run.

2026-10-10 0.3.4 release (PR #59): the user approved publishing 0.3.4 and merging PR #59
before its accompanied Steam Deck validation, so that the device test runs on the published
package instead of a CI candidate. This reverses the plan recorded above, which validated a
candidate before merge and proposed no release or tag. The branch merges main (2084f8d; only
this file conflicted and both series of entries are kept) and carries the version bump, the
dated changelog, the README install examples and the limits. Runs 38020534604 and 38023182398
(PR #63 and main at 9f94422) took 11m54s and 12m06s with the same 41 assertions per
target. State of the device when this was written: the installed package is still the 424849c
candidate, the saved greetd configuration is unchanged, greetd was not restarted and no item
of docs/greeter-controller-uat.md has been executed. Published without evidence: pad input on
hardware, controller/uinput access of the greetd account, the handoff to the desktop daemon,
and the Hyprland setup fix through a root installation and a real login. This is the first
release through release.yml since #62 and #63: the `actions: read` grant to the called checks
and the three-guest ARM matrix under workflow_call run for the first time with this tag. The
awaiter's job-name match was compared with the v0.3.3 release run, whose builder job is listed
as `quality / Package (Ubuntu aarch64) / Ubuntu aarch64 package contract`, the prefixed form it
accepts. If the tag workflow fails, the fix goes through a PR and a new version; the tag is not
moved. Next: tag v0.3.4 after main's checks, confirm the assets, then the accompanied device
procedure with the release package.

2026-10-10 0.3.4 on the device, first finding: v0.3.4 was tagged at 0860664 and published by
run 38051684030 (all gates and `publish` passed; the three first-release risks did not occur).
The user installed the release package on the Steam Deck OLED (binary sha256 bbc80f2e...,
BUILD-INFO 0860664). The dry-run plan was reviewed and correct; applying
`sudo decklock setup greeter --compositor hyprland --output eDP-1 --transform 3 --controller`
failed: "Hyprland was launched with superuser privileges, but the privileges check is not
omitted". greetd, the media and the greeter storage were unchanged (compared with the saved
originals), and greetd was not restarted. Root cause: verify_hyprland_config ran
`Hyprland --verify-config` with the caller's privileges, and Hyprland refuses euid 0 before
reading any configuration. 0.3.3 stopped one check earlier, at the runtime directory sudo
removes, so this setup has not worked through sudo in any published version; the user's Deck
runs a hand-written greeter configuration from 2026-09-30. Reproduced without real root in a
user namespace that maps the greeter account (Hyprland 0.56.2): the same Lua is "config ok" as
uid 1000, rejected with that message as uid 0, and "config ok" again when uid 0 runs the
verifier as greeter with storage that account owns; a deliberately broken Lua is still
rejected there. Fix: root verifies as greetd's configured account (uid/gid from /etc/passwd,
no supplementary groups), the configuration lives inside the 0700 verification storage, both
are handed to that account, and under root the storage is created in /tmp because root's
TMPDIR can be private to root (pam_tmpdir): with the /tmp rule removed, a 0700 root TMPDIR
made Hyprland report the configuration as invalid. A missing account or one with UID 0 is
refused before greetd changes. Regressions: the Arch guest installs Hyprland and its greeter
suite runs the setup as root against it, without a runtime directory and with a private
TMPDIR (first pushed alone, to fail in CI); the CLI regression's fake verifier requires the
configuration inside its own storage and owned by its account (failed first); a unit test
covers the account choice. Why the gates missed it twice: the CLI fake accepts any caller and
no VM installed Hyprland. Every manifest now declares SETUP_COMPOSITORS, so a guest without
Hyprland says so. Not established: a real Hyprland login, and everything of the pad
validation, which this failure stopped before greetd was touched. CI run 38058890089 on
b58ca00, the regression alone: the Arch VM failed at the new assertion with that same
message, after 'packaged system setup prepares private storage'; the isolated gates, the
package builds and the Fedora guest passed. The user approved publishing the fix as 0.3.5
from the same PR (#64): version bump, dated changelog and README examples are in it, and
both READMEs say the Hyprland setup needs 0.3.5. Next: tag v0.3.5 after main's checks, then
the device procedure again from the installation step.

2026-10-10 login polish after the device test of 0.3.5 (branch feat/greeter-login-polish): the
user activated the 0.3.5 login on the Steam Deck OLED and reported that everything worked
(wrong password refused at 12:45, correct one accepted at 12:46 in greetd's journal). They
asked for four changes. (1) No "DeckLock — Login" title; the help button becomes a question
mark in the same top-left corner. The button is now a plain "?" label in every window,
independent of the icon theme; the window title keeps the name. (2) The login's help opens one
short page about signing in (docs/guide/*/login.md), not the general and advanced chapters.
(3) Sign in with Enter, which on the Steam Deck is the A button: the visible keyboard captures A
with the other buttons (controller SOURCES) and the event handler ignored it, so with the
keyboard open the only ways to submit were the ghost keyboard's own Enter key or hiding the
keyboard and clicking. A now presses the keyboard's Enter key, as LB and RB press Backspace and
Space, at login and on the lock screen. A physical Enter already submitted: the new XTest
assertion in greeter_keys_check passed before any change. (4) The battery level above the power
buttons in the top-right corner: src/battery.rs reads /sys/class/power_supply (system batteries
only; mains, absent cells and scope=Device batteries such as a controller's are ignored; several
are averaged), refreshed every 30 s, hidden without a battery and in rest mode. ui::Settings gains
power_supply (main and the settings preview use /sys/class/power_supply; every test uses None
or a fixture directory, never the host battery). Regressions failed first: the battery GTK case
(no widget), the A button (no submission), the login title and help button, the help label and
the login guide; the battery unit tests failed against a stub. A first run of the Portuguese
login guide case failed because the test did not set config.locale, which the help follows as
on the lock screen; the test now sets it. An early manual run of greeter_keys_check from this
agent omitted the gate's environment and briefly opened a preview window on the user's Wayland
desktop before failing; later runs used the gate's isolated environment. Not established: the
battery on real hardware (the Deck's BAT1 matches the fixture's layout), the A button with a
physical controller, and the login help window on the Deck's Hyprland greeter.

Same branch, second commit: the user asked that the battery be configurable in CSS and visible
in the preview. The settings preview already read /sys/class/power_supply like the lock screen.
The icon had a fixed pixel size in code, which overrides CSS; it now takes -gtk-icon-size from
the theme (16 px in the default theme, 24 px in contrast), and #battery gets .low (20 % or less,
not charging) and .charging classes. The class assertions failed first; the icon-size assertion
was confirmed to fail by putting the fixed size back (16 against a theme rule of 32). Without a
battery the preview shows nothing, as the lock screen would.

Same branch, third change: the user asked that the lock screen work like the login, with the
full guide only in settings, and for an option to show the battery percentage or the icon
alone. A session lock displays no other window, which is why the lock had no help (help_check
asserted that exclusion). The help of the lock, login and preview is now a panel inside the
screen's own surface (help::screen), not a window: a ? button in the top-left corner (below the
preview banner), F1 to toggle, Esc and its x to close, hidden in rest mode; opening it hides the
keyboard and closing it refocuses the password. Short guides: docs/guide/*/lock.md (new) and
login.md. The settings window and its editors keep the full guide window. This changes the
documented lock contract: the lock now has a help button and F1 controller, still no window
controls, and help_check asserts that opening it creates no other window and authenticates
nothing. layout.battery_percentage (default true) with a settings check box beside the power
options; off hides the percentage and keeps it in the tooltip. Each new assertion failed first:
no help button on the lock, no settings check box, the percentage still shown.

Same branch, fourth change: the user asked for the battery left of the power buttons, in their
row, instead of on a line above, and for its position to be adjustable through CSS. The top-right
corner (#top-right) is now one row; the battery is alone in the corner when the power buttons
are hidden. The row assertions failed first (gap -208 px with the battery above). GTK CSS has no
order or position property, so CSS adjusts placement with margins: #battery relative to the
buttons and #top-right for the whole corner. That assertion passed once the row existed (a
theme rule replaces the default theme's 4 px margin rather than adding to it; the test first
assumed it added). Moving the battery elsewhere on the screen would need a layout option.

Same branch, fifth change: the user preferred the battery back on its own row above the power
buttons, with that row's height reserved across the full width so the help button in the
top-left corner comes down to the power buttons' height. The top of the screen is now one bar
(#top-bar): a status row (preview banner left, battery right) over a controls row (help left,
power right). Without a battery or banner the status row takes no space and the help button and
power buttons rise together; the preview banner moved into the status row so the help button
never overlaps it (its default margin is now below it only). The new layout assertions failed
first (the help button was not level with the power buttons). CSS placement: margins on #battery
within its row and on #top-bar for the whole bar.
The first capture of the bar showed the power buttons stretched to about 170 px each: their
icons expand within their slots, which made the power box expand once it shared a row with a
spacer. The layout test had not caught it; an assertion that each power button stays within
64 px failed first, and the power box no longer expands.

Same branch, sixth change: the user asked to remove the "Wayland · Preview" banner; the preview
now looks like the lock screen, and the preview-banner string and CSS rules are gone. The
assertion that the preview shows no banner failed first. Still identifying a preview: its window
title and, on submit, the notice that authentication is disabled in preview.

2026-10-10 lock, login and rest settings (branch feat/screen-settings, on top of PR #65; spec
.specs/features/screen-settings/spec.md). The user wants the settings to do everything the CLI
does, the login configured from the interface (asking for the administrator password through
polkit when needed), and three settings tabs: Lock, Login (mirroring Lock with a check box, on by
default) and Rest (one for both screens, as today). They approved a contract line for the polkit
exception and asked to hold 0.3.6 until this work is done. Phase 1: Config gains
LoginAppearance (same_as_lock, background_pool, slideshow_seconds, layout), skipped when
default; Config::use_login resolves it and only the greeter applies it (main::apply_screen);
apply_appearance carries it; the CLI edits login.* (a login layout starts from the lock's); the
setup export checks the login pool before migrating anything, migrates it and carries the
section. The unit, parser and CLI/setup tests failed first. A local cargo test run under load 16
(an emulator and a node build) failed the timing-bound auth test
inherited_stdout_does_not_extend_the_helper_deadline; it passed three times alone.
Phase 2: the settings tabs are Lock screen, Login and Rest. A LayoutForm builds each screen's
layout controls (the lock keeps the settings-* widget names, the login uses settings-login-*),
each with its own restore button; "Layout and preferences" keeps the shared options. The Login
tab has "Same as the lock screen" and, when off, its own pool, interval and layout. The live
preview gains a login mode (Config::use_login, greeter view) and a ui::build_preview that never
requests fullscreen; ui::build keeps the kiosk behavior for the real and standalone greeter. The
tab and login assertions failed first. While adding them, the settings cleanup check failed
because the new test code held the tab stack until the end; scoping those references fixed it,
which shows the window itself releases its controls.
Phase 3: "Apply to login" in the Login tab saves and runs `pkexec <decklock> setup
login-appearance`. That new subcommand refreshes only the login's appearance snapshot in the
storage directory its greetd command names, refusing a login without DECKLOCK_GREETER_APPEARANCE
in its command; it never rewrites the greetd command, so compositor, keyboard and controller
choices stay (a plain repeat of setup greeter regenerates a Cage command from its flags).
setup::invoking_uid reads SUDO_UID or PKEXEC_UID, never root. Packages carry
packaging/polkit/io.github.yan_vidal.DeckLock.policy (exec.path /usr/bin/decklock, exec.argv1
setup, auth_admin_keep for the active session); Arch optdepends polkit, Fedora recommends polkit,
Ubuntu recommends pkexec. AGENTS.md gains the exception line the user approved. setup status
now recognizes the generated Hyprland login (it looked only for `decklock --greeter` in the
command). Each test failed first: the status regression, the uid helper, the package contract,
the login-appearance CLI and the settings button with a fake pkexec. Not exercised: the real
polkit dialog and agent, and a root write to /etc/greetd through pkexec. This Deck runs polkitd
but no polkit agent in its Hyprland session, so the button would show the terminal command
until one (such as hyprpolkitagent) is installed and started.
Checked against the real pkexec on this Deck, without elevating anything (`pkexec /usr/bin/true`,
no agent running): without a terminal it fails with "Error creating textual authentication
agent" (exit 127); started from a terminal it would prompt there, away from the window. The
button now passes --disable-internal-agent, whose real answer is "No authentication agent
found." with exit 127, the case the fake reproduces; the test failed first on the old argv.

2026-10-10 emulated ARM guests booting into emergency mode (branch ci/arm-boot-hang-evidence):
the aarch64 `greeter` guest missed its boot deadline twice (run 38083250241 attempt 1 on
be38b62, run 38096972130 attempt 1 on 1786fe0); both reruns of only that job passed. The first
reading, recorded in PR #66's description, was a guest frozen in silence: wrong, it came from
the end of the serial log only. The whole console shows systemd timing out the devices of /boot
and /boot/efi after 90 s, failing local-fs.target, systemd-sysctl and systemd-journal-flush,
and printing "You are in emergency mode". sshd never starts there, and test-vm.py did not
recognize it, so it waited out its 32-minute deadline. The journal.log that setup.sh already
collects, from 50 passing boots of the same day in all three guests, shows the mechanism: from
"Switching root." to the device job of /boot being queued takes 15 s as a rule, but 40 to 112 s
in 11 of them, and systemd-udevd starts only after systemd-sysusers, so no disk can appear
before that stretch ends. A passing greeter boot had already spent 52 of the 90 s. Both failures
landing on the greeter guest is chance. The runner scales its own waits by 8 under emulation;
the guest's systemd timeouts were not scaled. The user chose to scale them over detection alone
or an automatic boot retry. vm_native.chroot_script writes 90-decklock-emulation.conf drop-ins
into an emulated guest's disk (DefaultDeviceTimeoutSec and the TimeoutSec of systemd-sysctl,
systemd-journal-flush and systemd-sysusers, 90 s times the slowdown), nothing for a KVM guest;
test-vm.py passes the slowdown, records guest_boot_timeout_scale and, after each failed SSH
probe, reads the console: emergency.target ends the wait at once with the units the console
names. setup.sh records boot-timeouts.txt and failed-units.txt. Each assertion failed first:
the detector against a real emergency console and a real booted one (scripts/fixtures), the
drop-ins through the real chroot script with stubbed tools, the pass-through and the wiring.
Not established: why the stretch after switch-root is slow in about a quarter of emulated boots
(no kernel stall warning in the journals, host memory normal; QEMU 8.2.2, multi-threaded TCG),
and the in-guest fallback, which keeps the stock timeouts.

2026-10-10 release 0.3.6 (branch release/0.3.6): the user decided the 0.3.6 release carries the
login polish (PR #65) and the lock, login and rest settings with "Apply to login" (PR #66), and
waits until both are merged; they are. The user asked for the release pull request; it also
carries #67, a CI-only change. Cargo, the lock file and both READMEs' install sections move to
0.3.6; the changelog's Unreleased becomes 0.3.6 dated 2026-10-10, with a blank line and an
overlong line inside the battery entry fixed and a limitations section added (a polkit agent is
needed for "Apply to login"; the real polkit dialog, the A button with a physical controller,
the battery on real hardware and the login help on a Steam Deck login are not established). The
README lines saying Hyprland login setup needs 0.3.5 or newer stay true and are kept. Publishing
still needs the user's approval of the tag after merge. The Steam Deck checks of 0.3.5 left
unrecorded (pads at login, handoff to the desktop, reconnection on the lock screen, login
without controller) and the new items of 0.3.6 are to be tested on the published 0.3.6 package.
