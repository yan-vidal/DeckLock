//! Share media as the desktop user; install private login appearance metadata.
use crate::config::{Config, Theme};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn selected(config: &Config, theme: &Theme, idle: bool) -> Result<Vec<PathBuf>, String> {
    let pool = if idle {
        &config.idle_pool
    } else {
        &config.background_pool
    };
    let background = if idle {
        config.idle_background.as_ref()
    } else {
        config.background.as_ref().or(theme.background.as_ref())
    };
    // An explicit pool (including an empty one) overrides the legacy background.
    // Only selected inputs may prevent exporting the login appearance.
    let inputs: Vec<&PathBuf> = match pool {
        Some(paths) => paths.iter().collect(),
        None => background.into_iter().collect(),
    };
    for path in inputs {
        if crate::procedural::id(path).is_some() {
            continue;
        }
        let metadata =
            fs::metadata(path).map_err(|e| format!("Selected media {}: {e}", path.display()))?;
        if metadata.is_dir() {
            fs::read_dir(path).map_err(|e| format!("Selected media {}: {e}", path.display()))?;
        } else if metadata.is_file() {
            File::open(path).map_err(|e| format!("Selected media {}: {e}", path.display()))?;
        } else {
            return Err(format!(
                "Selected media {} is not a regular file or directory",
                path.display()
            ));
        }
    }
    Ok(crate::library::pool(config, theme, idle))
}

pub(super) fn export(
    source: Option<&Path>,
    target: &Path,
    dry_run: bool,
    shared: Option<&Path>,
    prefer_saved: bool,
) -> Result<String, String> {
    let default = crate::shortcut::config_dir().map(|p| p.join("decklock/config.toml"));
    let config = match source.or(default.as_deref()) {
        Some(path) => match fs::metadata(path) {
            Ok(_) => Config::load(Some(path))?,
            Err(error) if source.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
                Config::default()
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        },
        None => Config::default(),
    };
    let theme = Theme::from_config(&config)?;
    crate::i18n::I18n::new(config.locale.as_deref(), None)?;
    let proposed = shared.ok_or("Shared media directory is required")?;
    let chosen = if prefer_saved {
        config
            .media_library
            .clone()
            .unwrap_or_else(|| proposed.to_path_buf())
    } else {
        proposed.to_path_buf()
    };
    let shared = chosen.as_path();
    let mut normal = selected(&config, &theme, false)?;
    let mut idle = selected(&config, &theme, true)?;
    let mut all = normal.clone();
    all.extend(idle.clone());
    all.extend(crate::library::files(&crate::library::directory(&config)));
    if let Some(home) = crate::shortcut::config_dir() {
        for mode in ["bloqueio", "ocioso"] {
            all.extend(crate::library::files(&home.join("midias").join(mode)));
        }
    }
    all.sort();
    all.dedup();
    super::shared_media::preflight(&all, shared)?;
    let count = all
        .iter()
        .filter(|p| crate::procedural::id(p).is_none())
        .count();
    if dry_run {
        return Ok(serde_json::json!({"media_dir": shared, "count": count}).to_string());
    }
    let mut migrated = HashMap::new();
    for path in all {
        migrated.insert(path.clone(), super::shared_media::migrate(&path, shared)?);
    }
    for path in normal.iter_mut().chain(&mut idle) {
        *path = migrated
            .get(path)
            .ok_or("Selected media was not migrated")?
            .clone();
    }
    let appearance = Config {
        procedurals: config.procedurals.clone(),
        theme: config.theme.as_ref().map(|_| PathBuf::from(".")),
        theme_preset: config.theme_preset.clone(),
        locale: config.locale.clone(),
        idle_seconds: config.idle_seconds,
        idle_enabled: config.idle_enabled,
        idle_reuse_background: config.idle_reuse_background,
        background_pool: Some(normal.clone()),
        idle_pool: Some(idle.clone()),
        slideshow_seconds: config.slideshow_seconds,
        idle_slideshow_seconds: config.idle_slideshow_seconds,
        system_keyboard: config.system_keyboard,
        layout: config.layout.clone(),
        ..Config::default()
    };
    appearance.validate()?;
    if !dry_run {
        if appearance.theme.is_some() {
            let mut theme = theme;
            // The resolved theme background is already in the normal pool.
            theme.background = None;
            fs::write(target.join("theme.toml"), theme.document()?).map_err(|e| e.to_string())?;
            fs::write(target.join("style.css"), theme.css).map_err(|e| e.to_string())?;
        }
        fs::write(
            target.join("config.toml"),
            toml::to_string_pretty(&appearance).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    let mut user = config.clone();
    user.media_library = Some(shared.to_path_buf());
    user.background_pool = Some(normal);
    user.idle_pool = Some(idle);
    let destination = source
        .or(default.as_deref())
        .ok_or("Cannot locate user configuration")?;
    let previous = fs::read(destination).ok();
    let text = toml::to_string_pretty(&user).map_err(|e| e.to_string())?;
    if previous.as_deref() != Some(text.as_bytes()) {
        if let Some(previous) = previous {
            let parent = destination.parent().ok_or("Configuration needs a parent")?;
            let mut backup = tempfile::Builder::new()
                .prefix("config.toml.bak.")
                .tempfile_in(parent)
                .map_err(|e| e.to_string())?;
            backup.write_all(&previous).map_err(|e| e.to_string())?;
            backup.keep().map_err(|e| e.to_string())?;
        }
        user.save(destination)?;
    }
    Ok(format!(
        "Shared {count} media files; original paths remain as aliases; HOME permissions unchanged"
    ))
}

pub(super) enum Plan {
    Snapshot(tempfile::TempDir, u32, PathBuf),
    Disable,
}

pub(super) fn prepare(
    dir: Option<&Path>,
    system: bool,
    source: Option<&Path>,
    disabled: bool,
    dry_run: bool,
    media_dir: Option<&Path>,
) -> Result<(Option<Plan>, String), String> {
    if disabled {
        return Ok((
            (!dry_run && dir.is_some()).then_some(Plan::Disable),
            "User appearance sharing disabled".into(),
        ));
    }
    let caller = if unsafe { libc::geteuid() } == 0 {
        std::env::var("SUDO_UID")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .map(super::caller_by_uid)
            .transpose()?
    } else {
        None
    };
    if source.is_none() && (!system || caller.is_none()) {
        return Ok((None, String::new()));
    }
    if dir.is_none() {
        return Err("Sharing user appearance needs --state-dir with a custom --target".into());
    }
    if unsafe { libc::geteuid() } == 0 && caller.is_none() {
        return Err("Share user appearance through sudo from the desktop account".into());
    }
    let export_uid = caller
        .as_ref()
        .map_or_else(|| unsafe { libc::geteuid() }, |c| c.uid);
    let export_gid = caller
        .as_ref()
        .map_or_else(|| unsafe { libc::getegid() }, |c| c.gid);
    let mut shared = match media_dir {
        Some(path) => path.to_path_buf(),
        None if system => super::shared_media::default_library(
            &caller.as_ref().ok_or("Desktop caller is required")?.home,
            export_uid,
        )?,
        None => dir
            .unwrap()
            .with_file_name(format!("decklock-media-{export_uid}")),
    };
    if !shared.is_absolute() {
        return Err("Shared media directory must be absolute".into());
    }
    let staging = tempfile::Builder::new()
        .prefix("decklock-appearance-export-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    if let Some(caller) = &caller {
        std::os::unix::fs::chown(staging.path(), Some(caller.uid), Some(caller.gid))
            .map_err(|e| e.to_string())?;
    }
    let mut command = match caller {
        Some(caller) => caller.command()?,
        None => Command::new(std::env::current_exe().map_err(|e| e.to_string())?),
    };
    command
        .args(["setup", "export-appearance", "--target"])
        .arg(staging.path())
        .arg("--media-dir")
        .arg(&shared);
    if let Some(source) = source {
        command.arg("--source").arg(source);
    }
    let log = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let output = log.reopen().map_err(|e| e.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
    command.arg("--dry-run");
    if media_dir.is_none() {
        command.arg("--prefer-saved-library");
    }
    execute_export(&mut command, &log)?;
    let plan: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(log.path()).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cannot read media migration plan: {e}"))?;
    shared = PathBuf::from(
        plan.get("media_dir")
            .and_then(serde_json::Value::as_str)
            .ok_or("Media plan has no library path")?,
    );
    let count = plan
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Media plan has no file count")?;
    let report = format!(
        "{} {count} media files in {}; original paths remain as aliases, HOME permissions unchanged",
        if dry_run {
            "Dry-run: would share"
        } else {
            "Shared"
        },
        shared.display()
    );
    if dry_run {
        return Ok((None, report));
    }
    super::shared_media::prepare(&shared, export_uid, export_gid, system)?;
    // Rebuild without --dry-run, preserving the unprivileged caller environment.
    let mut actual = if unsafe { libc::geteuid() } == 0 && export_uid != 0 {
        super::caller_by_uid(export_uid)?.command()?
    } else {
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
    };
    actual
        .args(["setup", "export-appearance", "--target"])
        .arg(staging.path())
        .arg("--media-dir")
        .arg(&shared);
    if let Some(source) = source {
        actual.arg("--source").arg(source);
    }
    let output = log.reopen().map_err(|e| e.to_string())?;
    actual
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
    execute_export(&mut actual, &log)?;
    Ok((Some(Plan::Snapshot(staging, export_uid, shared)), report))
}

fn execute_export(command: &mut Command, log: &tempfile::NamedTempFile) -> Result<(), String> {
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(1800);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Media migration timed out; login configuration was not changed; source aliases and verified shared files are recoverable".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(format!(
            "Media export failed; login configuration was not changed:\n{}",
            fs::read_to_string(log.path()).map_err(|e| e.to_string())?
        ));
    }
    Ok(())
}

// The exporter is unprivileged. Treat its files as untrusted: flat regular
// files only, opened without following symlinks; never root-read source paths.
fn regular(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Appearance export must contain regular files only".into());
    }
    Ok(file)
}
fn owned(file: &File, owner: Option<(u32, u32)>) -> Result<(), String> {
    file.set_permissions(fs::Permissions::from_mode(0o640))
        .map_err(|e| e.to_string())?;
    if let Some((uid, gid)) = owner {
        use std::os::fd::AsRawFd;
        if unsafe { libc::fchown(file.as_raw_fd(), uid, gid) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    Ok(())
}
impl Plan {
    pub(super) fn install(self, dir: &Path, system: bool, user: &str) -> Result<(), String> {
        let owner = if system {
            Some(
                super::passwd_ids(
                    &fs::read_to_string("/etc/passwd").map_err(|e| e.to_string())?,
                    user,
                )
                .ok_or("Greeter service account missing")?,
            )
        } else {
            None
        };
        let marker = dir.join("appearance.disabled");
        let Self::Snapshot(staging, export_uid, shared) = self else {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker);
            match file {
                Ok(file) => owned(&file, owner)?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.to_string()),
            }
            return Ok(());
        };
        // Copy into a root-owned private directory before parsing or hashing.
        let snapshot = tempfile::Builder::new()
            .prefix(".appearance-")
            .tempdir_in(dir)
            .map_err(|e| e.to_string())?;
        // Anchor the user-owned staging directory by fd: replacing its path
        // with a symlink must never redirect privileged reads to another tree.
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(staging.path())
            .map_err(|e| e.to_string())?;
        if directory.metadata().map_err(|e| e.to_string())?.uid() != export_uid {
            return Err("Export directory owner changed".into());
        }
        let anchor = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let snapshot_directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(snapshot.path())
            .map_err(|e| e.to_string())?;
        let snapshot_anchor =
            PathBuf::from(format!("/proc/self/fd/{}", snapshot_directory.as_raw_fd()));
        let mut names = fs::read_dir(&anchor)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        names.sort();
        for name in &names {
            let mut input = regular(&anchor.join(name))?;
            let info = input.metadata().map_err(|e| e.to_string())?;
            if info.uid() != export_uid || info.nlink() != 1 {
                return Err("Export files must belong to the caller and have no hard links".into());
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(snapshot_anchor.join(name))
                .map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
            output.sync_all().map_err(|e| e.to_string())?;
            owned(&output, owner)?;
        }
        let mut appearance: Config = toml::from_str(
            &fs::read_to_string(snapshot_anchor.join("config.toml")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        appearance.validate()?;
        if appearance.background.is_some()
            || appearance.idle_background.is_some()
            || appearance
                .theme
                .as_deref()
                .is_some_and(|p| p != Path::new("."))
        {
            return Err("Invalid paths in appearance export".into());
        }
        for path in [&appearance.background_pool, &appearance.idle_pool]
            .into_iter()
            .flatten()
            .flatten()
        {
            if crate::procedural::id(path).is_none() && (!path.is_absolute() || !path.is_file()) {
                return Err("Appearance media must refer to existing shared files".into());
            }
        }
        for path in [&appearance.background_pool, &appearance.idle_pool]
            .into_iter()
            .flatten()
            .flatten()
        {
            if crate::procedural::id(path).is_some() {
                continue;
            }
            if let Some((uid, gid)) = owner {
                let mut probe = Command::new("setpriv");
                probe
                    .args([
                        "--reuid",
                        &uid.to_string(),
                        "--regid",
                        &gid.to_string(),
                        "--init-groups",
                        "--",
                        "/usr/bin/test",
                        "-r",
                    ])
                    .arg(path)
                    .arg("-a")
                    .arg("-f")
                    .arg(path);
                let mut child = probe.spawn().map_err(|e| e.to_string())?;
                let deadline = Instant::now() + Duration::from_secs(30);
                let status = loop {
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Login media access check timed out; login configuration was not changed".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                if !status.success() {
                    return Err(format!(
                        "The login account cannot read shared media {}; login configuration was not changed",
                        path.display()
                    ));
                }
            }
        }
        if system {
            super::shared_media::prepare(
                &shared,
                export_uid,
                super::caller_by_uid(export_uid)?.gid,
                true,
            )?;
        }
        if appearance.theme.is_some() {
            let document: toml::Value = toml::from_str(
                &fs::read_to_string(snapshot_anchor.join("theme.toml"))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if document.get("css").and_then(toml::Value::as_str) != Some("style.css")
                || document.get("background").is_some()
            {
                return Err("Invalid exported theme paths".into());
            }
            Theme::load(Some(&snapshot_anchor))?;
        }
        let mut checksum = glib::Checksum::new(glib::ChecksumType::Sha256).unwrap();
        let mut buffer = [0u8; 65536];
        for name in &names {
            use std::os::unix::ffi::OsStrExt;
            checksum.update(name.as_bytes());
            checksum.update(&[0]);
            let mut file = regular(&snapshot_anchor.join(name))?;
            loop {
                let length = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if length == 0 {
                    break;
                }
                checksum.update(&buffer[..length]);
            }
        }
        let destination = dir.join(format!("appearance-{}", checksum.string().unwrap()));
        if destination.exists() {
            let existing = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(&destination)
                .map_err(|e| format!("Cannot reuse installed appearance snapshot: {e}"))?;
            let existing_anchor = PathBuf::from(format!("/proc/self/fd/{}", existing.as_raw_fd()));
            let expected_uid = owner.map_or_else(|| unsafe { libc::geteuid() }, |(uid, _)| uid);
            let mut other = [0u8; 65536];
            for name in &names {
                let mut installed = regular(&existing_anchor.join(name))?;
                let info = installed.metadata().map_err(|e| e.to_string())?;
                if info.uid() != expected_uid || info.nlink() != 1 {
                    return Err("Installed snapshot ownership or links changed; login configuration was not changed".into());
                }
                let mut candidate = regular(&snapshot_anchor.join(name))?;
                loop {
                    let n = candidate.read(&mut buffer).map_err(|e| e.to_string())?;
                    let m = installed.read(&mut other).map_err(|e| e.to_string())?;
                    if n != m || buffer[..n] != other[..m] {
                        return Err("Installed snapshot contents changed; login configuration was not changed".into());
                    }
                    if n == 0 {
                        break;
                    }
                }
            }
        } else {
            fs::rename(snapshot.path(), &destination).map_err(|e| e.to_string())?;
            // Finish ownership through the held fd after the atomic rename.
            if let Some((uid, gid)) = owner
                && unsafe { libc::fchown(snapshot_directory.as_raw_fd(), uid, gid) } != 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            snapshot_directory
                .set_permissions(fs::Permissions::from_mode(0o750))
                .map_err(|e| e.to_string())?;
        }
        appearance.resolve_paths(&destination);
        let text = toml::to_string_pretty(&appearance).map_err(|e| e.to_string())?;
        let pointer = dir.join("appearance.toml");
        let existing = match regular(&pointer) {
            Ok(mut file) => {
                let mut text = String::new();
                file.read_to_string(&mut text).map_err(|e| e.to_string())?;
                Some(text)
            }
            Err(_) if !pointer.exists() => None,
            Err(error) => return Err(format!("Cannot inspect installed appearance: {error}")),
        };
        if existing.as_deref() != Some(&text) {
            let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            file.as_file().sync_all().map_err(|e| e.to_string())?;
            owned(file.as_file(), owner)?;
            file.persist(&pointer).map_err(|e| e.to_string())?;
        }
        match fs::remove_file(marker) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
        super::prepare_greeter_dirs(dir, system, false, user)?;
        super::shared_media::retire_snapshots(
            dir,
            &shared,
            owner.map_or(export_uid, |(uid, _)| uid),
            export_uid,
        )?;
        if system {
            super::shared_media::prepare(
                &shared,
                export_uid,
                super::caller_by_uid(export_uid)?.gid,
                true,
            )?;
        }
        Ok(())
    }
}
