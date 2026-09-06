//! Compatibility with the existing `deck-osk --toggle` launcher.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Registration {
    path: PathBuf,
    pid: String,
}

impl Registration {
    pub fn acquire(path: PathBuf) -> Result<Self, String> {
        let parent = path.parent().ok_or("Invalid shortcut path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(pid) = text.trim().parse::<i32>() {
                // SAFETY: signal zero checks existence without delivering a signal.
                if pid > 0
                    && (unsafe { libc::kill(pid, 0) } == 0
                        || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
                {
                    return Err("Another locker already owns the keyboard shortcut".into());
                }
            }
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let pid = std::process::id().to_string();
        file.write_all(pid.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Self { path, pid })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).ok().as_deref() == Some(&self.pid) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
}

pub fn cooldown(dir: &Path) {
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        let _ = fs::write(
            dir.join("ghost-osk.cooldown"),
            now.as_secs_f64().to_string(),
        );
    }
}

/// Standalone launcher: no GTK initialization, Python, or controller capture.
pub fn toggle() -> Result<(), String> {
    let dir = config_dir()
        .ok_or("Cannot locate DeckLock configuration")?
        .join("scc");
    if fs::metadata(dir.join("ghost-osk.cooldown"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|elapsed| elapsed < std::time::Duration::from_secs(1))
    {
        return Ok(());
    }
    let pid: i32 = fs::read_to_string(dir.join("deck-lock.pid"))
        .map_err(|_| "No running DeckLock instance")?
        .trim()
        .parse()
        .map_err(|_| "Invalid DeckLock PID")?;
    if pid <= 0 {
        return Err("Invalid DeckLock PID".into());
    }
    let executable =
        fs::read_link(format!("/proc/{pid}/exe")).map_err(|_| "DeckLock is no longer running")?;
    let name = executable
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or_default();
    if !matches!(name, "decklock" | "decklock (deleted)") {
        return Err("PID does not belong to DeckLock".into());
    }
    // SAFETY: positive PID verified against the Rust executable; SIGUSR1 only
    // toggles the embedded keyboard and never unlocks the session.
    if unsafe { libc::kill(pid, libc::SIGUSR1) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_rejects_live_owner_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deck-lock.pid");
        let guard = Registration::acquire(path.clone()).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
        assert!(Registration::acquire(path.clone()).is_err());
        drop(guard);
        assert!(!path.exists());
    }
    #[test]
    fn stale_registration_is_replaced_without_removing_new_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deck-lock.pid");
        fs::write(&path, "invalid").unwrap();
        let guard = Registration::acquire(path.clone()).unwrap();
        fs::write(&path, "replacement").unwrap();
        drop(guard);
        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    }
}
