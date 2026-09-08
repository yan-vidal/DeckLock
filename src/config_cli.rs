//! Headless configuration editing. Uses the same schema and atomic save as GTK.
use crate::config::{Config, Theme};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Action {
    /// Print configuration as TOML (omitted optional values are inherited).
    Show,
    /// Print the configuration file path.
    Path,
    /// Read a value, including nested layout options.
    Get { key: String },
    /// Set a key. Values accept TOML booleans, numbers, arrays, or plain strings.
    Set {
        key: String,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Reset a key to its default; unset layout restores the theme's layout.
    Unset { key: String },
    /// Validate and atomically replace configuration from a TOML file.
    Import { file: PathBuf },
}
fn validate(config: &Config) -> Result<(), String> {
    config.validate()?;
    crate::themes::colors(&config.theme_preset, false)?;
    Theme::from_config(config)?;
    crate::i18n::I18n::new(config.locale.as_deref(), None)?;
    Ok(())
}
fn parts(key: &str) -> Result<Vec<&str>, String> {
    let parts: Vec<_> = key.split('.').collect();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err("Use a key such as idle_seconds or layout.padding".into());
    }
    Ok(parts)
}
fn table_at<'a>(
    root: &'a mut toml::Value,
    keys: &[&str],
) -> Result<&'a mut toml::map::Map<String, toml::Value>, String> {
    let mut current = root;
    for key in keys {
        current = current
            .get_mut(*key)
            .ok_or_else(|| format!("Unknown configuration table: {key}"))?;
    }
    current
        .as_table_mut()
        .ok_or_else(|| "Parent key is not a table".into())
}
pub fn execute(path: &Path, action: Action) -> Result<String, String> {
    if matches!(action, Action::Path) {
        return Ok(path.display().to_string());
    }
    if let Action::Import { file } = action {
        let file = if file.is_absolute() {
            file
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(file)
        };
        let config = Config::load(Some(&file))?;
        validate(&config)?;
        config.save(path)?;
        return Ok(format!("Saved {}", path.display()));
    }
    // Preserve permission/read errors; a missing file alone selects defaults.
    let config = match std::fs::metadata(path) {
        Ok(_) => Config::load(Some(path))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(e.to_string()),
    };
    if matches!(action, Action::Show) {
        return toml::to_string_pretty(&config).map_err(|e| e.to_string());
    }
    let (key, value, remove) = match action {
        Action::Get { key } => (key, None, false),
        Action::Set { key, value } => (key, Some(value), false),
        Action::Unset { key } => (key, None, true),
        _ => unreachable!(),
    };
    let keys = parts(&key)?;
    let mut document = toml::Value::try_from(&config).map_err(|e| e.to_string())?;
    if keys.len() > 1 && keys[0] == "layout" && config.layout.is_none() {
        document.as_table_mut().unwrap().insert(
            "layout".into(),
            toml::Value::try_from(Theme::from_config(&config)?.layout)
                .map_err(|e| e.to_string())?,
        );
    }
    let table = table_at(&mut document, &keys[..keys.len() - 1])?;
    let leaf = keys[keys.len() - 1];
    if let Some(value) = value {
        let parsed = toml::from_str::<toml::Table>(&format!("value = {value}"))
            .ok()
            .and_then(|mut table| {
                if table.len() == 1 {
                    table.remove("value")
                } else {
                    None
                }
            })
            .unwrap_or(toml::Value::String(value));
        table.insert(leaf.to_owned(), parsed);
    } else if remove {
        if table.remove(leaf).is_none() {
            return Err(format!("Key is unset or unknown: {key}"));
        }
    } else {
        return table
            .get(leaf)
            .map(ToString::to_string)
            .ok_or_else(|| format!("Key is unset or unknown: {key}"));
    }
    let mut changed: Config = document
        .try_into()
        .map_err(|e| format!("Invalid configuration: {e}"))?;
    changed.resolve_paths(path.parent().unwrap_or(Path::new(".")));
    validate(&changed)?;
    changed.save(path)?;
    Ok(format!("Saved {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_edits_nested_values_preserves_unedited_fields_and_rejects_invalid_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        let set = |key: &str, value: &str| {
            execute(
                &file,
                Action::Set {
                    key: key.into(),
                    value: value.into(),
                },
            )
        };
        set("pam_service", "preserve-me").unwrap();
        set("layout.padding", "48").unwrap();
        set("idle_enabled", "false").unwrap();
        set("background_pool", "[\"/tmp/a.jpg\", \"/tmp/b.mp4\"]").unwrap();
        let config = Config::load(Some(&file)).unwrap();
        assert_eq!(config.pam_service, "preserve-me");
        assert_eq!(config.layout.unwrap().padding, 48);
        assert!(!config.idle_enabled);
        assert_eq!(config.background_pool.unwrap().len(), 2);
        assert_eq!(
            execute(
                &file,
                Action::Get {
                    key: "layout.padding".into()
                }
            )
            .unwrap(),
            "48"
        );
        let before = std::fs::read(&file).unwrap();
        for (key, value) in [
            ("idle_seconds", "0"),
            ("layout.padding", "999"),
            ("typo", "true"),
            ("theme_preset", "invalid"),
            ("layout.typo", "1"),
            ("idle_enabled", "maybe"),
        ] {
            assert!(set(key, value).is_err(), "{key}");
            assert_eq!(std::fs::read(&file).unwrap(), before);
        }
        execute(
            &file,
            Action::Unset {
                key: "layout".into(),
            },
        )
        .unwrap();
        assert!(Config::load(Some(&file)).unwrap().layout.is_none());
    }
    #[test]
    fn headless_reads_do_not_create_files_and_import_resolves_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new/config.toml");
        execute(&file, Action::Show).unwrap();
        assert!(!file.exists());
        let source = dir.path().join("source.toml");
        std::fs::write(&source, "background_pool = ['photo.jpg']\n").unwrap();
        execute(&file, Action::Import { file: source }).unwrap();
        assert_eq!(
            Config::load(Some(&file)).unwrap().background_pool.unwrap(),
            vec![dir.path().join("photo.jpg")]
        );
    }
}
