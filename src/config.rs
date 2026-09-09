use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub procedurals: crate::procedural::Presets,
    #[serde(skip_serializing)]
    pub animation: crate::animation::Animation,
    #[serde(skip_serializing)]
    pub idle_animation: crate::animation::Animation,
    pub theme: Option<PathBuf>,
    pub theme_preset: String,
    pub locale: Option<String>,
    pub pam_service: String,
    pub idle_seconds: u32,
    pub idle_enabled: bool,
    pub idle_reuse_background: bool,
    pub background_pool: Option<Vec<PathBuf>>,
    pub idle_pool: Option<Vec<PathBuf>>,
    pub slideshow_seconds: u32,
    pub idle_slideshow_seconds: u32,
    pub background: Option<PathBuf>,
    pub idle_background: Option<PathBuf>,
    pub controller_socket: Option<PathBuf>,
    pub system_keyboard: bool,
    pub window_decorations: bool,
    pub layout: Option<Layout>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            procedurals: Default::default(),
            animation: Default::default(),
            idle_animation: Default::default(),
            theme: None,
            theme_preset: "classic".into(),
            locale: None,
            pam_service: "login".into(),
            idle_seconds: 600,
            idle_enabled: true,
            idle_reuse_background: false,
            background_pool: None,
            idle_pool: None,
            slideshow_seconds: 30,
            idle_slideshow_seconds: 30,
            background: None,
            idle_background: None,
            controller_socket: None,
            system_keyboard: true,
            window_decorations: true,
            layout: None,
        }
    }
}

impl Config {
    // Unreleased overlay drafts migrate to media items without deleting existing media.
    fn migrate_overlays(&mut self) {
        for (old, pool) in [
            (&mut self.animation, &mut self.background_pool),
            (&mut self.idle_animation, &mut self.idle_pool),
        ] {
            if old.effect != crate::animation::Effect::None {
                let id = old.effect.id();
                self.procedurals.set(
                    id,
                    crate::procedural::Parameters {
                        density: old.density,
                        speed: old.speed,
                        fps: old.fps,
                        color: old.color.clone(),
                        background: old.background.clone(),
                        seed: old.seed,
                    },
                );
                let path = crate::procedural::path(id);
                let pool = pool.get_or_insert_with(Vec::new);
                if !pool.contains(&path) {
                    pool.push(path);
                }
                old.effect = crate::animation::Effect::None;
            }
        }
    }

    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let mut config: Self =
            toml::from_str(&read_text(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        config.migrate_overlays();
        config.validate()?;
        let base = path.parent().unwrap_or(Path::new("."));
        config.resolve_paths(base);
        Ok(config)
    }
    pub fn resolve_paths(&mut self, base: &Path) {
        for path in [
            &mut self.theme,
            &mut self.background,
            &mut self.idle_background,
            &mut self.controller_socket,
        ]
        .into_iter()
        .flatten()
        {
            *path = resolve(base, path);
        }
        for paths in [&mut self.background_pool, &mut self.idle_pool]
            .into_iter()
            .flatten()
        {
            for path in paths {
                if !path.to_string_lossy().starts_with("procedural:") {
                    *path = resolve(base, path);
                }
            }
        }
    }
    /// Preserve the original media folders and explicit config/theme precedence.
    pub fn prepare_media(
        &mut self,
        config_home: &Path,
        theme_background: Option<&Path>,
    ) -> Result<(), String> {
        let root = config_home.join("midias");
        for mode in ["bloqueio", "ocioso"] {
            for kind in ["fotos", "videos"] {
                std::fs::create_dir_all(root.join(mode).join(kind)).map_err(|e| e.to_string())?;
            }
        }
        if self.background.is_none() && theme_background.is_none() {
            self.background = Some(root.join("bloqueio"));
        }
        if self.idle_background.is_none() {
            self.idle_background = Some(root.join("ocioso"));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.procedurals.validate()?;
        for path in [&self.background_pool, &self.idle_pool]
            .into_iter()
            .flatten()
            .flatten()
        {
            if path.to_string_lossy().starts_with("procedural:")
                && crate::procedural::id(path).is_none()
            {
                return Err("Unknown procedural media ID".into());
            }
        }
        self.animation.validate()?;
        self.idle_animation.validate()?;
        if !(1..=86400).contains(&self.slideshow_seconds)
            || !(1..=86400).contains(&self.idle_slideshow_seconds)
        {
            return Err("Slideshow intervals must be between 1 and 86400 seconds".into());
        }
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
        let mut migrated = self.clone();
        migrated.migrate_overlays();
        let text = toml::to_string_pretty(&migrated).map_err(|e| e.to_string())?;
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
    pub idle_clock_visible: bool,
    pub avatar_visible: bool,
    pub power_visible: bool,
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
            idle_clock_visible: true,
            avatar_visible: true,
            power_visible: true,
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

#[derive(Deserialize, Serialize)]
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
    pub fn document(&self) -> Result<String, String> {
        toml::to_string_pretty(&ThemeFile {
            name: self.name.clone(),
            css: "style.css".into(),
            layout: self.layout.clone(),
            background: self.background.clone(),
        })
        .map_err(|e| e.to_string())
    }
    pub fn from_document(source: &str, css: &str, base: &Path) -> Result<Self, String> {
        let file: ThemeFile = toml::from_str(source).map_err(|e| e.to_string())?;
        file.layout.validate()?;
        if file.css != Path::new("style.css") {
            return Err(
                "The CSS tab is saved as style.css; keep css = 'style.css' in theme.toml".into(),
            );
        }
        Ok(Self {
            name: file.name,
            css: format!(
                "{}\n{}\n{}",
                crate::themes::colors("classic", false)?,
                crate::themes::settings_css(),
                css
            ),
            layout: file.layout,
            background: file.background.map(|p| resolve(base, &p)),
        })
    }

    pub fn from_config(config: &Config) -> Result<Self, String> {
        if config.theme.is_some() {
            return Self::load(config.theme.as_deref());
        }
        let mut theme = Self::load(None)?;
        theme.css +=
            &crate::themes::colors(&config.theme_preset, config.theme_preset != "classic")?;
        theme.name = crate::themes::presets()
            .into_iter()
            .find(|p| p.id == config.theme_preset)
            .unwrap()
            .name;
        Ok(theme)
    }

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
            css: format!(
                "{}\n{}\n{}",
                crate::themes::colors("classic", false)?,
                crate::themes::settings_css(),
                css
            ),
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
    #[test]
    fn procedural_ids_and_unreleased_overlay_drafts_migrate_without_path_expansion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "background_pool=['photo.png']\n[animation]\neffect='starfield'\nspeed=0.8\n",
        )
        .unwrap();
        let config = Config::load(Some(&path)).unwrap();
        assert_eq!(
            config.background_pool,
            Some(vec![
                dir.path().join("photo.png"),
                crate::procedural::path("starfield")
            ])
        );
        assert_eq!(config.procedurals.starfield.speed, 0.8);
        config.save(&path).unwrap();
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(!source.contains("[animation]"));
        let reloaded = Config::load(Some(&path)).unwrap();
        assert_eq!(reloaded.background_pool, config.background_pool);
        assert_eq!(reloaded.procedurals.starfield.speed, 0.8);
    }
    #[test]
    fn pools_round_trip_and_resolve_relative_media() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "background_pool = ['a.png', 'b.mp4']\nidle_pool = []\nidle_enabled = false\nidle_reuse_background = true\nslideshow_seconds = 12\n").unwrap();
        let config = super::Config::load(Some(&path)).unwrap();
        assert_eq!(
            config.background_pool.as_ref().unwrap()[0],
            dir.path().join("a.png")
        );
        assert_eq!(config.idle_pool, Some(vec![]));
        assert!(!config.idle_enabled);
        assert!(config.idle_reuse_background);
        config.save(&path).unwrap();
        assert_eq!(
            super::Config::load(Some(&path)).unwrap().slideshow_seconds,
            12
        );
    }
    #[test]
    fn media_defaults_create_folders_and_preserve_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = super::Config::default();
        config.prepare_media(dir.path(), None).unwrap();
        assert_eq!(config.background, Some(dir.path().join("midias/bloqueio")));
        assert!(dir.path().join("midias/ocioso/videos").is_dir());
        let custom = dir.path().join("custom.mp4");
        config.background = Some(custom.clone());
        config.prepare_media(dir.path(), None).unwrap();
        assert_eq!(config.background, Some(custom.clone()));
        let mut themed = super::Config::default();
        themed.prepare_media(dir.path(), Some(&custom)).unwrap();
        assert!(themed.background.is_none());
    }

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
