use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: Option<PathBuf>,
    pub locale: Option<String>,
    pub pam_service: String,
    pub idle_seconds: u32,
    pub background: Option<PathBuf>,
    pub idle_background: Option<PathBuf>,
    pub controller_socket: Option<PathBuf>,
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
        if !(1..=86400).contains(&config.idle_seconds) {
            return Err("idle_seconds must be between 1 and 86400".into());
        }
        if config.pam_service.is_empty()
            || config.pam_service.len() > 64
            || !config
                .pam_service
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("pam_service must be a service name, not a path".into());
        }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Alignment {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Arrangement {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Deserialize)]
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
        if !(0..=128).contains(&file.layout.spacing)
            || !(0..=256).contains(&file.layout.padding)
            || !file.layout.keyboard_scale.is_finite()
            || !(0.5..=2.0).contains(&file.layout.keyboard_scale)
        {
            return Err(
                "Theme layout requires spacing 0..128, padding 0..256, keyboard_scale 0.5..2.0"
                    .into(),
            );
        }
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
