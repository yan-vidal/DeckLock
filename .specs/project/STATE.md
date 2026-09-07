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
