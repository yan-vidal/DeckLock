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
