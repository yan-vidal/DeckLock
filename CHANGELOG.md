# Changelog

User-facing changes are reviewed in pull requests. Dates are assigned when a release is approved. Packaging-only rebuilds use Arch pkgrel and a separate -rN tag; published assets are never replaced.

## [Unreleased]

### Added

- The login can have its own background pool, slideshow interval and layout,
  through a `[login]` section with `same_as_lock = false`
  (`decklock config set login.same_as_lock false`, `login.layout.clock_visible`,
  `login.background_pool`). With `same_as_lock`, the default, the login shows what
  the lock screen shows, as before, and the file keeps no `[login]` section. Rest
  stays one for both screens. Login setup migrates the login's media to the shared
  library like the others and fails before migrating anything if one is unreadable.
- Settings have three tabs: Lock screen, Login and Rest. Lock screen holds its
  pool, interval and layout; Login holds "Same as the lock screen" (on by default)
  and, when off, its own pool, interval and layout; Rest is unchanged and applies to
  both screens. Each layout has its own "Restore defaults". The preview shows the
  screen of the open tab, the login included, in an ordinary window. "Layout and
  preferences" keeps the options shared by all screens.
- "Apply to login" in the Login tab saves the settings and applies them to the login
  screen through polkit: the session's authentication agent asks for the
  administrator password, which DeckLock never sees. It runs the new
  `decklock setup login-appearance`, which refreshes only the login's appearance and
  media and refuses a login that does not use DeckLock; the greetd command,
  compositor, keyboard and controller stay as they are. Packages install a polkit
  action limited to `/usr/bin/decklock setup`. Without a polkit agent (common on
  Hyprland, Sway, labwc or Wayfire) or without `pkexec`, the button shows the terminal
  command instead. Setup run through `pkexec` finds the desktop user like `sudo` does.

- The lock, login and preview screens show the battery level on devices with a
  system battery, in the top-right corner on its own row above the power buttons.
  That row spans the width, so the help button in the top-left corner lines up with
  the power buttons below it; without a battery the row takes no space. It is read from the Linux power-supply
  class every 30 seconds; mains adapters and the batteries of peripherals such as a
  controller are ignored, and nothing is shown without a battery. Rest mode hides it
  together with the power buttons, and the settings preview shows it like the lock
  screen. Themes style it through CSS: `#battery`, its `image` and `label`, the icon
  size with `-gtk-icon-size`, and the `.low` (20 % or less, not charging) and
  `.charging` states. Margins on `#battery` move it within its row, and margins on
  `#top-bar` move the whole top bar.
- Layout option "Show battery percentage" (`layout.battery_percentage`, on by
  default); off shows the battery icon alone, with the level in its tooltip.
- The lock screen, login and preview have a question-mark help button in the top-left
  corner. It opens one short page in plain words, about unlocking or signing in,
  inside the screen itself: a session lock shows no other window. F1 opens it too,
  and Esc or its × close it. The full guide stays in settings.

### Changed

- The login no longer shows a "DeckLock — Login" title; its help button takes the
  top-left corner. Help buttons in every window are a plain question mark, which no
  longer depends on the icon theme. The preview's help button opens the short lock
  guide instead of the full guide.
- The preview no longer shows a "Wayland · Preview" banner; it looks like the lock
  screen. Its window title and the notice shown on submit still say it is a preview.

### Fixed

- `decklock setup status` recognizes a login generated for Hyprland, which starts
  DeckLock from its Lua file; it reported it as not configured.
- The A button confirms like Enter while the on-screen keyboard holds the controller,
  at login and on the lock screen. The keyboard captures A along with the other
  buttons and ignored it, so with the keyboard open the password could only be sent
  with the keyboard's own Enter key, or by hiding the keyboard and clicking.

## [0.3.5] - 2026-10-10

### Fixed

- `setup greeter --compositor hyprland` and `setup all --compositor hyprland` work
  through sudo. Hyprland refuses to start with superuser privileges, even to verify
  a configuration, so root setup rejected its own valid Lua and left greetd
  unchanged: 0.3.4 stopped there, and 0.3.3 one step earlier, at the runtime
  directory sudo removes. Root now verifies as greetd's configured account, with
  storage that account owns under `/tmp`, and never as root. A missing account
  or one with UID 0 is refused before greetd changes.

### Compatibility and limitations

- The Arch VM gate now installs Hyprland and runs this setup as root against it;
  the 0.3.3 and 0.3.4 gates used a fake verifier that accepted any caller. An actual
  Hyprland login, GPU behavior and the Steam Deck validation of pad input remain
  outside what the suites establish.

## [0.3.4] - 2026-10-10

### Added

- Opt-in Steam Deck pad input before authentication: `setup greeter --controller`
  and `setup all --controller` launch a greeter-owned sc-controller daemon with a
  private runtime profile and the existing embedded keyboard. Repeat setup preserves
  the choice; `--controller=false` disables it. Missing/failed controller input keeps
  mouse/touch fallback. Preview never starts this daemon, and greeter termination
  stops it. Tests use fake processes and isolated GTK; physical device validation
  remains pending.

### Fixed

- Hyprland login setup verifies its generated Lua with private temporary
  HOME/XDG storage, including a runtime directory. Installation works when sudo
  removes `XDG_RUNTIME_DIR`; rejected configuration still preserves saved greetd
  and Lua files.
- A ready sc-controller daemon without a connected device keeps the clickable
  keyboard fallback. Device hotplug restores pad mode without restarting DeckLock.

### Changed

- The required checks finish in about 12 minutes instead of about 57 on the measured
  runs. Test dependencies come from a prepared image and caches of pristine
  downloads, package builds run concurrently, and the emulated Ubuntu arm64 guest
  uses QEMU's implementation-defined pointer authentication, boots while its
  candidate builds and shares the suites with two more guests. Every target still
  runs the same 41 assertions on the candidate installed in a running guest; on
  arm64 the suites no longer follow one another in a single guest. This affects CI
  only.

### Compatibility and limitations

- Pad input before authentication is published before its validation on a physical
  Steam Deck. Its tests use a fake `scc-daemon` and isolated GTK; the packaged VM
  gates install no sc-controller and exercise no USB or uinput device. Controller
  and uinput access for the greetd account, pad ergonomics and the handoff to the
  desktop's sc-controller daemon are not established. A greeter-owned daemon that
  dies is not restarted, and abrupt termination can leave static runtime files until
  runtime cleanup. `docs/greeter-controller-uat.md` gives the accompanied procedure
  and recovery.
- The Hyprland setup fix is covered by a real-CLI regression and was checked against
  the Hyprland 0.56.2 parser in isolated storage. A root installation and a real
  login with it are not established.

## [0.3.3] - 2026-10-03

### Fixed

- Login setup shares one canonical media library between the locker and greeter, readable by all local accounts and writable only by its desktop owner. Selected normal/rest media and user-library media are migrated as that owner; original paths and old generated media copies become links. Same-filesystem migration preserves the inode; cross-filesystem transfer is verified before replacing the source. Setup prefers the common HOME volume outside the personal HOME when it differs from the system volume, or `/var/lib/decklock/media/UID` otherwise, supports `--media-dir`, and persists SELinux labels. Private appearance/theme/config generations remain for recovery, and invalid/unreadable selections preserve the prior login/appearance.
- Greeter startup with service-account `HOME=/` no longer fails while trying to create optional personal media directories. Existing readable media and packaged fallback backgrounds remain available; lock/preview report optional directory failures as warnings.
- Login setup prepares private storage owned by the configured greetd account, checks ancestor access and preserves enforcing SELinux labels before changing its login command. Cage fullscreen is requested after the initial surface frame.
- Hyprland login setup supports the Steam Deck OLED panel/touch rotation through verified Lua configuration, preserves an existing Hyprland greeter on repeat and keeps backups.
- X11 stacking recovery runs independently of GTK frame processing while retaining GTK input grabs and authentication; the isolated VM display also keeps its server generation across client disconnects.
- Lock setup preserves shell expressions and unrelated hypridle settings, migrates known lockers and keeps custom wrappers unless explicitly replaced.
- `setup all` accepts the same login compositor/rotation options and configures the sudo caller's hypridle as that user, preserving file ownership, permissions and backups. It validates both configurations before writes and reports completed lock changes if login setup fails later.
- Lock, login and preview controls automatically fit logical viewports below 1280×800. At scale 1.6 on a Steam Deck, they match the size of the scale-1 login; media stays fullscreen and keyboard hit testing follows the scaled controls.
- The controller connection survives a restart of the sc-controller daemon: the keyboard reconnects instead of staying without pad input until DeckLock restarts. The pads are captured again only while the keyboard is still active, and a protocol refusal or an overflowing event queue still ends the connection for good. Connection changes reach every lock surface, and a reconnect restores the pad keyboard geometry and clears the "controller unavailable" notice.
- `decklock setup greeter` and `setup all` no longer promise that a changed login command takes effect at the next logout. greetd reads its configuration when the daemon starts and a healthy daemon starts the cached greeter command again after logout, so the new command needs a reboot, or ending the managed desktop session and restarting greetd from a separate TTY. Setup still never restarts services or the active session. The command output, both READMEs and the offline guides say so.

### Changed

- The emulated Ubuntu arm64 VM gate installs its harness packages on the host before the guest boots and finishes in about 35 minutes instead of about 94, with the same 41 assertions. A host step that cannot run falls back to the previous in-guest install with a visible warning, and a latent timing race between the gate's Sway start-up and its first `swaymsg` call was fixed. This affects CI only.

### Compatibility and limitations

- Desktop zoom and existing theme/layout configuration remain unchanged. The greeter keeps private configuration/state; shared media deliberately becomes public to local readers (0755 directories, 0644 files). HOME permissions stay unchanged. Repeat setup to refresh selected pools/theme, not duplicate media; CLI/GUI imports use the saved `media_library`. Private/encrypted HOME mounts are not made available before login, and manually supplied config paths still need readable files.
- Controlled GTK and packaged VM checks do not establish physical touchscreen/controller ergonomics, every compositor or real GPU behavior on a Steam Deck. Controller reconnection is tested against a fake daemon, not a real sc-controller restart with a physical controller.

## [0.3.2] - 2026-09-24

### Added

- Ubuntu 26.04 `aarch64` (arm64) package, published alongside the x86_64 packages. It goes through the same disposable VM gate as the others before release: real Sway and Linux-PAM, X11, labwc, Wayfire and greetd login. GitHub's arm64 runners have no hardware virtualization, so that guest is emulated and its waits scaled.
- The VM gate repeats the lock boundary on Wayfire, rendering with software GLES, on Arch, Fedora and Ubuntu.
- A weekly CI job checks whether a Debian release or backport carries `gtk4-layer-shell` 1.2, the floor a Debian package needs (#25).

### Changed

- The X11 gate requires the lock screen on top in every sample once the window manager settles, instead of 19 of 20 samples counted from its first recovery, which made the outcome depend on timing.

### Compatibility and limitations

- The Fedora 43 `aarch64` package is built and passes the package contract but is not published: it has no VM target yet.
- The arm64 package is validated in an emulated VM, not on real ARM hardware.
- Wayfire is validated with software rendering, not on real GPU drivers. Hyprland, niri, river and COSMIC stay covered by protocol compliance only; Hyprland needs a real GPU even for headless outputs.

## [0.3.1] - 2026-09-19

### Added

- Experimental X11 lock backend behind the `x11` Cargo feature, built into candidate release packages. It uses an override-redirect window per monitor plus keyboard and pointer grabs, keeps itself on top and takes X focus back from the window manager, and refuses to lock when it cannot take the keyboard.
- The lock screen states a reduced guarantee while it is up, and the settings window reports it too: on X11 any other program in the session can read what is typed, and the screen unlocks if DeckLock stops. A theme cannot hide that notice.
- An X11 lock gate that runs the real binary against a private Xvfb and xfwm4, with a helper playing another client in the session.
- The `x11` feature builds the GStreamer sink's X11 GL support, so video backgrounds take the accelerated path on X11 instead of the software one.
- Disposable VM gate exercises real PAM authentication, session release, and wrong password rejection under the X11 backend in addition to Wayland.
- Multi-architecture packaging support: PKGBUILD, RPM spec, and DEB control files dynamically support both `x86_64` and `aarch64`.
- Compatibility matrix in `README.md` and `README.pt-BR.md` covering compositors, distributions, and architectures.

### Changed

- The offline guide states, per distribution, whether the account-lockout notice can appear at all and how to make the system count failed passwords. On stock Fedora and Ubuntu nothing counts them, so the notice never appears; DeckLock does not change that, because it is the administrator's policy.
- The offline guide describes what a build with the `x11` feature provides instead of stating that X11 locking does not exist.
- The lock is chosen through a `LockBackend` abstraction that states what each protocol guarantees, instead of the Wayland session-lock instance reaching the caller directly. Wayland behavior is unchanged, and a Wayland session is never downgraded to X11.
- Formally completed Phase C (X11 backend) and closed Phase D (BSDs out of scope).

### Compatibility and limitations

- GNOME and KDE Plasma Wayland sessions are not supported: both draw their own lock screen inside the desktop and do not let another program replace it. DeckLock refuses to lock there.
- X11 support offers reduced security guarantees compared to Wayland: session-level processes can snoop input or terminate the locker.
- Multi-architecture packaging supports `x86_64` and `aarch64`.

## [0.3.0] - 2026-09-16

### Added

- Fedora 43 RPM and Ubuntu 26.04 DEB packages, published alongside the Arch package. Each is built against its own distribution's libraries, and the packaged binary is byte-identical to the one the package tests verified.
- Each package installs its own `/etc/pam.d/decklock` including that distribution's authentication stack, with only `auth` and `account` lines because DeckLock never opens a session.
- `--lock` refuses, naming the expected file, when the configured `pam_service` has no service file. Without this, PAM falls back to `/etc/pam.d/other`, every attempt is denied and the compositor keeps the session locked, trapping the user.
- A "Which desktops work" section in both READMEs, listing tested, expected and unsupported desktops with the reason GNOME and KDE Plasma are unsupported.
- Disposable VM gate per distribution: each package is installed into its own Arch, Fedora or Ubuntu guest and exercised against real Sway and real PAM, with Fedora's SELinux left enforcing.

### Changed

- `pam_service` defaults to `decklock` instead of `login`. Existing configurations holding the previous default migrate on load and save; any other value is left alone, including a deliberate `login`, which cannot be told apart from the old default.
- The refusal on an unsupported desktop explains that the desktop does not let other programs provide the lock screen, and names compositors that work, instead of stating only that the compositor lacks session locking.
- Releases publish every package with one combined `SHA256SUMS`, and the channel follows the source version: a plain version publishes as a full release.

### Fixed

- The documented `gtk4-layer-shell` requirement was 1.3; the floor derived from the binary's symbol versions is 1.2.

### Compatibility and limitations

- GNOME and KDE Plasma are not supported and are not planned: both draw their own lock screen inside the desktop and do not let another program replace it. DeckLock refuses to lock there.
- Ubuntu 24.04 and Debian 13 are not supported. Ubuntu 24.04 does not package `gtk4-layer-shell`, and Debian 13 packages 1.0.4, below the required 1.2.
- With a stock configuration the lockout notice can only appear on Arch. Fedora and Ubuntu do not count failed passwords by default, so there is no lockout to report until an administrator enables it.
- On Fedora and Ubuntu the VM gate installs Sway to exercise locking, because neither ships a compositor implementing `ext-session-lock-v1`. The packages work where such a compositor is installed, not on those distributions' default desktops.
- Derivatives sharing these repositories are not tested. This remains experimental: physical-device recovery, GPU paths, accessibility and battery use remain unverified.

## [0.2.0] - 2026-09-09

### Added

- Disposable Arch/QEMU integration gate using real Sway and PAM, the packaged executable, isolated keyboard input and visible lockout notices.

- Eight procedural media items: Starfield, Particles, Lissajous, Matrix rain, Doom fire, Aurora, Flow field and Ridgeline.
- Procedural thumbnails and per-item live editors in the library and media viewer.
- Preview diagnostics for generated FPS, drawing CPU, process CPU and process RSS.
- Offline F1 help with General and Advanced chapters in English and Brazilian Portuguese, following the selected interface language.
- Consistent close controls for ordinary windows and a GUI/CLI preference to hide title bars.
- Lock-screen notice when PAM reports the account locked by failed attempts, with a countdown, additional warnings supplied by PAM, and `#status.warning`/`#status.locked` theme selectors.
- Power-button visibility and restore controls for layout and theme drafts.
- Embedded application artwork, desktop/package icons and bilingual README branding.
- Deterministic regression gates, isolated GTK/Wayland tests and CI-built release candidates.

### Changed

- Procedurals replace the background and participate in normal/rest pools like videos, rather than acting as overlays. Unreleased overlay configurations migrate on load/save.
- Video thumbnails load serially in the background and share a bounded cache across lists.
- Release artifacts and tags retain the full three-part Cargo version; package revisions remain separate.
- Releases combine this reviewed summary with generated pull-request references.

### Fixed

- Frame limiter aliasing against the display clock, which could reduce a 30 FPS target to 20 FPS.
- Metrics ignoring the selected language and inconsistent initial procedural speeds; all now default to 1 without replacing saved custom values.
- Package archive-root naming and fragmented mock-compositor response handling.

### Compatibility and limitations

- Existing 0.1 configurations retain defaults for new fields. window_decorations affects ordinary windows only; a real lock never gains a close/help action.
- Preview CPU/RSS totals describe the settings process, not exclusive procedural or GPU consumption.
- The lockout notice repeats PAM's own report and never enforces a policy: input stays enabled, the countdown is an estimate in whole minutes, and no remaining-attempt count is inferred from local policy files.
- This remains experimental. A disposable VM tests the installed package against real Sway and PAM. Other PAM policies/compositors, physical-device recovery, accessibility and battery use remain unverified.
- The packaged binary targets Arch Linux x86_64 and its declared shared-library versions; other distributions should build from source.

## [0.1-r2] - 2026-09-08

### Fixed

- CLI option ordering and packaged-executable checks. Packaging revision 2 retains application version 0.1.0.

## [0.1] - 2026-09-08

### Added

- Initial experimental Rust/GTK4 Wayland locker with virtual keyboard, optional external sc-controller integration, configurable themes, media pools, rest mode and bilingual settings.
- Author-supplied Osaka video and Arch binary distribution.
