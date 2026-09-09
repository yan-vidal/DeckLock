# Inside DeckLock

This chapter introduces the implementation for curious users and contributors. It describes the current design, not a guarantee of security or performance on every desktop.

## Rust and GTK

Rust owns configuration, application state, keyboard logic and rendering orchestration. GTK4 builds ordinary settings/preview windows and the lock-screen widgets. CSS styles GTK widgets; TOML describes validated layout and configuration. Themes cannot run scripts. Lua/plugins are not implemented.

The actual locker uses gtk4-session-lock and the compositor's ext-session-lock-v1 protocol. Ordinary window decorations belong only to settings, editors, help and preview. X11 lock support is not implemented.

## Authentication boundary

The session state machine authorizes unlock only after compositor lock ownership and a successful response for the current authentication attempt. PAM runs through a separate helper. Stale replies, failed authentication, preview, signals and window closure must not authorize unlock.

The UI is not the authority for session ownership. A successful preview test does not demonstrate real PAM policy or compositor recovery. SIGTERM is not an unlock shortcut.

The helper process returns acceptance, refusal or error through its exit code, and now also the text modules ask to display: only `PAM_ERROR_MSG` and `PAM_TEXT_INFO`, never the prompt we answer and never the password. That text is flattened to one line, bounded in bytes and rendered as a plain label, with no markup interpretation. The helper runs under `LC_ALL=C` so module wording can be recognized and re-rendered in the interface language instead of being matched against translations.

Lockout notices come exclusively from PAM messages. Recognized pam_faillock messages are translated; other sanitized messages remain plain text. DeckLock neither reads faillock.conf nor invokes faillock(8): module arguments, service stacks and failure windows can make a separate policy calculation incorrect. The countdown uses a CLOCK_BOOTTIME deadline, including suspension; delayed GTK callbacks only delay repainting, never extend the deadline. Starting another authentication cancels the old countdown. Helper stdout is drained without blocking, retaining at most 512 bytes and enforcing the authentication timeout even if descendants inherit the pipe.

This path is presentation only. It authorizes nothing, refuses nothing and delays nothing; the password field stays enabled during the countdown, because the policy belongs to PAM and only PAM decides when an attempt is accepted.

## Media pipeline

GStreamer decodes video into a GTK paintable. Images and procedural textures occupy the same background stack; photos transition using its crossfade. Selection is separated from rendering, so a video or procedural stays selected for a lock while photos use a photo-only slideshow.

Built-in procedural references use procedural: identifiers rather than filesystem paths. Per-item parameters are stored under procedurals in TOML and are shared by both pools. The seed and injected time make frame generation repeatable.

Cairo renders procedural frames into bounded pixel buffers, with the largest dimension limited to 640 pixels. GTK scales the texture to the window. A frame-clock callback limits updates to the configured rate, at most 30 FPS, and stops receiving ticks while unmapped. This is bounded raster rendering, not imported GLSL or a scripting runtime.

Video thumbnails are decoded serially on a background worker. The settings catalog shares pending requests and caches up to 128 completed results, including failures, by path and file metadata. Owned pixels return to the GTK thread for texture creation; weak widget references avoid retaining discarded rows.

## Metrics and interpretation

Drawing CPU uses the current thread's CPU clock around rendering. Process CPU uses the process CPU clock; 100 percent corresponds to one fully occupied core. RSS comes from /proc/self/status. These process figures include GTK, media and configuration work. Generated FPS does not measure compositor presentation latency or GPU utilization.

## Configuration and localization

Serde reads TOML with defaults and rejects unknown fields. Validation runs before atomic saves. CLI commands and the GUI share the same schema; changing an unrelated field must preserve existing values. The window_decorations preference affects ordinary windows only.

Fluent catalogs provide English and Brazilian Portuguese. The guide uses Markdown source documents embedded at build time and also included in the package. The offline renderer supports headings, paragraphs, lists and fenced code; it does not execute HTML, scripts or remote content. A future static website can consume these same Markdown files.

sc-controller remains an optional external dependency communicating through a Unix socket. Power requests are systemctl calls; system policy stays outside DeckLock.

## Working on the project

```sh
scripts/cargo-local run -- --settings
scripts/check
scripts/check --all
```

Read AGENTS.md and docs/testing.md before editing behavior. Add a failing regression at the boundary that failed: CLI executable, isolated GTK or protocol peer. Use fixed seeds, private HOME/XDG paths, fake controller daemons and bounded waits. Never point automated tests at a real user's lock session, PAM password or controller.

The complete gate checks formatting, Clippy, Rust/CLI tests, example builds, package contracts, whitespace, isolated GTK and a mock Wayland compositor. It cannot certify real-device ergonomics, battery life, PAM integration or security.

## Releases and contribution flow

Changes go through a PR with required checks. CI builds candidate Arch artifacts. A version tag on reviewed main source reruns verification, validates the version and publishes checksummed artifacts with a reviewed changelog plus generated PR references.

Cargo and release tags use the full three-part version. Arch pkgrel tracks packaging revisions separately. Version 0.2.0 is being prepared; publication remains a separate step. The roadmap lists directions rather than promised delivery dates.
