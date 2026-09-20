//! Exercise the shipped command interface, not only its internal parser/helpers.
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
struct Cli {
    home: tempfile::TempDir,
    file: PathBuf,
}
impl Cli {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("settings/config.toml");
        Self { home, file }
    }
    fn run(&self, args: &[&str], success: bool) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_decklock"))
            .args(args)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env("XDG_DATA_HOME", self.home.path().join("data"))
            .env("LANG", "C.UTF-8")
            .env("TZ", "UTC")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > end {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("CLI hung: {args:?}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn config(&self, args: &[&str], success: bool) -> Output {
        let mut full = vec!["--config", self.file.to_str().unwrap(), "config"];
        full.extend_from_slice(args);
        self.run(&full, success)
    }
}
#[test]
fn help_and_read_commands_never_open_a_display_or_write_configuration() {
    let cli = Cli::new();
    for args in [
        vec![],
        vec!["--help"],
        vec!["help"],
        vec!["config", "--help"],
        vec!["setup", "--help"],
    ] {
        let output = cli.run(&args, true);
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("Usage:"));
        if args == ["--help"] {
            assert!(text.contains("--greeter"));
            assert!(text.contains("setup"));
        }
    }
    cli.config(&["show"], true);
    cli.config(&["path"], true);
    assert!(!cli.file.exists());
    assert!(!cli.home.path().join("config").exists());
}
#[test]
fn documented_options_round_trip_in_the_real_binary() {
    let cli = Cli::new();
    for (key, value) in [
        ("pam_service", "preserve-me"),
        ("theme_preset", "catppuccin-mocha"),
        ("locale", "pt-BR"),
        ("idle_seconds", "120"),
        ("idle_enabled", "false"),
        ("idle_reuse_background", "true"),
        ("slideshow_seconds", "15"),
        ("idle_slideshow_seconds", "20"),
        ("system_keyboard", "false"),
        ("layout.padding", "48"),
        ("layout.spacing", "24"),
        ("layout.keyboard_scale", "1.25"),
        ("layout.alignment", "start"),
        ("layout.arrangement", "horizontal"),
        ("layout.clock_visible", "false"),
        ("layout.idle_clock_visible", "true"),
        ("layout.avatar_visible", "false"),
        ("background_pool", "[\"photo.jpg\", \"video.mp4\"]"),
        ("idle_pool", "[]"),
        ("switch_user_command", "\"loginctl activate 2\""),
    ] {
        cli.config(&["set", key, value], true);
        cli.config(&["get", key], true);
    }
    cli.run(
        &[
            "config",
            "set",
            "idle_enabled",
            "true",
            "--config",
            cli.file.to_str().unwrap(),
        ],
        true,
    );
    let config = decklock::config::Config::load(Some(&cli.file)).unwrap();
    assert_eq!(config.pam_service, "preserve-me");
    assert_eq!(
        config.switch_user_command.as_deref(),
        Some("loginctl activate 2")
    );
    assert!(config.idle_enabled);
    assert_eq!(config.layout.unwrap().padding, 48);
    assert_eq!(
        config.background_pool.unwrap()[0],
        cli.file.parent().unwrap().join("photo.jpg")
    );
    cli.config(&["unset", "layout"], true);
    assert!(
        decklock::config::Config::load(Some(&cli.file))
            .unwrap()
            .layout
            .is_none()
    );
}
#[test]
fn invalid_commands_leave_existing_bytes_unchanged() {
    let cli = Cli::new();
    cli.config(&["set", "pam_service", "keep-me"], true);
    let before = std::fs::read(&cli.file).unwrap();
    for (key, value) in [
        ("idle_seconds", "0"),
        ("idle_seconds", "-1"),
        ("layout.padding", "257"),
        ("idle_enabled", "maybe"),
        ("theme_preset", "not-a-theme"),
        ("layout.typo", "1"),
        ("pam_service", "/etc/shadow"),
        ("background_pool", "42"),
        ("switch_user_command", "\"   \""),
        ("unknown", "true"),
    ] {
        cli.config(&["set", key, value], false);
        assert_eq!(
            std::fs::read(&cli.file).unwrap(),
            before,
            "Invalid write: {key}"
        );
    }
    cli.run(&["--lock", "config", "show"], false);
    cli.run(&["--settings", "config", "show"], false);
    assert_eq!(std::fs::read(&cli.file).unwrap(), before);
}
#[test]
fn importing_relative_media_resolves_against_source_and_preserves_them() {
    let cli = Cli::new();
    let source = cli.home.path().join("import/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(
        &source,
        "background_pool = ['picture.jpg']\npam_service='keep-me'\n",
    )
    .unwrap();
    cli.config(&["import", source.to_str().unwrap()], true);
    let config = decklock::config::Config::load(Some(&cli.file)).unwrap();
    assert_eq!(
        config.background_pool.unwrap(),
        vec![source.parent().unwrap().join(Path::new("picture.jpg"))]
    );
    assert_eq!(config.pam_service, "keep-me");
}

#[test]
fn procedural_media_options_roundtrip_and_reject_invalid_writes() {
    let cli = Cli::new();
    for (key, value) in [
        ("background_pool", "['procedural:starfield']"),
        ("procedurals.starfield.color", "#abcdef"),
        ("procedurals.starfield.speed", "0.5"),
        ("procedurals.starfield.seed", "4294967295"),
        ("idle_pool", "['procedural:lissajous']"),
    ] {
        cli.config(&["set", key, value], true);
    }
    let output = cli.config(&["get", "background_pool"], true);
    assert!(String::from_utf8_lossy(&output.stdout).contains("procedural:starfield"));
    let saved = std::fs::read(&cli.file).unwrap();
    for (key, value) in [
        ("procedurals.starfield.fps", "60"),
        ("procedurals.starfield.speed", "nan"),
        ("procedurals.starfield.color", "bad"),
        ("background_pool", "['procedural:unknown']"),
    ] {
        cli.config(&["set", key, value], false);
        assert_eq!(std::fs::read(&cli.file).unwrap(), saved);
    }
    cli.config(&["unset", "procedurals"], true);
    assert_eq!(
        String::from_utf8_lossy(
            &cli.config(&["get", "procedurals.starfield.speed"], true)
                .stdout
        )
        .trim(),
        "1.0"
    );
}

#[test]
fn every_procedural_starts_at_unit_speed() {
    let cli = Cli::new();
    for id in decklock::procedural::ITEMS {
        let key = format!("procedurals.{id}.speed");
        assert_eq!(
            String::from_utf8_lossy(&cli.config(&["get", &key], true).stdout).trim(),
            "1.0",
            "{id}"
        );
    }
}

#[test]
fn window_decorations_roundtrip_and_reject_non_booleans() {
    let cli = Cli::new();
    assert_eq!(
        String::from_utf8_lossy(&cli.config(&["get", "window_decorations"], true).stdout).trim(),
        "true"
    );
    cli.config(&["set", "window_decorations", "false"], true);
    assert_eq!(
        String::from_utf8_lossy(&cli.config(&["get", "window_decorations"], true).stdout).trim(),
        "false"
    );
    let before = std::fs::read(&cli.file).unwrap();
    cli.config(&["set", "window_decorations", "invalid"], false);
    assert_eq!(std::fs::read(&cli.file).unwrap(), before);
    cli.config(&["unset", "window_decorations"], true);
    assert_eq!(
        String::from_utf8_lossy(&cli.config(&["get", "window_decorations"], true).stdout).trim(),
        "true"
    );
}

#[test]
fn locking_refuses_and_names_the_file_when_the_configured_pam_service_is_absent() {
    let cli = Cli::new();
    cli.config(&["set", "pam_service", "decklock-absent-fixture"], true);
    let output = cli.run(&["--config", cli.file.to_str().unwrap(), "--lock"], false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // A locker that cannot authenticate would trap the user behind its own lock
    // screen, so an absent service is refused before anything is locked.
    assert!(
        stderr.contains("/etc/pam.d/decklock-absent-fixture"),
        "refusal must name the missing service file: {stderr}"
    );
    // The configuration error the user can fix is reported ahead of the
    // environment, which also keeps this assertion reachable with no compositor
    // and no display present.
    // Read from the catalog rather than repeating the wording here, so rewording
    // the refusal cannot quietly turn this into an assertion that always passes.
    let unsupported = include_str!("../locales/en-US.ftl")
        .lines()
        .find_map(|line| line.strip_prefix("unsupported = "))
        .expect("the English catalog defines the unsupported-compositor refusal");
    assert!(
        !stderr.contains(unsupported),
        "the PAM preflight must run before the compositor gate: {stderr}"
    );
}

#[test]
fn setup_subcommands_exercise_cli_boundary_and_support_isolated_targets() {
    let cli = Cli::new();
    let status = cli.run(&["setup", "status"], true);
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("DeckLock System Integration Status:")
    );

    let target_greetd = cli.home.path().join("greetd-test.toml");
    let dry_run = cli.run(
        &[
            "setup",
            "greeter",
            "--dry-run",
            "--target",
            target_greetd.to_str().unwrap(),
        ],
        true,
    );
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("Dry-run: would write"));
    assert!(!target_greetd.exists());

    cli.run(
        &[
            "setup",
            "greeter",
            "--target",
            target_greetd.to_str().unwrap(),
        ],
        true,
    );
    assert!(target_greetd.exists());
    let content = std::fs::read_to_string(&target_greetd).unwrap();
    assert!(content.contains("cage -s -- decklock --greeter --keyboard"));

    let target_hypridle = cli.home.path().join("hypridle-test.conf");
    cli.run(
        &[
            "setup",
            "lock",
            "--target",
            target_hypridle.to_str().unwrap(),
        ],
        true,
    );
    assert!(target_hypridle.exists());
    let lock_content = std::fs::read_to_string(&target_hypridle).unwrap();
    assert!(lock_content.contains("lock_cmd = pidof decklock || decklock --lock"));
}
