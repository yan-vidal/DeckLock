# Design

Core modules are independent of GTK where practical: configuration/theme loading,
Fluent localization, input model, controller protocol, authentication boundary.
GTK4 owns UI and media; gtk4-session-lock owns Wayland surfaces. The same view
constructor serves preview and lock. UI theme files never select authentication
callbacks or execute commands. PAM authenticates the actual uid through an isolated
helper before any GTK initialization. No handler connects SIGTERM to unlock.

Use the installed native GTK4/GStreamer and a locally built gtk4-layer-shell for
development if unavailable in the distribution. Pin resolved Cargo dependencies.
CSS is GTK CSS (not browser CSS). Theme TOML supplies safe, bounded layout choices;
external Fluent files supply translations with a built-in English fallback.

Controller support implements the installed daemon's line protocol independently,
with bounded buffering and capture lifecycle. Embedded keys edit the GTK field
directly; standalone OSK injection remains in the Python reference for now.
