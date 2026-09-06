fn main() {
    pkg_config::Config::new()
        .atleast_version("1.3")
        .probe("gtk4-layer-shell-0")
        .expect("gtk4-layer-shell >= 1.3 is required; see scripts/bootstrap-native");
}
