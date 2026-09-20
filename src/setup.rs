//! System setup and configuration automation for greetd and compositor screen locks.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

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
    let target_path = target
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/greetd/config.toml"));

    let keyboard_flag = if no_keyboard { "" } else { " --keyboard" };
    let content = format!(
        "[terminal]\n\
         # The VT to run the greeter on.\n\
         vt = 1\n\n\
         [default_session]\n\
         command = \"cage -s -- decklock --greeter{keyboard_flag}\"\n\
         user = \"greeter\"\n"
    );

    if dry_run {
        return Ok(format!(
            "Dry-run: would write to {}\n---\n{}",
            target_path.display(),
            content
        ));
    }

    if target_path.exists() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
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

    fs::write(&target_path, content).map_err(|e| {
        format!(
            "Failed to write {}: {e}. (Make sure to run with sudo or appropriate permissions)",
            target_path.display()
        )
    })?;

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

    if dry_run {
        return Ok(format!(
            "Dry-run: would configure screen lock in {}",
            target_path.display()
        ));
    }

    if target_path.exists() {
        let content = fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read {}: {e}", target_path.display()))?;
        if content.contains("decklock --lock") {
            return Ok(format!(
                "Screen lock in {} is already configured for DeckLock",
                target_path.display()
            ));
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup_path = target_path.with_extension(format!("conf.bak.{timestamp}"));
        let _ = fs::copy(&target_path, &backup_path);

        let updated = if content.contains("gtklock") {
            content.replace("gtklock", "decklock --lock")
        } else {
            format!(
                "# Added by decklock setup\n\
                 general {{\n\
                     lock_cmd = pidof decklock || decklock --lock\n\
                     before_sleep_cmd = pidof decklock || decklock --lock\n\
                     after_sleep_cmd = hyprctl dispatch dpms on\n\
                     inhibit_sleep = 3\n\
                 }}\n\n{}",
                content
            )
        };
        fs::write(&target_path, updated)
            .map_err(|e| format!("Failed to write {}: {e}", target_path.display()))?;
    } else {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
        }
        let template = include_str!("../packaging/setup/hypridle.conf");
        fs::write(&target_path, template)
            .map_err(|e| format!("Failed to write {}: {e}", target_path.display()))?;
    }

    Ok(format!(
        "Successfully configured hypridle at {}",
        target_path.display()
    ))
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
            dry_run,
        } => setup_greeter(target.as_deref(), no_keyboard, dry_run),
        Action::Lock { target, dry_run } => setup_lock(target.as_deref(), dry_run),
        Action::All { dry_run } => {
            let greeter_res = setup_greeter(None, false, dry_run)?;
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
        assert!(out.contains("Dry-run: would configure"));
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
        assert!(content.contains("lock_cmd = pidof decklock || decklock --lock"));

        // Replacing existing locker (e.g. gtklock)
        fs::write(&hypridle_path, "general { lock_cmd = gtklock }\n").unwrap();
        let out = setup_lock(Some(&hypridle_path), false).unwrap();
        assert!(out.contains("Successfully configured hypridle"));
        let content = fs::read_to_string(&hypridle_path).unwrap();
        assert!(content.contains("general { lock_cmd = decklock --lock }"));
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
}
