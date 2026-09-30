//! System setup and configuration automation for greetd and compositor screen locks.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GreeterCompositor {
    Cage,
    Hyprland,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Action {
    /// Inspect the current system configuration for greetd and screen lock.
    Status,
    /// Configure /etc/greetd/config.toml to run DeckLock inside cage (requires root/sudo).
    Greeter {
        /// Alternative destination path (for dry-runs or custom configs).
        #[arg(long)]
        target: Option<PathBuf>,
        /// Do not enable the virtual keyboard by default in greeter mode.
        #[arg(long)]
        no_keyboard: bool,
        /// Where the greeter remembers the last user and session. Defaults to
        /// /var/lib/decklock-greeter, and is left alone when --target is given.
        #[arg(long)]
        state_dir: Option<PathBuf>,
        /// Compositor for login. Existing Hyprland greeters are preserved when omitted.
        #[arg(long, value_enum)]
        compositor: Option<GreeterCompositor>,
        /// Output to rotate in the Hyprland greeter (Steam Deck: eDP-1).
        #[arg(long, requires = "compositor")]
        output: Option<String>,
        /// Wayland output/input transform, 0..7 (Steam Deck OLED: 3).
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=7), requires = "output")]
        transform: Option<u8>,
        /// Dry-run mode: show what would be written without modifying files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Configure hypridle to lock using DeckLock.
    Lock {
        /// Alternative destination path for hypridle.conf.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Dry-run mode: show what would be written without modifying files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Configure both greetd login and hypridle screen lock.
    All {
        /// Dry-run mode: show what would be written without modifying files.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemReport {
    pub greetd_installed: bool,
    pub greetd_service_active: bool,
    pub greetd_config_path: PathBuf,
    pub greetd_uses_decklock: bool,
    pub current_greetd_cmd: Option<String>,
    pub cage_installed: bool,
    pub greeter_user_exists: bool,
    pub hypridle_installed: bool,
    pub hypridle_config_path: Option<PathBuf>,
    pub hypridle_uses_decklock: bool,
}

fn command_exists(name: &str) -> bool {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

pub fn inspect_system(
    greetd_override: Option<&Path>,
    hypridle_override: Option<&Path>,
) -> SystemReport {
    let greetd_installed = command_exists("greetd") || Path::new("/etc/greetd").is_dir();
    let greetd_service_active = ProcessCommand::new("systemctl")
        .args(["is-active", "--quiet", "greetd.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let greetd_config_path = greetd_override
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/greetd/config.toml"));

    let mut current_greetd_cmd = None;
    let mut greetd_uses_decklock = false;

    if let Ok(content) = fs::read_to_string(&greetd_config_path) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(cmd) = line.strip_prefix("command") {
                let clean = cmd.trim().trim_start_matches('=').trim().trim_matches('"');
                current_greetd_cmd = Some(clean.to_string());
                if clean.contains("decklock --greeter") || clean.contains("decklock --greetter") {
                    greetd_uses_decklock = true;
                }
            }
        }
    }

    let cage_installed = command_exists("cage");

    let greeter_user_exists = fs::read_to_string("/etc/passwd")
        .map(|s| s.lines().any(|line| line.starts_with("greeter:")))
        .unwrap_or(false);

    let hypridle_installed = command_exists("hypridle");

    let hypridle_config_path = hypridle_override.map(PathBuf::from).or_else(|| {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("hypr/hypridle.conf"))
    });

    let hypridle_uses_decklock = hypridle_config_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|c| c.contains("decklock --lock"))
        .unwrap_or(false);

    SystemReport {
        greetd_installed,
        greetd_service_active,
        greetd_config_path,
        greetd_uses_decklock,
        current_greetd_cmd,
        cage_installed,
        greeter_user_exists,
        hypridle_installed,
        hypridle_config_path,
        hypridle_uses_decklock,
    }
}

pub fn format_status(report: &SystemReport) -> String {
    let mut out = String::new();
    out.push_str("DeckLock System Integration Status:\n\n");

    out.push_str("Login (greetd):\n");
    out.push_str(&format!(
        "  greetd installed:       {}\n",
        if report.greetd_installed {
            "Yes"
        } else {
            "No (install greetd)"
        }
    ));
    out.push_str(&format!(
        "  greetd service:         {}\n",
        if report.greetd_service_active {
            "Active"
        } else {
            "Inactive or not running"
        }
    ));
    out.push_str(&format!(
        "  cage compositor:        {}\n",
        if report.cage_installed {
            "Installed"
        } else {
            "Not installed (recommended: sudo pacman -S cage)"
        }
    ));
    out.push_str(&format!(
        "  greeter user:           {}\n",
        if report.greeter_user_exists {
            "Present"
        } else {
            "Missing"
        }
    ));
    out.push_str(&format!(
        "  greetd configuration:   {}\n",
        report.greetd_config_path.display()
    ));
    out.push_str(&format!(
        "  current greeter:        {}\n",
        if report.greetd_uses_decklock {
            "DeckLock [Configured]".to_string()
        } else if let Some(ref cmd) = report.current_greetd_cmd {
            format!("{cmd} [Run: sudo decklock setup greeter]")
        } else {
            "None found [Run: sudo decklock setup greeter]".to_string()
        }
    ));

    out.push_str("\nScreen Lock (hypridle / Wayland):\n");
    out.push_str(&format!(
        "  hypridle installed:     {}\n",
        if report.hypridle_installed {
            "Yes"
        } else {
            "No"
        }
    ));
    if let Some(ref path) = report.hypridle_config_path {
        out.push_str(&format!("  hypridle configuration: {}\n", path.display()));
        out.push_str(&format!(
            "  lock command:           {}\n",
            if report.hypridle_uses_decklock {
                "decklock --lock [Configured]"
            } else {
                "Not using decklock [Run: decklock setup lock]"
            }
        ));
    } else {
        out.push_str("  hypridle configuration: Not found [Run: decklock setup lock]\n");
    }

    out
}

pub fn setup_greeter(
    target: Option<&Path>,
    no_keyboard: bool,
    dry_run: bool,
) -> Result<String, String> {
    let keyboard_flag = if no_keyboard { "" } else { " --keyboard" };
    write_greeter_config(
        target,
        &format!("cage -s -- decklock --greeter{keyboard_flag}"),
        dry_run,
    )
}

fn write_greeter_config(
    target: Option<&Path>,
    command: &str,
    dry_run: bool,
) -> Result<String, String> {
    let target_path = target
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/greetd/config.toml"));
    let previous = if target_path.exists() {
        fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read {}: {e}", target_path.display()))?
    } else {
        String::new()
    };
    let mut document: toml::Value = if previous.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str(&previous).map_err(|e| {
            format!(
                "Invalid greetd configuration {}: {e}",
                target_path.display()
            )
        })?
    };
    let root = document
        .as_table_mut()
        .ok_or_else(|| "greetd configuration must be a TOML table".to_string())?;
    let terminal = root
        .entry("terminal")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| "greetd terminal must be a TOML table".to_string())?;
    terminal
        .entry("vt")
        .or_insert_with(|| toml::Value::Integer(1));
    let session = root
        .entry("default_session")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| "greetd default_session must be a TOML table".to_string())?;
    // greetd runs this as `sh -c "exec <command>"`, where a leading VAR=value
    // is taken as the program name. pam_systemd supplies XDG_RUNTIME_DIR.
    session.insert("command".into(), toml::Value::String(command.to_string()));
    session.insert("user".into(), toml::Value::String(GREETER_USER.into()));
    let content = toml::to_string_pretty(&document)
        .map_err(|e| format!("Failed to serialize greetd configuration: {e}"))?;

    if dry_run {
        return Ok(format!(
            "Dry-run: would write to {}\n---\n{}",
            target_path.display(),
            content
        ));
    }

    if previous == content {
        return Ok(format!(
            "greetd at {} is already configured",
            target_path.display()
        ));
    }

    if target_path.exists() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backup_path = target_path.with_extension(format!("toml.bak.{timestamp}"));
        fs::copy(&target_path, &backup_path).map_err(|e| {
            format!(
                "Failed to backup existing config to {}: {e}",
                backup_path.display()
            )
        })?;
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let parent = target_path
        .parent()
        .ok_or_else(|| "greetd configuration has no parent directory".to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Failed to create temporary greetd configuration: {e}"))?;
    use std::io::Write;
    temporary
        .write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write temporary greetd configuration: {e}"))?;
    // The temporary file is private. greetd's config is a normal readable system file,
    // and an administrator's own mode must survive the replacement.
    crate::config::keep_mode(&temporary, &target_path, Some(0o644))
        .map_err(|e| format!("Failed to set the mode of {}: {e}", target_path.display()))?;
    temporary
        .persist(&target_path)
        .map_err(|e| format!("Failed to replace {}: {e}", target_path.display()))?;

    Ok(format!(
        "Successfully configured greetd at {}",
        target_path.display()
    ))
}

pub fn setup_lock(target: Option<&Path>, dry_run: bool) -> Result<String, String> {
    let default_path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|p| p.join("hypr/hypridle.conf"));

    let target_path = target
        .map(PathBuf::from)
        .or(default_path)
        .ok_or_else(|| "Could not determine hypridle.conf path".to_string())?;

    let (content_to_write, was_existing) = if target_path.exists() {
        let content = fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read {}: {e}", target_path.display()))?;
        let mut updated_lines = Vec::new();
        let mut has_lock_cmd = false;
        for line in content.lines() {
            if let Some(pos) = line.find("lock_cmd") {
                let after = &line[pos + "lock_cmd".len()..];
                let after_trimmed = after.trim_start();
                if after_trimmed.starts_with('=') {
                    has_lock_cmd = true;
                    let indent = &line[..pos];
                    let suffix = if line.ends_with('}') { " }" } else { "" };
                    updated_lines.push(format!("{indent}lock_cmd = decklock --lock{suffix}"));
                    continue;
                }
            }
            if let Some(pos) = line.find("before_sleep_cmd") {
                let after = &line[pos + "before_sleep_cmd".len()..];
                let after_trimmed = after.trim_start();
                if after_trimmed.starts_with('=') {
                    let indent = &line[..pos];
                    let suffix = if line.ends_with('}') { " }" } else { "" };
                    updated_lines.push(format!(
                        "{indent}before_sleep_cmd = decklock --lock{suffix}"
                    ));
                    continue;
                }
            }
            updated_lines.push(line.to_string());
        }
        let mut edited = updated_lines.join("\n");
        if content.ends_with('\n') {
            edited.push('\n');
        }
        let updated = if has_lock_cmd {
            edited
        } else if let Some(start) = edited.find("general {") {
            let close = edited[start..]
                .find('}')
                .map(|offset| start + offset)
                .ok_or_else(|| "hypridle general block is not closed".to_string())?;
            let before_sleep = if edited.contains("before_sleep_cmd") {
                String::new()
            } else {
                "    before_sleep_cmd = decklock --lock\n".to_string()
            };
            edited.insert_str(
                close,
                &format!("    lock_cmd = decklock --lock\n{before_sleep}"),
            );
            edited
        } else {
            format!(
                "# Added by decklock setup\n\
                 general {{\n\
                     lock_cmd = decklock --lock\n\
                     before_sleep_cmd = decklock --lock\n\
                     after_sleep_cmd = hyprctl dispatch dpms on\n\
                     inhibit_sleep = 3\n\
                 }}\n\n{}",
                content
            )
        };
        if updated == content {
            return Ok(format!(
                "Screen lock in {} is already configured for DeckLock",
                target_path.display()
            ));
        }
        (updated, true)
    } else {
        let template = include_str!("../packaging/setup/hypridle.conf").to_string();
        (template, false)
    };

    if dry_run {
        let action_desc = if was_existing {
            "would update screen lock in"
        } else {
            "would create screen lock config in"
        };
        return Ok(format!(
            "Dry-run: {action_desc} {}\n---\n{}",
            target_path.display(),
            content_to_write
        ));
    }

    if was_existing {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup_path = target_path.with_extension(format!("conf.bak.{timestamp}"));
        let _ = fs::copy(&target_path, &backup_path);
    } else if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    fs::write(&target_path, content_to_write)
        .map_err(|e| format!("Failed to write {}: {e}", target_path.display()))?;

    Ok(format!(
        "Successfully configured hypridle at {}",
        target_path.display()
    ))
}

const GREETER_USER: &str = "greeter";

/// The directory the greeter remembers its choice in: the one asked for, else the
/// system one, but never for a custom `--target`, which is not the system greetd.
fn greeter_state_dir(target: Option<&Path>, explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| {
        target
            .is_none()
            .then(|| PathBuf::from(crate::greeter::SYSTEM_STATE_DIR))
    })
}

/// User and group ids of `user` in an /etc/passwd-formatted text.
fn passwd_ids(passwd: &str, user: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == user).then_some(())?;
        let uid = fields.nth(1)?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        Some((uid, gid))
    })
}

/// Prepare private greeter storage before replacing greetd's working config.
/// Custom targets are isolated fixtures owned by the caller; system installs
/// require the real greeter account and fail before changing the login command.
fn prepare_greeter_dirs(dir: &Path, system: bool, dry_run: bool) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    if !dir.is_absolute() {
        return Err("Greeter storage directory must be an absolute path".into());
    }
    if dry_run {
        return Ok(format!(
            "Dry-run: would prepare {} for the '{GREETER_USER}' account (mode 0750)",
            dir.display()
        ));
    }
    let owner = if system {
        Some(
            fs::read_to_string("/etc/passwd")
                .ok()
                .and_then(|passwd| passwd_ids(&passwd, GREETER_USER))
                .ok_or("The 'greeter' account is missing; install/configure greetd before setup")?,
        )
    } else {
        None
    };
    for path in [dir.to_path_buf(), dir.join("cache"), dir.join("data")] {
        let fail = |e| {
            format!(
                "Failed to prepare {}: {e}; greetd configuration was not changed",
                path.display()
            )
        };
        fs::create_dir_all(&path).map_err(fail)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).map_err(fail)?;
        if let Some((uid, gid)) = owner {
            std::os::unix::fs::chown(&path, Some(uid), Some(gid)).map_err(fail)?;
        }
    }
    Ok(format!(
        "Greeter directory {} is ready (mode 0750)",
        dir.display()
    ))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn greeter_command(no_keyboard: bool, dir: Option<&Path>) -> String {
    let keyboard = if no_keyboard { "" } else { " --keyboard" };
    let command = format!("cage -s -- decklock --greeter{keyboard}");
    with_greeter_storage(&command, dir)
}

fn with_greeter_storage(command: &str, dir: Option<&Path>) -> String {
    match dir {
        None => command.to_string(),
        Some(dir) => format!(
            "env XDG_CONFIG_HOME={} XDG_CACHE_HOME={} XDG_DATA_HOME={} DECKLOCK_GREETER_STATE={} {command}",
            shell_quote(&dir.to_string_lossy()),
            shell_quote(&dir.join("cache").to_string_lossy()),
            shell_quote(&dir.join("data").to_string_lossy()),
            shell_quote(&dir.join("greeter-state.toml").to_string_lossy())
        ),
    }
}

fn setup_greeter_with_storage(
    target: Option<&Path>,
    no_keyboard: bool,
    state_dir: Option<PathBuf>,
    dry_run: bool,
    compositor: Option<GreeterCompositor>,
    output: Option<&str>,
    transform: Option<u8>,
) -> Result<String, String> {
    if output.is_some() && compositor != Some(GreeterCompositor::Hyprland) {
        return Err("Output rotation requires --compositor hyprland".into());
    }
    if output.is_some_and(|output| {
        output.is_empty()
            || !output
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
    }) {
        return Err("Output must be a connector name, such as eDP-1".into());
    }
    let dir = greeter_state_dir(target, state_dir);
    let path = target.unwrap_or_else(|| Path::new("/etc/greetd/config.toml"));
    let previous = fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|doc| {
            doc.get("default_session")?
                .get("command")?
                .as_str()
                .map(str::to_string)
        })
        .filter(|command| command.contains("start-hyprland") && command.contains("greeter"));
    let lua_path = path.with_file_name("hyprland-greeter.lua");
    let lua = (compositor == Some(GreeterCompositor::Hyprland))
        .then(|| hyprland_greeter_config(no_keyboard, output, transform.unwrap_or(0)));
    let command = if lua.is_some() {
        with_greeter_storage(
            &format!(
                "start-hyprland -- -c {}",
                shell_quote(&lua_path.to_string_lossy())
            ),
            dir.as_deref(),
        )
    } else if compositor.is_none() && previous.is_some() {
        previous.unwrap()
    } else {
        greeter_command(no_keyboard, dir.as_deref())
    };
    // Parse/validate existing greetd policy before touching any saved files.
    let _ = write_greeter_config(target, &command, true)?;
    if let Some(lua) = &lua {
        if dry_run {
            return Ok(format!(
                "{}\nDry-run: would write {}\n{lua}",
                write_greeter_config(target, &command, true)?,
                lua_path.display()
            ));
        }
        verify_hyprland_config(lua)?;
    }
    let preparation = match &dir {
        Some(dir) => prepare_greeter_dirs(dir, target.is_none(), dry_run)?,
        None => String::new(),
    };
    if let Some(lua) = lua {
        save_greeter_lua(&lua_path, &lua)?;
    }
    let report = write_greeter_config(target, &command, dry_run)?;
    Ok(format!(
        "{report}\n{preparation}\nLogin changes take effect at the next logout or boot; greetd was not restarted."
    ))
}

fn hyprland_greeter_config(no_keyboard: bool, output: Option<&str>, transform: u8) -> String {
    // JSON strings are also Lua quoted strings. User-supplied connector names
    // must remain values, never executable Lua or shell fragments.
    let monitor = output.map(|output| format!(
        "hl.monitor({{ output = {}, mode = \"preferred\", position = \"0x0\", scale = 1, transform = {transform} }})\n",
        serde_json::to_string(output).unwrap())).unwrap_or_default();
    let touch = output
        .map(|output| {
            format!(
                "input = {{ touchdevice = {{ output = {}, transform = {transform} }} }},",
                serde_json::to_string(output).unwrap()
            )
        })
        .unwrap_or_default();
    let keyboard = if no_keyboard { "" } else { " --keyboard" };
    format!(
        "-- DeckLock login compositor; no desktop services or user configuration.\n{monitor}\nhl.monitor({{ output = \"\", mode = \"preferred\", position = \"auto\", scale = 1 }})\nhl.config({{\n general = {{ gaps_in = 0, gaps_out = 0, border_size = 0 }},\n {touch}\n misc = {{ disable_hyprland_logo = true, disable_splash_rendering = true, force_default_wallpaper = 0, disable_autoreload = true }},\n ecosystem = {{ no_update_news = true, no_donation_nag = true }},\n animations = {{ enabled = false }},\n}})\nhl.on(\"hyprland.start\", function()\n hl.exec_cmd(\"decklock --greeter{keyboard}; hyprctl dispatch 'hl.dsp.exit()'\")\nend)\n"
    )
}

fn verify_hyprland_config(lua: &str) -> Result<(), String> {
    use std::io::Write;
    if !command_exists("start-hyprland") || !command_exists("hyprctl") {
        return Err("Hyprland setup needs start-hyprland and hyprctl installed".into());
    }
    let mut file = tempfile::Builder::new()
        .suffix(".lua")
        .tempfile()
        .map_err(|e| e.to_string())?;
    file.write_all(lua.as_bytes()).map_err(|e| e.to_string())?;
    // Configuration verification cannot block installation indefinitely. Keep
    // output in a temporary file so a verbose verifier cannot fill a pipe.
    let log = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut child = ProcessCommand::new("Hyprland")
        .args(["--verify-config", "-c"])
        .arg(file.path())
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .stdout(log.reopen().map_err(|e| e.to_string())?)
        .stderr(log.reopen().map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| format!("Cannot verify Hyprland Lua configuration: {e}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Hyprland configuration verification timed out; greetd was not changed".into(),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if !status.success() {
        return Err(format!(
            "Hyprland rejected the generated Lua configuration; greetd was not changed: {}",
            fs::read_to_string(log.path()).unwrap_or_default()
        ));
    }
    Ok(())
}

fn save_greeter_lua(path: &Path, content: &str) -> Result<(), String> {
    use std::io::Write;
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or("Greeter compositor configuration needs a parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    crate::config::keep_mode(&file, path, Some(0o644)).map_err(|e| e.to_string())?;
    if path.exists() {
        let backup = tempfile::Builder::new()
            .prefix("hyprland-greeter.lua.bak.")
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?;
        fs::copy(path, backup.path()).map_err(|e| e.to_string())?;
        backup.keep().map_err(|e| e.to_string())?;
    }
    file.persist(path)
        .map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
    Ok(())
}

pub fn execute(action: Option<Action>) -> Result<String, String> {
    match action.unwrap_or(Action::Status) {
        Action::Status => {
            let report = inspect_system(None, None);
            Ok(format_status(&report))
        }
        Action::Greeter {
            target,
            no_keyboard,
            state_dir,
            compositor,
            output,
            transform,
            dry_run,
        } => setup_greeter_with_storage(
            target.as_deref(),
            no_keyboard,
            state_dir,
            dry_run,
            compositor,
            output.as_deref(),
            transform,
        ),
        Action::Lock { target, dry_run } => setup_lock(target.as_deref(), dry_run),
        Action::All { dry_run } => {
            let greeter_res =
                setup_greeter_with_storage(None, false, None, dry_run, None, None, None)?;
            let lock_res = setup_lock(None, dry_run)?;
            Ok(format!("{greeter_res}\n{lock_res}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dry_run_greeter_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let greetd_path = dir.path().join("greetd.toml");
        let hypridle_path = dir.path().join("hypridle.conf");

        let out = setup_greeter(Some(&greetd_path), false, true).unwrap();
        assert!(out.contains("Dry-run: would write"));
        assert!(out.contains("cage -s -- decklock --greeter --keyboard"));
        assert!(!greetd_path.exists());

        let out = setup_lock(Some(&hypridle_path), true).unwrap();
        assert!(out.contains("Dry-run: would"));
        assert!(out.contains("lock_cmd = decklock --lock"));
        assert!(!hypridle_path.exists());
    }

    #[test]
    fn test_execute_greeter_setup_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let greetd_path = dir.path().join("config.toml");
        fs::write(&greetd_path, "old_config = true\n").unwrap();

        let out = setup_greeter(Some(&greetd_path), false, false).unwrap();
        assert!(out.contains("Successfully configured greetd"));

        let content = fs::read_to_string(&greetd_path).unwrap();
        assert!(content.contains("cage -s -- decklock --greeter --keyboard"));
        assert!(content.contains("user = \"greeter\""));

        // Verify backup was created
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("config.toml.bak"))
            .collect();
        assert_eq!(backups.len(), 1);
        let backup_content = fs::read_to_string(backups[0].path()).unwrap();
        assert_eq!(backup_content, "old_config = true\n");
    }

    #[test]
    fn test_execute_hypridle_setup() {
        let dir = tempfile::tempdir().unwrap();
        let hypridle_path = dir.path().join("hypridle.conf");

        // Fresh creation from template
        let out = setup_lock(Some(&hypridle_path), false).unwrap();
        assert!(out.contains("Successfully configured hypridle"));
        let content = fs::read_to_string(&hypridle_path).unwrap();
        assert!(content.contains("lock_cmd = decklock --lock"));

        // Replacing existing locker (e.g. gtklock)
        fs::write(&hypridle_path, "general { lock_cmd = gtklock }\n").unwrap();
        let out = setup_lock(Some(&hypridle_path), false).unwrap();
        assert!(out.contains("Successfully configured hypridle"));
        let content = fs::read_to_string(&hypridle_path).unwrap();
        assert!(content.contains("lock_cmd = decklock --lock"));
    }

    #[test]
    fn test_inspect_system_report() {
        let dir = tempfile::tempdir().unwrap();
        let greetd_path = dir.path().join("config.toml");
        let hypridle_path = dir.path().join("hypridle.conf");

        fs::write(
            &greetd_path,
            "[default_session]\ncommand = \"cage -s -- decklock --greeter\"\n",
        )
        .unwrap();
        fs::write(&hypridle_path, "general { lock_cmd = decklock --lock }\n").unwrap();

        let report = inspect_system(Some(&greetd_path), Some(&hypridle_path));
        assert!(report.greetd_uses_decklock);
        assert!(report.hypridle_uses_decklock);

        let formatted = format_status(&report);
        assert!(formatted.contains("DeckLock [Configured]"));
        assert!(formatted.contains("decklock --lock [Configured]"));
    }

    #[test]
    fn passwd_ids_reads_the_uid_and_gid_of_the_named_account_only() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      greeter:x:968:967:greetd greeter user:/:/bin/bash\n\
                      greeter2:x:1:2::/:/bin/false\n\
                      broken\n";
        assert_eq!(passwd_ids(passwd, "greeter"), Some((968, 967)));
        assert_eq!(passwd_ids(passwd, "greeter2"), Some((1, 2)));
        assert_eq!(passwd_ids(passwd, "nobody"), None);
        assert_eq!(passwd_ids("greeter:x:abc:1::/:/bin/sh\n", "greeter"), None);
    }
}
