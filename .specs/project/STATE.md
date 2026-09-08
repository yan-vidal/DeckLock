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
accompanied by `assets/media/CREDITS.md` with explicit CC BY-SA 4.0 redistribution terms.
Added `packaging/` directory containing desktop entry, Arch PKGBUILD template, and
`scripts/package-release.py` staging release archives and Arch recipes for version 0.1.
README.md and README.pt-BR.md restructured to prioritize installation and usage,
moving build instructions to the development section. 38 core unit tests and clippy pass.

