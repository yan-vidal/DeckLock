//! Bundled palette adaptations; custom theme directories retain precedence.
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    bg: String,
    surface: String,
    raised: String,
    text: String,
    muted: String,
    accent: String,
}
#[derive(Deserialize)]
struct Presets {
    presets: Vec<Preset>,
}
pub fn presets() -> Vec<Preset> {
    toml::from_str::<Presets>(include_str!("../themes/presets.toml"))
        .expect("Built-in palette data")
        .presets
}
pub fn colors(id: &str, keyboard: bool) -> Result<String, String> {
    let p = presets()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("Unknown theme preset: {id}"))?;
    let mut css = format!(
        "@define-color settings_bg {};\n@define-color settings_surface {};\n@define-color settings_raised {};\n@define-color settings_text {};\n@define-color settings_muted {};\n@define-color accent {};\n@define-color foreground {};\n",
        p.bg, p.surface, p.raised, p.text, p.muted, p.accent, p.text
    );
    if keyboard {
        css += &format!(
            "@define-color keyboard_background {};\n@define-color keyboard_dark {};\n@define-color keyboard_text {};\n@define-color keyboard_hilight {};\n@define-color keyboard_pressed {};\n",
            p.surface, p.raised, p.text, p.raised, p.accent
        );
    }
    Ok(css)
}
pub fn settings_css() -> &'static str {
    include_str!("../themes/settings.css")
}

#[cfg(test)]
mod tests {
    #[test]
    fn presets_load_for_lock_and_reject_unknown_ids() {
        let mut ids = std::collections::HashSet::new();
        for preset in super::presets() {
            assert!(ids.insert(preset.id.clone()));
            let config = crate::config::Config {
                theme_preset: preset.id,
                ..Default::default()
            };
            let theme = crate::config::Theme::from_config(&config).unwrap();
            assert_eq!(theme.name, preset.name);
            assert!(theme.css.contains("window.settings"));
            assert!(theme.css.contains("keyboard_background"));
        }
        assert!(super::colors("invalid", true).is_err());
    }
}
