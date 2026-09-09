//! Built-in procedural media IDs and per-item settings shared by every pool.
use crate::animation::{Animation, Effect};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ITEMS: [&str; 6] = [
    "starfield",
    "particles",
    "lissajous",
    "matrix",
    "doom-fire",
    "aurora",
];
pub fn id(path: &Path) -> Option<&str> {
    let id = path.to_str()?.strip_prefix("procedural:")?;
    ITEMS.contains(&id).then_some(id)
}
pub fn path(id: &str) -> PathBuf {
    PathBuf::from(format!("procedural:{id}"))
}
pub fn effect(id: &str) -> Effect {
    match id {
        "starfield" => Effect::Starfield,
        "particles" => Effect::Particles,
        "lissajous" => Effect::Lissajous,
        "matrix" => Effect::Matrix,
        "doom-fire" => Effect::DoomFire,
        "aurora" => Effect::Aurora,
        _ => Effect::None,
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Parameters {
    pub density: u32,
    pub speed: f64,
    pub fps: u32,
    pub color: String,
    pub background: String,
    pub seed: u32,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            density: 120,
            speed: 1.0,
            fps: 30,
            color: "#b4befe".into(),
            background: "#141725".into(),
            seed: 1,
        }
    }
}
impl Parameters {
    pub fn animation(&self, id: &str) -> Animation {
        Animation {
            effect: effect(id),
            density: self.density,
            speed: self.speed,
            fps: self.fps,
            color: self.color.clone(),
            background: self.background.clone(),
            seed: self.seed,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.animation("starfield").validate()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Presets {
    pub starfield: Parameters,
    pub particles: Parameters,
    pub lissajous: Parameters,
    pub matrix: Parameters,
    #[serde(rename = "doom-fire")]
    pub doom_fire: Parameters,
    pub aurora: Parameters,
}
impl Default for Presets {
    fn default() -> Self {
        Self {
            starfield: Default::default(),
            particles: Default::default(),
            lissajous: Default::default(),
            matrix: Parameters {
                color: "#38f277".into(),
                background: "#020806".into(),
                density: 220,
                ..Default::default()
            },
            doom_fire: Parameters {
                color: "#ffcc66".into(),
                background: "#080304".into(),
                ..Default::default()
            },
            aurora: Parameters {
                color: "#7ef0c0".into(),
                background: "#080f1c".into(),
                density: 168,
                speed: 0.6,
                ..Default::default()
            },
        }
    }
}
impl Presets {
    pub fn get(&self, id: &str) -> &Parameters {
        match id {
            "particles" => &self.particles,
            "lissajous" => &self.lissajous,
            "matrix" => &self.matrix,
            "doom-fire" => &self.doom_fire,
            "aurora" => &self.aurora,
            _ => &self.starfield,
        }
    }
    pub fn set(&mut self, id: &str, value: Parameters) {
        match id {
            "starfield" => self.starfield = value,
            "particles" => self.particles = value,
            "lissajous" => self.lissajous = value,
            "matrix" => self.matrix = value,
            "doom-fire" => self.doom_fire = value,
            "aurora" => self.aurora = value,
            _ => {}
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        for id in ITEMS {
            self.get(id).validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Names are built as `animation-<id>`, so a missing entry silently renders the
    /// raw id in the library, the editor and the viewer title.
    #[test]
    fn every_item_is_named_routed_and_stored_in_both_locales() {
        for locale in ["en-US", "pt-BR"] {
            let strings = crate::i18n::I18n::new(Some(locale), None).unwrap();
            for item in ITEMS {
                let key = format!("animation-{item}");
                assert_ne!(strings.text(&key), key, "{locale} is missing {key}");
            }
        }
        let mut presets = Presets::default();
        for item in ITEMS {
            assert_ne!(effect(item), Effect::None, "{item} has no effect");
            assert_eq!(id(&path(item)), Some(item), "{item} does not round-trip");
            let mut value = presets.get(item).clone();
            value.seed = 4242;
            presets.set(item, value);
            assert_eq!(presets.get(item).seed, 4242, "{item} is not stored apart");
        }
    }
}
