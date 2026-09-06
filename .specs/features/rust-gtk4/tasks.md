# Tasks

- [ ] T1 (R1–R3): Cargo scaffold; tested configuration/theme and Fluent loading.
  Files: Cargo.*, src/config.rs, src/i18n.rs, locales/, themes/. Gate: core tests.
- [ ] T2 (R5): Authentication helper and lock state rules. Files: src/auth.rs,
  src/session.rs. Gate: helper failure and state transition tests; no live PAM.
- [ ] T3 (R1–R4): GTK4 preview, configurable UI, media, embedded keyboard.
  Files: src/main.rs, src/ui.rs, src/keyboard.rs. Gate: build + preview inspection.
- [ ] T4 (R5,R7): Session-lock adapter, surfaces/hotplug, authenticated unlock.
  Files: src/lock.rs. Gate: build and isolated compositor tests where available.
- [ ] T5 (R6): Optional controller protocol and embedded keyboard adapter.
  Files: src/controller.rs, src/ui.rs. Gate: fake Unix daemon tests.
- [ ] T6 (R1–R8): Docs, parity report, gates, preview screenshots and handoff.
  Files: README.md, docs/rust-migration.md, .specs/. Gate: cargo test, fmt, clippy,
  build, git diff --check. Device/real-lock UAT explicitly distinguished.
