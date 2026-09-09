# Changelog

User-facing changes are reviewed in pull requests. Dates are assigned when a release is approved. Packaging-only rebuilds use Arch pkgrel and a separate -rN tag; published assets are never replaced.

## [0.2.0] - Unreleased

### Added

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
- This remains experimental. Real PAM policy, compositor/device recovery, accessibility and battery use require manual validation.
- The packaged binary targets Arch Linux x86_64 and its declared shared-library versions; other distributions should build from source.

## [0.1-r2] - 2026-09-08

### Fixed

- CLI option ordering and packaged-executable checks. Packaging revision 2 retains application version 0.1.0.

## [0.1] - 2026-09-08

### Added

- Initial experimental Rust/GTK4 Wayland locker with virtual keyboard, optional external sc-controller integration, configurable themes, media pools, rest mode and bilingual settings.
- Author-supplied Osaka video and Arch binary distribution.
