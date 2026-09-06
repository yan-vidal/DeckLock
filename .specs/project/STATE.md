# State

Rust/GTK4 migration started on feat/rust-gtk4. Python main remains the reference.
Wayland only; X11 removed from scope by user. CSS + declarative theme layout;
i18n now, plugins/Lua later. No measured Rust performance claim.

Environment: GTK 4.22.4, GStreamer 1.28.6, Rust 1.96.0, Hyprland.
gtk4-layer-shell missing from pkg-config; prepare local development dependency.
Installed sc-controller: 0.7.1. Embedded input can edit the password directly;
standalone OSK requires an additional input injection backend.

Validation pending. Preview does not validate compositor lock or PAM.
