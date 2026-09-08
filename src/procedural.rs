//! Built-in procedural media IDs and per-item settings shared by every pool.
use crate::animation::{Animation, Effect};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ITEMS: [&str; 5] = ["starfield", "particles", "lissajous", "matrix", "doom-fire"];
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
                ..Default::default()
            },
            doom_fire: Parameters {
                color: "#ffcc66".into(),
                background: "#080304".into(),
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
