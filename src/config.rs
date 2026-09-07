use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: Option<PathBuf>,
    pub locale: Option<String>,
    pub pam_service: String,
    pub idle_seconds: u32,
    pub background: Option<PathBuf>,
    pub idle_background: Option<PathBuf>,
    pub controller_socket: Option<PathBuf>,
    pub system_keyboard: bool,
    pub layout: Option<Layout>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            locale: None,
            pam_service: "login".into(),
            idle_seconds: 600,
            background: None,
            idle_background: None,
            controller_socket: None,
            system_keyboard: true,
            layout: None,
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let mut config: Self =
            toml::from_str(&read_text(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        config.validate()?;
        let base = path.parent().unwrap_or(Path::new("."));
        for path in [
            &mut config.theme,
            &mut config.background,
            &mut config.idle_background,
            &mut config.controller_socket,
        ]
        .into_iter()
        .flatten()
        {
            *path = resolve(base, path);
        }
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=86400).contains(&self.idle_seconds) {
            return Err("idle_seconds must be between 1 and 86400".into());
        }
        if self.pam_service.is_empty()
            || self.pam_service.len() > 64
            || !self
                .pam_service
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("pam_service must be a service name, not a path".into());
        }
        if let Some(layout) = &self.layout {
            layout.validate()?;
        }
        Ok(())
    }

    /// Replace only after serialization and validation succeed.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use std::io::Write;
        self.validate()?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Alignment {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Arrangement {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub alignment: Alignment,
    pub arrangement: Arrangement,
    pub spacing: i32,
    pub padding: i32,
    pub clock_visible: bool,
    pub avatar_visible: bool,
    pub keyboard_scale: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            alignment: Alignment::Center,
            arrangement: Arrangement::Vertical,
            spacing: 16,
            padding: 32,
            clock_visible: true,
            avatar_visible: true,
            keyboard_scale: 1.0,
        }
    }
}

impl Layout {
    pub fn validate(&self) -> Result<(), String> {
        if !(0..=128).contains(&self.spacing)
            || !(0..=256).contains(&self.padding)
            || !self.keyboard_scale.is_finite()
            || !(0.5..=2.0).contains(&self.keyboard_scale)
        {
            return Err(
                "Theme layout requires spacing 0..128, padding 0..256, keyboard_scale 0.5..2.0"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub css: String,
    pub layout: Layout,
    pub background: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ThemeFile {
    name: String,
    css: PathBuf,
    layout: Layout,
    background: Option<PathBuf>,
}

impl Default for ThemeFile {
    fn default() -> Self {
        Self {
            name: "DeckLock".into(),
            css: "style.css".into(),
            layout: Layout::default(),
            background: None,
        }
    }
}

impl Theme {
    /// `path` is a theme directory containing theme.toml and a local CSS file.
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let source = match path {
            Some(dir) => read_text(&dir.join("theme.toml"))?,
            None => include_str!("../themes/default/theme.toml").to_owned(),
        };
        let file: ThemeFile = toml::from_str(&source).map_err(|e| format!("Invalid theme: {e}"))?;
        file.layout.validate()?;
        let css = match path {
            Some(dir) => read_text(&resolve(dir, &file.css))?,
            None => include_str!("../themes/default/style.css").to_owned(),
        };
        Ok(Self {
            name: file.name,
            css,
            layout: file.layout,
            background: file
                .background
                .map(|p| resolve(path.unwrap_or(Path::new(".")), &p)),
        })
    }
}

fn resolve(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_preserves_unedited_options_and_layout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        let config = Config {
            pam_service: "custom-login".into(),
            idle_background: Some(dir.path().join("idle.mp4")),
            controller_socket: Some(dir.path().join("daemon.socket")),
            layout: Some(Layout {
                alignment: Alignment::End,
                keyboard_scale: 1.25,
                ..Layout::default()
            }),
            ..Config::default()
        };
        config.save(&path).unwrap();
        let loaded = Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.pam_service, "custom-login");
        assert_eq!(loaded.idle_background, config.idle_background);
        assert_eq!(loaded.controller_socket, config.controller_socket);
        assert_eq!(loaded.layout.as_ref().unwrap().alignment, Alignment::End);
        assert_eq!(loaded.layout.unwrap().keyboard_scale, 1.25);
    }

    #[test]
    fn invalid_settings_do_not_replace_existing_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let config = Config {
            layout: Some(Layout {
                padding: -1,
                ..Layout::default()
            }),
            ..Config::default()
        };
        assert!(config.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn default_theme_is_valid_and_has_styles() {
        let theme = Theme::load(None).unwrap();
        assert_eq!(theme.layout.alignment, Alignment::Center);
        assert!(!theme.css.is_empty());
    }

    #[test]
    fn config_rejects_bad_values_and_unknown_options() {
        for content in [
            "idle_seconds = 0",
            "pam_service = '../login'",
            "idle_second = 12",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.toml");
            std::fs::write(&path, content).unwrap();
            assert!(Config::load(Some(&path)).is_err(), "{content}");
        }
    }

    #[test]
    fn paths_resolve_relative_to_config_and_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "theme = 'theme'\nbackground = 'wall.png'").unwrap();
        let config = Config::load(Some(&path)).unwrap();
        assert_eq!(config.background, Some(dir.path().join("wall.png")));
        assert_eq!(config.theme, Some(dir.path().join("theme")));
    }

    #[test]
    fn themes_reject_invalid_geometry_and_script_fields() {
        for content in [
            "[layout]\nkeyboard_scale = 0.0",
            "[layout]\npadding = -1",
            "command = 'anything'",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("theme.toml"), content).unwrap();
            std::fs::write(dir.path().join("style.css"), "window {}").unwrap();
            assert!(Theme::load(Some(dir.path())).is_err(), "{content}");
        }
    }
}
