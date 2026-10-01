//! One user-writable, publicly readable library; never open the user's HOME to login.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

pub(super) fn default_library(home: &Path, uid: u32) -> Result<PathBuf, String> {
    let home = fs::canonicalize(home).map_err(|e| e.to_string())?;
    let parent = home.parent().ok_or("Desktop HOME needs a parent")?;
    let home_info = fs::metadata(&home).map_err(|e| e.to_string())?;
    let parent_info = fs::metadata(parent).map_err(|e| e.to_string())?;
    let system_info = fs::metadata("/var/lib").map_err(|e| e.to_string())?;
    if home_info.uid() != uid {
        return Err("Desktop HOME must belong to the sudo caller".into());
    }
    // A private home mount (pam_mount, for example) cannot supply pre-login media.
    // Prefer the common volume only when it also contains the public parent.
    if home_info.dev() != system_info.dev() && parent_info.dev() == home_info.dev() {
        return Ok(parent.join(".decklock-media").join(uid.to_string()));
    }
    Ok(PathBuf::from("/var/lib/decklock/media").join(uid.to_string()))
}

pub(super) fn directory(path: &Path, create: bool) -> Result<File, String> {
    if !path.is_absolute() {
        return Err("Shared media directory must be absolute".into());
    }
    let mut dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open("/")
        .map_err(|e| e.to_string())?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            if matches!(component, Component::RootDir) {
                continue;
            }
            return Err("Shared media directory cannot contain parent components".into());
        };
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
        let open = || unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        let mut fd = open();
        let mut created = false;
        if fd < 0
            && create
            && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound
        {
            if unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            fd = open();
            created = true;
        }
        if fd < 0 {
            return Err(format!(
                "Shared media directory {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        use std::os::fd::FromRawFd;
        dir = unsafe { File::from_raw_fd(fd) };
        if created {
            clear_acl(&dir, c"system.posix_acl_access")?;
            clear_acl(&dir, c"system.posix_acl_default")?;
            dir.set_permissions(fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(dir)
}

pub(super) fn prepare(path: &Path, uid: u32, gid: u32, system: bool) -> Result<(), String> {
    if system {
        let home = super::caller_by_uid(uid)?.home;
        if path.starts_with(&home) || home.starts_with(path) {
            return Err("Shared media must be outside the desktop HOME".into());
        }
        // Privileged labelling must operate only beneath root-controlled ancestors.
        for parent in path.ancestors().skip(1) {
            if let Ok(dir) = directory(parent, false) {
                let info = dir.metadata().map_err(|e| e.to_string())?;
                if info.uid() != 0 || info.mode() & 0o022 != 0 || info.mode() & 0o001 == 0 {
                    return Err(
                        "Shared media needs publicly traversable root-owned ancestors without group/other write access"
                            .into(),
                    );
                }
            }
        }
    }
    let existed = directory(path, false).is_ok();
    let dir = directory(path, true)?;
    let info = dir.metadata().map_err(|e| e.to_string())?;
    if info.uid() != uid && (existed || info.uid() != unsafe { libc::geteuid() }) {
        return Err("Shared media directory belongs to another account".into());
    }
    if unsafe { libc::geteuid() } == 0 && unsafe { libc::fchown(dir.as_raw_fd(), uid, gid) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    clear_acl(&dir, c"system.posix_acl_access")?;
    clear_acl(&dir, c"system.posix_acl_default")?;
    dir.set_permissions(fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    if system && Path::new("/sys/fs/selinux/enforce").exists() {
        label(path)?;
    }
    Ok(())
}

fn command(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "{program} failed; login configuration was not changed"
                ))
            };
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{program} timed out; login configuration was not changed"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn label(path: &Path) -> Result<(), String> {
    command("restorecon", &["-RF".as_ref(), path.as_os_str()])?;
    let dir = directory(path, false)?;
    let context = || {
        let mut bytes = [0u8; 1024];
        let size = unsafe {
            libc::fgetxattr(
                dir.as_raw_fd(),
                c"security.selinux".as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if size < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(String::from_utf8_lossy(&bytes[..size as usize]).into_owned())
    };
    if context()?.split(':').nth(2) != Some("xdm_var_lib_t") {
        // A persistent mapping survives relabels; never overwrite an administrator's mapping.
        let text = path.to_str().ok_or("SELinux media path must be UTF-8")?;
        let escaped: String = text
            .chars()
            .flat_map(|c| {
                if ".^$*+?()[]{}|\\".contains(c) {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect();
        let expression = format!("{escaped}(/.*)?");
        command(
            "semanage",
            &[
                "fcontext".as_ref(),
                "-a".as_ref(),
                "-t".as_ref(),
                "xdm_var_lib_t".as_ref(),
                expression.as_ref(),
            ],
        )?;
        command("restorecon", &["-RF".as_ref(), path.as_os_str()])?;
    }
    if context()?.split(':').nth(2) != Some("xdm_var_lib_t") {
        return Err("Shared media must have xdm_var_lib_t SELinux context; login configuration was not changed".into());
    }
    Ok(())
}

fn media(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("Media {}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Shared media must be regular files".into());
    }
    Ok(file)
}

fn clear_acl(file: &File, name: &std::ffi::CStr) -> Result<(), String> {
    let result = unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr()) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(libc::ENODATA | libc::EOPNOTSUPP)) {
            return Err(error.to_string());
        }
    }
    Ok(())
}
fn public(file: &File) -> Result<(), String> {
    // Named ACL entries can override "other" bits. Clear only the selected
    // public file's ACL, never a HOME or unrelated ancestor ACL.
    clear_acl(file, c"system.posix_acl_access")?;
    file.set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|e| e.to_string())
}

fn label_import(path: &Path, file: &File) -> Result<(), String> {
    if !Path::new("/sys/fs/selinux/enforce").exists() {
        return Ok(());
    }
    // A hard link keeps the source label. Normalize it as its owner, including
    // later GUI/CLI imports, rather than waiting for another privileged setup.
    command("restorecon", &["-F".as_ref(), path.as_os_str()])?;
    let mut bytes = [0u8; 1024];
    let length = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"security.selinux".as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if length < 0
        || String::from_utf8_lossy(&bytes[..length.max(0) as usize])
            .split(':')
            .nth(2)
            != Some("xdm_var_lib_t")
    {
        return Err("Shared media needs its login SELinux label; run setup before importing; original path was preserved".into());
    }
    Ok(())
}

pub(crate) fn preflight(paths: &[PathBuf], root: &Path) -> Result<(), String> {
    for path in paths {
        if crate::procedural::id(path).is_some() {
            continue;
        }
        let canonical = fs::canonicalize(path)
            .map_err(|e| format!("Selected media {}: {e}", path.display()))?;
        let mut file = media(&canonical)?;
        let info = file.metadata().map_err(|e| e.to_string())?;
        if info.uid() != unsafe { libc::geteuid() } {
            if info.mode() & 0o004 == 0
                || canonical
                    .ancestors()
                    .skip(1)
                    .any(|p| fs::metadata(p).map_or(true, |m| m.mode() & 0o001 == 0))
            {
                return Err("Another owner's media must already be publicly readable; it will not be copied or changed".into());
            }
            continue;
        }
        if canonical.starts_with(root) {
            continue;
        }
        if info.nlink() != 1 {
            let hash = checksum(&mut file)?;
            let recoverable = info.nlink() == 2
                && existing_content(root, &hash, info.uid())?.is_some_and(|p| {
                    fs::metadata(p).is_ok_and(|m| m.ino() == info.ino() && m.dev() == info.dev())
                });
            if !recoverable {
                return Err("Cannot migrate unrelated hard links; source was preserved".into());
            }
        }
        use std::os::unix::ffi::OsStrExt;
        let parent = std::ffi::CString::new(
            canonical
                .parent()
                .ok_or("Media needs a parent")?
                .as_os_str()
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        if unsafe {
            libc::faccessat(
                libc::AT_FDCWD,
                parent.as_ptr(),
                libc::W_OK | libc::X_OK,
                libc::AT_EACCESS,
            )
        } != 0
        {
            return Err(format!(
                "Cannot migrate {}: source directory is not writable",
                path.display()
            ));
        }
    }
    Ok(())
}

fn checksum(file: &mut File) -> Result<String, String> {
    let mut digest = glib::Checksum::new(glib::ChecksumType::Sha256).unwrap();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(digest.string().unwrap().to_string())
}

fn existing_content(root: &Path, hash: &str, uid: u32) -> Result<Option<PathBuf>, String> {
    if !root.exists() {
        return Ok(None);
    }
    let directory = directory(root, false)?;
    let anchor = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    let prefix = format!("{hash}-");
    let mut names = fs::read_dir(&anchor)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    names.sort();
    for name in names {
        if !name.to_string_lossy().starts_with(&prefix) {
            continue;
        }
        if fs::symlink_metadata(anchor.join(&name))
            .map_err(|e| e.to_string())?
            .is_symlink()
        {
            continue;
        }
        let mut candidate = media(&anchor.join(&name))?;
        if candidate.metadata().map_err(|e| e.to_string())?.uid() == uid
            && checksum(&mut candidate)? == hash
        {
            return Ok(Some(root.join(name)));
        }
    }
    Ok(None)
}

/// Same filesystem: a temporary hard link and atomic alias replacement use one inode.
/// Cross-filesystem: verify/fsync the transfer before replacing the source by its alias.
pub(crate) fn migrate(path: &Path, root: &Path) -> Result<PathBuf, String> {
    if crate::procedural::id(path).is_some() {
        return Ok(path.to_path_buf());
    }
    let source = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let mut input = media(&source)?;
    let info = input.metadata().map_err(|e| e.to_string())?;
    // Packaged/public system media already has one canonical copy. Never mutate another owner.
    if info.uid() != unsafe { libc::geteuid() } {
        let hash = checksum(&mut input)?;
        let extension = source
            .extension()
            .and_then(|s| s.to_str())
            .ok_or("Media extension missing")?;
        let mut destination = root.join(format!("{hash}-public.{extension}"));
        let mut number = 0;
        while fs::symlink_metadata(&destination).is_ok() {
            if fs::canonicalize(&destination).is_ok_and(|p| p == source) {
                return Ok(source);
            }
            number += 1;
            if number > 10000 {
                return Err("Too many colliding media names".into());
            }
            destination = root.join(format!("{hash}-public-{number}.{extension}"));
        }
        std::os::unix::fs::symlink(&source, destination).map_err(|e| e.to_string())?;
        directory(root, false)?
            .sync_all()
            .map_err(|e| e.to_string())?;
        return Ok(source);
    }
    if source.starts_with(root) {
        public(&input)?;
        label_import(&source, &input)?;
        return Ok(source);
    }
    let hash = checksum(&mut input)?;
    let after = input.metadata().map_err(|e| e.to_string())?;
    if info.len() != after.len()
        || info.mtime() != after.mtime()
        || info.mtime_nsec() != after.mtime_nsec()
    {
        return Err("Media changed during migration; source was preserved".into());
    }
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Media filename must be UTF-8")?;
    let mut bytes = 0;
    let stem: String = stem
        .chars()
        .take_while(|c| {
            bytes += c.len_utf8();
            bytes <= 150
        })
        .collect();
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .ok_or("Media extension missing")?;
    let mut destination = existing_content(root, &hash, info.uid())?
        .unwrap_or_else(|| root.join(format!("{hash}-{stem}.{extension}")));
    if destination.exists() {
        let mut existing = media(&destination)?;
        if existing.metadata().map_err(|e| e.to_string())?.uid() != info.uid()
            || checksum(&mut existing)? != hash
        {
            let mut number = 1;
            loop {
                let alternative = root.join(format!("{hash}-{stem}-{number}.{extension}"));
                if !alternative.exists() {
                    destination = alternative;
                    break;
                }
                number += 1;
                if number > 10000 {
                    return Err("Too many colliding media names".into());
                }
            }
        }
    }
    if destination.exists() {
        let mut existing = media(&destination)?;
        if existing.metadata().map_err(|e| e.to_string())?.uid() != info.uid()
            || checksum(&mut existing)? != hash
        {
            return Err("Shared destination contents/owner differ; source was preserved".into());
        }
    } else {
        match fs::hard_link(&source, &destination) {
            Ok(()) => (),
            Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                use std::io::{Seek, SeekFrom};
                input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                let mut output =
                    tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
                std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
                output.flush().map_err(|e| e.to_string())?;
                output.as_file().sync_all().map_err(|e| e.to_string())?;
                let mut verify = output.reopen().map_err(|e| e.to_string())?;
                if checksum(&mut verify)? != hash {
                    return Err("Media transfer verification failed; source was preserved".into());
                }
                output
                    .persist_noclobber(&destination)
                    .map_err(|e| e.to_string())?;
            }
            Err(error) => return Err(format!("Cannot migrate media: {error}")),
        }
    }
    let published = media(&destination)?;
    public(&published)?;
    label_import(&destination, &published)?;
    directory(root, false)?
        .sync_all()
        .map_err(|e| e.to_string())?;
    let current = fs::metadata(&source).map_err(|e| e.to_string())?;
    if current.ino() != info.ino()
        || current.dev() != info.dev()
        || current.len() != info.len()
        || current.mtime() != info.mtime()
        || current.mtime_nsec() != info.mtime_nsec()
    {
        return Err("Source changed before migration completed; source was preserved".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix(".decklock-alias-")
        .tempdir_in(source.parent().unwrap())
        .map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&destination, temporary.path().join("link"))
        .map_err(|e| e.to_string())?;
    fs::rename(temporary.path().join("link"), &source).map_err(|e| e.to_string())?;
    File::open(source.parent().unwrap())
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())?;
    directory(root, false)?
        .sync_all()
        .map_err(|e| e.to_string())?;
    Ok(destination)
}

// Only inspect root-controlled, already-public package files as root. In
// particular, never hash arbitrary paths supplied by the unprivileged exporter.
fn bundled_content(hash: &str) -> Result<Option<PathBuf>, String> {
    for root in [
        "/usr/share/decklock/media",
        "/usr/local/share/decklock/media",
    ] {
        for path in crate::library::files(Path::new(root)) {
            let safe = path.ancestors().skip(1).all(|parent| {
                directory(parent, false)
                    .and_then(|d| d.metadata().map_err(|e| e.to_string()))
                    .is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0 && m.mode() & 0o001 != 0)
            });
            if !safe
                || fs::symlink_metadata(&path)
                    .map_err(|e| e.to_string())?
                    .is_symlink()
            {
                continue;
            }
            let mut file = media(&path)?;
            let info = file.metadata().map_err(|e| e.to_string())?;
            if info.uid() == 0 && info.mode() & 0o004 != 0 && checksum(&mut file)? == hash {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

/// Convert generated snapshot payloads to aliases, including previously selected
/// media. Their tiny config/theme generations remain valid for recovery.
pub(super) fn retire_snapshots(
    state: &Path,
    root: &Path,
    old_uid: u32,
    uid: u32,
) -> Result<(), String> {
    let state_fd = directory(state, false)?;
    let state_anchor = PathBuf::from(format!("/proc/self/fd/{}", state_fd.as_raw_fd()));
    let root_fd = directory(root, false)?;
    let root_info = root_fd.metadata().map_err(|e| e.to_string())?;
    if root_info.uid() != uid {
        return Err("Shared library ownership changed".into());
    }
    let root_anchor = PathBuf::from(format!("/proc/self/fd/{}", root_fd.as_raw_fd()));
    for entry in fs::read_dir(&state_anchor).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(hash) = name.to_str().and_then(|s| s.strip_prefix("appearance-")) else {
            continue;
        };
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let snapshot = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(state_anchor.join(&name))
            .map_err(|e| e.to_string())?;
        if snapshot.metadata().map_err(|e| e.to_string())?.uid() != old_uid {
            return Err("Legacy snapshot directory ownership changed; media was preserved".into());
        }
        let anchor = PathBuf::from(format!("/proc/self/fd/{}", snapshot.as_raw_fd()));
        for entry in fs::read_dir(&anchor).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let path = anchor.join(&name);
            let Some(text) = name.to_str() else {
                continue;
            };
            if !text.starts_with("media-") || crate::library::kind(&path).is_none() {
                continue;
            }
            if fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .is_symlink()
            {
                continue;
            }
            let mut input = media(&path)?;
            let info = input.metadata().map_err(|e| e.to_string())?;
            let hash = checksum(&mut input)?;
            let existing = existing_content(root, &hash, uid)?;
            let recovering = info.uid() == uid
                && info.nlink() == 2
                && existing.as_ref().is_some_and(|p| {
                    fs::metadata(p).is_ok_and(|m| m.ino() == info.ino() && m.dev() == info.dev())
                });
            if !recovering && (info.uid() != old_uid || info.nlink() != 1) {
                return Err("Legacy media ownership/links changed; media was preserved".into());
            }
            let bundled = if existing.is_none() {
                bundled_content(&hash)?
            } else {
                None
            };
            let canonical_path = existing
                .or(bundled.clone())
                .unwrap_or_else(|| root.join(format!("{hash}-legacy-{text}")));
            let target = if canonical_path.starts_with(root) {
                root_anchor.join(
                    canonical_path
                        .file_name()
                        .ok_or("Canonical filename missing")?,
                )
            } else {
                canonical_path.clone()
            };
            if !target.exists() {
                match fs::hard_link(&path, &target) {
                    Ok(()) => {
                        let linked = media(&target)?;
                        let linked_info = linked.metadata().map_err(|e| e.to_string())?;
                        if linked_info.ino() != info.ino()
                            || linked_info.dev() != info.dev()
                            || linked_info.uid() != old_uid
                        {
                            return Err(
                                "Legacy source changed; original media was preserved".into()
                            );
                        }
                        if unsafe { libc::geteuid() } == 0
                            && unsafe { libc::fchown(linked.as_raw_fd(), uid, root_info.gid()) }
                                != 0
                        {
                            return Err(std::io::Error::last_os_error().to_string());
                        }
                        public(&linked)?;
                    }
                    Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
                        use std::io::{Seek, SeekFrom};
                        input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                        let mut file = tempfile::NamedTempFile::new_in(&root_anchor)
                            .map_err(|e| e.to_string())?;
                        std::io::copy(&mut input, &mut file).map_err(|e| e.to_string())?;
                        file.as_file().sync_all().map_err(|e| e.to_string())?;
                        if checksum(&mut file.reopen().map_err(|e| e.to_string())?)? != hash {
                            return Err("Legacy transfer verification failed".into());
                        }
                        if unsafe { libc::geteuid() } == 0
                            && unsafe {
                                libc::fchown(file.as_file().as_raw_fd(), uid, root_info.gid())
                            } != 0
                        {
                            return Err(std::io::Error::last_os_error().to_string());
                        }
                        public(file.as_file())?;
                        file.persist_noclobber(&target).map_err(|e| e.to_string())?;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            let mut canonical = media(&target)?;
            if canonical.metadata().map_err(|e| e.to_string())?.uid()
                != if bundled.is_some() { 0 } else { uid }
                || checksum(&mut canonical)? != hash
            {
                return Err("Canonical media differs; legacy media was preserved".into());
            }
            use std::os::unix::ffi::OsStrExt;
            let source_name = std::ffi::CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
            root_fd.sync_all().map_err(|e| e.to_string())?;
            let current = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if current.ino() != info.ino()
                || current.dev() != info.dev()
                || current.len() != info.len()
                || current.mtime() != info.mtime()
                || current.mtime_nsec() != info.mtime_nsec()
            {
                return Err("Legacy source changed; media was preserved".into());
            }
            let canonical_path = std::ffi::CString::new(canonical_path.as_os_str().as_bytes())
                .map_err(|e| e.to_string())?;
            // Both operations stay relative to the held snapshot fd. A service
            // replacing an ancestor cannot redirect a privileged rename.
            let alias =
                std::ffi::CString::new(format!(".media-alias-{}-{}", std::process::id(), hash))
                    .unwrap();
            if unsafe {
                libc::symlinkat(
                    canonical_path.as_ptr(),
                    snapshot.as_raw_fd(),
                    alias.as_ptr(),
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if unsafe {
                libc::renameat(
                    snapshot.as_raw_fd(),
                    alias.as_ptr(),
                    snapshot.as_raw_fd(),
                    source_name.as_ptr(),
                )
            } != 0
            {
                let error = std::io::Error::last_os_error();
                unsafe {
                    libc::unlinkat(snapshot.as_raw_fd(), alias.as_ptr(), 0);
                }
                return Err(error.to_string());
            }
            snapshot.sync_all().map_err(|e| e.to_string())?;
        }
    }
    root_fd.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
