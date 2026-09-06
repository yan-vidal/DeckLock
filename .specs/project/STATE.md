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
