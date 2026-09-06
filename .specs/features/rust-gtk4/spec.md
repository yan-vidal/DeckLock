# Rust / GTK4 migration

Status: in progress. Approved direction: Wayland only, Rust, GTK4, external CSS
themes, extensive customization, localization from the start. Lua/plugins deferred.

## Requirements

- R1: Rust application with explicit `--preview` (normal closable window) and
  `--lock` (ext-session-lock). Preview never authenticates, powers off, or captures
  controllers by default. No implicit real lock during development.
- R2: External theme directory containing theme.toml, style.css and optional
  local media. Layout configuration controls arrangement, alignment, spacing and
  visibility of decorative elements; authentication is owned by application code.
- R3: English and Brazilian Portuguese Fluent catalogs, locale fallback and
  external translation override. No translated strings in authentication logic.
- R4: Physical keyboard and clickable embedded keyboard, password masking and
  reveal, Unicode edits, submit and visible errors, clock/date, avatar, idle mode,
  image/video backgrounds. Video is muted and looped.
- R5: Only successful PAM authentication may request unlock. Separate helper
  process, asynchronous UI, bounded input and authentication timeout. Signals,
  crashes, close requests and theme changes must not request unlock.
- R6: Optional sc-controller daemon integration over Unix socket; no Python
  imports or runtime dependency for keyboard/mouse operation. Capture refusal
  degrades gracefully and does not kill another daemon.
- R7: One lock surface per output, monitor hotplug, explicit unsupported-
  compositor errors. Real-lock validation is separate from preview validation.
- R8: Keep Python reference operational during migration. Record parity gaps
  honestly, including standalone OSK, generic layout synchronization and device UAT.

## Deferred

X11, Lua/plugin runtime, arbitrary theme scripts, standalone Rust OSK input
injection, claims of universal compositor compatibility or performance improvement.

## Acceptance

Cargo tests, formatting, clippy, build and git diff checks; preview inspection on
the current Wayland desktop. Locking tests use an isolated compositor if available;
the active user session is not a test fixture. No real password is requested/logged.
