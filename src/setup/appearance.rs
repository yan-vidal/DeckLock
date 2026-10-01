//! Export selected appearance as the desktop user; install private login copies.
use crate::config::{Config, Theme};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
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
    // Explicit missing inputs must not silently become an empty login pool.
    for path in pool.as_ref().into_iter().flatten().chain(background) {
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
    let mut copies = HashMap::<PathBuf, PathBuf>::new();
    let mut copy = |path: PathBuf| -> Result<PathBuf, String> {
        if crate::procedural::id(&path).is_some() {
            return Ok(path);
        }
        if let Some(name) = copies.get(&path) {
            return Ok(name.clone());
        }
        let extension = path
            .extension()
            .and_then(|p| p.to_str())
            .ok_or("Media extension missing")?;
        let name = PathBuf::from(format!("media-{:06}.{extension}", copies.len()));
        // A dry-run still opens inputs with the exporting user's privileges.
        let mut input =
            File::open(&path).map_err(|e| format!("Cannot export {}: {e}", path.display()))?;
        if !dry_run {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(target.join(&name))
                .map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        }
        copies.insert(path, name.clone());
        Ok(name)
    };
    let normal = selected(&config, &theme, false)?
        .into_iter()
        .map(&mut copy)
        .collect::<Result<Vec<_>, _>>()?;
    let idle = selected(&config, &theme, true)?
        .into_iter()
        .map(&mut copy)
        .collect::<Result<Vec<_>, _>>()?;
    let appearance = Config {
        procedurals: config.procedurals,
        theme: config.theme.as_ref().map(|_| PathBuf::from(".")),
        theme_preset: config.theme_preset,
        locale: config.locale,
        idle_seconds: config.idle_seconds,
        idle_enabled: config.idle_enabled,
        idle_reuse_background: config.idle_reuse_background,
        background_pool: Some(normal),
        idle_pool: Some(idle),
        slideshow_seconds: config.slideshow_seconds,
        idle_slideshow_seconds: config.idle_slideshow_seconds,
        system_keyboard: config.system_keyboard,
        layout: config.layout,
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
    Ok(format!(
        "{} {} selected media files for login; source files and HOME permissions unchanged",
        if dry_run {
            "Dry-run: would copy"
        } else {
            "Exported"
        },
        copies.len()
    ))
}

pub(super) enum Plan {
    Snapshot(tempfile::TempDir, u32),
    Disable,
}

pub(super) fn prepare(
    dir: Option<&Path>,
    system: bool,
    source: Option<&Path>,
    disabled: bool,
    dry_run: bool,
) -> Result<Option<Plan>, String> {
    if disabled {
        return Ok((!dry_run && dir.is_some()).then_some(Plan::Disable));
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
        return Ok(None);
    }
    if dir.is_none() {
        return Err("Copying user appearance needs --state-dir with a custom --target".into());
    }
    if unsafe { libc::geteuid() } == 0 && caller.is_none() {
        return Err("Copy user appearance through sudo from the desktop account".into());
    }
    let staging = tempfile::Builder::new()
        .prefix("decklock-appearance-export-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    if let Some(caller) = &caller {
        std::os::unix::fs::chown(staging.path(), Some(caller.uid), Some(caller.gid))
            .map_err(|e| e.to_string())?;
    }
    let export_uid = caller
        .as_ref()
        .map_or_else(|| unsafe { libc::geteuid() }, |c| c.uid);
    let mut command = match caller {
        Some(caller) => caller.command()?,
        None => Command::new(std::env::current_exe().map_err(|e| e.to_string())?),
    };
    command
        .args(["setup", "export-appearance", "--target"])
        .arg(staging.path());
    if let Some(source) = source {
        command.arg("--source").arg(source);
    }
    if dry_run {
        command.arg("--dry-run");
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
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Media export timed out; login configuration was not changed".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(format!(
            "Media export failed; login configuration was not changed:\n{}",
            fs::read_to_string(log.path()).map_err(|e| e.to_string())?
        ));
    }
    Ok((!dry_run).then_some(Plan::Snapshot(staging, export_uid)))
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
fn flat(path: &Path) -> bool {
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
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
        let Self::Snapshot(staging, export_uid) = self else {
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
            if crate::procedural::id(path).is_none()
                && (!flat(path) || !snapshot_anchor.join(path).is_file())
            {
                return Err("Appearance media must refer to exported flat files".into());
            }
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
        Ok(())
    }
}
