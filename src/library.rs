//! File-based media library and per-lock selection, independent of GTK.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Procedural,
}
pub fn kind(path: &Path) -> Option<Kind> {
    if crate::procedural::id(path).is_some() {
        return Some(Kind::Procedural);
    }
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "avif" | "bmp" | "svg" => Some(Kind::Image),
        "mp4" | "mkv" | "webm" | "mov" => Some(Kind::Video),
        _ => None,
    }
}
pub fn files(path: &Path) -> Vec<PathBuf> {
    if crate::procedural::id(path).is_some() {
        return vec![path.into()];
    }
    if path.is_file() {
        return kind(path).map(|_| vec![path.into()]).unwrap_or_default();
    }
    let mut result: Vec<_> = [
        path.to_path_buf(),
        path.join("fotos"),
        path.join("videos"),
        path.join("images"),
    ]
    .into_iter()
    .filter_map(|p| std::fs::read_dir(p).ok())
    .flatten()
    .filter_map(Result::ok)
    .map(|e| e.path())
    .filter(|p| p.is_file() && kind(p).is_some())
    .collect();
    result.sort();
    result.dedup();
    result
}
fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
}
pub fn user_dir() -> PathBuf {
    data_home().join("decklock/library")
}
pub fn bundled() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    )
    .map(|p| p.join("decklock/media"))
    .collect();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        roots.push(dir.join("../share/decklock/media"));
        roots.push(dir.join("media"));
    }
    roots.push(data_home().join("decklock/media"));
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/media"));
    let mut result: Vec<_> = roots.iter().flat_map(|p| files(p)).collect();
    result.sort();
    result.dedup();
    result
}
pub fn pool(
    config: &crate::config::Config,
    theme: &crate::config::Theme,
    idle: bool,
) -> Vec<PathBuf> {
    let explicit = if idle {
        &config.idle_pool
    } else {
        &config.background_pool
    };
    if let Some(paths) = explicit {
        return paths.iter().flat_map(|p| files(p)).collect();
    }
    let legacy = if idle {
        config.idle_background.as_deref()
    } else {
        config.background.as_deref().or(theme.background.as_deref())
    };
    if let Some(path) = legacy {
        let result = files(path);
        if !result.is_empty() {
            return result;
        }
    }
    if let Some(home) = crate::shortcut::config_dir() {
        let result = files(&home.join(if idle {
            "midias/ocioso"
        } else {
            "midias/bloqueio"
        }));
        if !result.is_empty() {
            return result;
        }
    }
    if idle { Vec::new() } else { bundled() }
}
pub fn catalog(config: &crate::config::Config, theme: &crate::config::Theme) -> Vec<PathBuf> {
    let mut result = bundled();
    result.extend(crate::procedural::ITEMS.map(crate::procedural::path));
    result.extend(files(&user_dir()));
    for paths in [&config.background_pool, &config.idle_pool]
        .into_iter()
        .flatten()
    {
        result.extend(paths.clone());
    }
    for idle in [false, true] {
        result.extend(pool(config, theme, idle));
    }
    if let Some(home) = crate::shortcut::config_dir() {
        for mode in ["bloqueio", "ocioso"] {
            result.extend(files(&home.join("midias").join(mode)));
        }
    }
    result.sort();
    result.dedup();
    result
}
/// Imports never overwrite an existing file and never remove the source.
pub fn import(source: &Path, root: &Path) -> Result<PathBuf, String> {
    let folder = match kind(source) {
        Some(Kind::Image) => "images",
        Some(Kind::Video) => "videos",
        Some(Kind::Procedural) | None => return Err("Unsupported media format".into()),
    };
    if !source.is_file() {
        return Err("Media file does not exist".into());
    }
    let folder = root.join(folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name = source
        .file_name()
        .ok_or("Missing filename")?
        .to_string_lossy();
    for number in 0..10000 {
        let dest = folder.join(if number == 0 {
            name.to_string()
        } else {
            format!("{number}-{name}")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(mut output) => {
                let copy = std::fs::File::open(source)
                    .and_then(|mut input| std::io::copy(&mut input, &mut output));
                if let Err(e) = copy {
                    let _ = std::fs::remove_file(&dest);
                    return Err(e.to_string());
                }
                return Ok(dest);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many media files with this name".into())
}
#[derive(Debug)]
pub struct Selection {
    pub paths: Vec<PathBuf>,
    index: usize,
}
impl Selection {
    pub fn new(pool: Vec<PathBuf>, seed: usize) -> Self {
        if pool.is_empty() {
            return Self {
                paths: pool,
                index: 0,
            };
        }
        let chosen = pool[seed % pool.len()].clone();
        let paths = if matches!(kind(&chosen), Some(Kind::Video | Kind::Procedural)) {
            vec![chosen.clone()]
        } else {
            pool.into_iter()
                .filter(|p| kind(p) == Some(Kind::Image))
                .collect()
        };
        let index = paths.iter().position(|p| p == &chosen).unwrap_or(0);
        Self { paths, index }
    }
    pub fn current(&self) -> Option<&Path> {
        self.paths.get(self.index).map(PathBuf::as_path)
    }
    pub fn advance(&mut self) -> bool {
        if self.paths.len() < 2 {
            return false;
        }
        self.index = (self.index + 1) % self.paths.len();
        true
    }
}
pub fn seed() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_stays_fixed_and_images_cycle_without_entering_video() {
        let paths = vec!["a.png".into(), "v.mp4".into(), "b.jpg".into()];
        let mut video = Selection::new(paths.clone(), 1);
        assert!(!video.advance());
        assert_eq!(video.current(), Some(Path::new("v.mp4")));
        let mut images = Selection::new(paths, 0);
        assert!(images.advance());
        assert_eq!(images.current(), Some(Path::new("b.jpg")));
        assert!(images.advance());
        assert_eq!(images.current(), Some(Path::new("a.png")));
        assert!(!Selection::new(vec![], 0).advance());
    }
    #[test]
    fn import_preserves_source_and_colliding_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("image.png");
        std::fs::write(&source, "first").unwrap();
        let root = dir.path().join("library");
        let first = import(&source, &root).unwrap();
        std::fs::write(&source, "second").unwrap();
        let second = import(&source, &root).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(source).unwrap(), "second");
        assert_eq!(files(&root).len(), 2);
    }
}
