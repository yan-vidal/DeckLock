fn main() {
    for path in [
        "assets/icons/icons.gresource.xml",
        "assets/icons/decklock.svg",
        "assets/icons/decklock-symbolic.svg",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let target =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("icons.gresource");
    let status = std::process::Command::new("glib-compile-resources")
        .args([
            "--sourcedir=assets/icons",
            "assets/icons/icons.gresource.xml",
            "--target",
        ])
        .arg(target)
        .status()
        .expect("glib-compile-resources is required (GLib development tools)");
    assert!(
        status.success(),
        "Failed to compile DeckLock icon resources"
    );

    pkg_config::Config::new()
        .atleast_version("1.3")
        .probe("gtk4-layer-shell-0")
        .expect("gtk4-layer-shell >= 1.3 is required; see scripts/bootstrap-native");
}
