//! Built-in procedural media IDs and per-item settings shared by every pool.
use crate::animation::{Animation, Effect};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ITEMS: [&str; 3] = ["starfield", "particles", "lissajous"];
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
            speed: 0.3,
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
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Presets {
    pub starfield: Parameters,
    pub particles: Parameters,
    pub lissajous: Parameters,
}
impl Presets {
    pub fn get(&self, id: &str) -> &Parameters {
        match id {
            "particles" => &self.particles,
            "lissajous" => &self.lissajous,
            _ => &self.starfield,
        }
    }
    pub fn set(&mut self, id: &str, value: Parameters) {
        match id {
            "starfield" => self.starfield = value,
            "particles" => self.particles = value,
            "lissajous" => self.lissajous = value,
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
