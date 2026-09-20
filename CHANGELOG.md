# Changelog

User-facing changes are reviewed in pull requests. Dates are assigned when a release is approved. Packaging-only rebuilds use Arch pkgrel and a separate -rN tag; published assets are never replaced.

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
