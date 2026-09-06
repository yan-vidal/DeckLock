# State

2026-09-06: first Rust/GTK4 implementation verified on `feat/rust-gtk4`.
Python `main` remains the reference; public origin is yan-vidal/DeckLock.
Wayland only; external CSS + declarative TOML layout; Fluent i18n now,
plugins/Lua later. No measured performance claim.

Environment: GTK 4.22.4, GStreamer 1.28.6, Rust 1.96.0, Hyprland.
gtk4-layer-shell 1.3 built locally in ignored `.deps/`; no system installation.
Use `scripts/bootstrap-native --tests` to reproduce the native setup.
Installed sc-controller: 0.7.1; Rust integration uses its external Unix daemon.

Validation passed: 21 unit/integration tests, fmt, clippy with warnings denied,
build, configuration check, GTK preview harness, and isolated compositor test.
The latter covers acquisition, monitor add/remove/re-add, SIGTERM without unlock,
and refusal of a second locker after the first dies. Preview was visually
inspected in Portuguese/English with keyboard and a looping test video.
No real-session lock, real PAM password, physical controller or power action was
used for validation. No automatic replacement of the existing locker.

Next: device UAT, controller recovery/ergonomics, ghost keyboard/haptics,
XKB/AltGr/custom keyboard layout, and standalone OSK parity. Full details and
reproduction commands: `docs/rust-migration.md`, `README.md`.
