//! Bounded procedural overlays. No scripts, files, decoder or authentication access.
use gtk::{cairo, gdk, glib, prelude::*};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    #[default]
    None,
    Starfield,
    Particles,
    Lissajous,
}
impl Effect {
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Starfield => "starfield",
            Self::Particles => "particles",
            Self::Lissajous => "lissajous",
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Animation {
    pub effect: Effect,
    pub density: u32,
    pub speed: f64,
    pub fps: u32,
    pub color: String,
    pub seed: u32,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            effect: Effect::None,
            density: 120,
            speed: 0.3,
            fps: 30,
            color: "#b4befe".into(),
            seed: 1,
        }
    }
}
impl Animation {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=300).contains(&self.density)
            || !(1..=30).contains(&self.fps)
            || !self.speed.is_finite()
            || !(0.01..=2.0).contains(&self.speed)
        {
            return Err("Animation requires density 1..300, fps 1..30 and speed 0.01..2".into());
        }
        self.rgb().map(|_| ())
    }
    fn rgb(&self) -> Result<(f64, f64, f64), String> {
        let s = self.color.as_bytes();
        if s.len() != 7 || s[0] != b'#' || !s[1..].iter().all(u8::is_ascii_hexdigit) {
            return Err("Animation color must be #RRGGBB".into());
        }
        let value = u32::from_str_radix(&self.color[1..], 16).map_err(|e| e.to_string())?;
        Ok((
            ((value >> 16) & 255) as f64 / 255.,
            ((value >> 8) & 255) as f64 / 255.,
            (value & 255) as f64 / 255.,
        ))
    }
}
// Stable hashing: the scene depends only on seed, element index and injected time.
fn random(seed: u32, index: u32) -> f64 {
    let mut x = seed.wrapping_add(index.wrapping_mul(0x9e3779b9));
    x = (x ^ (x >> 16)).wrapping_mul(0x85ebca6b);
    x = (x ^ (x >> 13)).wrapping_mul(0xc2b2ae35);
    (x ^ (x >> 16)) as f64 / u32::MAX as f64
}
pub fn render(
    config: &Animation,
    time: f64,
    width: i32,
    height: i32,
) -> Result<cairo::ImageSurface, String> {
    config.validate()?;
    if !time.is_finite() || time < 0. || !(1..=640).contains(&width) || !(1..=640).contains(&height)
    {
        return Err("Invalid animation frame dimensions or time".into());
    }
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| e.to_string())?;
    let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
    let (r, g, b) = config.rgb()?;
    let t = (time % 100000.) * config.speed;
    let (w, h) = (width as f64, height as f64);
    match config.effect {
        Effect::None => {}
        Effect::Starfield | Effect::Particles => {
            for i in 0..config.density {
                let a = random(config.seed, i * 3);
                let b0 = random(config.seed, i * 3 + 1);
                let c = random(config.seed, i * 3 + 2);
                let (x, y, size, alpha) = if config.effect == Effect::Starfield {
                    let z = 1. - (c + t * 0.12).fract();
                    (
                        w * (0.5 + (a - 0.5) / z.max(0.03)),
                        h * (0.5 + (b0 - 0.5) / z.max(0.03)),
                        0.4 + (1. - z) * 1.5,
                        (1. - z) * 0.85,
                    )
                } else {
                    (
                        w * ((a + t * 0.012).fract()),
                        h * ((b0 + t * 0.008 + (t * 0.2 + a * TAU).sin() * 0.03).rem_euclid(1.)),
                        0.6 + c * 1.4,
                        0.2 + c * 0.35,
                    )
                };
                cr.set_source_rgba(r, g, b, alpha);
                cr.arc(x, y, size, 0., TAU);
                cr.fill().map_err(|e| e.to_string())?;
            }
        }
        Effect::Lissajous => {
            for trail in 0..8 {
                let phase = t * 0.25 - trail as f64 * 0.04;
                cr.set_source_rgba(r, g, b, 0.4 * (1. - trail as f64 / 8.));
                cr.set_line_width(0.8);
                for i in 0..=360 {
                    let p = i as f64 / 360. * TAU;
                    let x = w * (0.5 + 0.42 * (3. * p + phase).sin());
                    let y = h * (0.5 + 0.42 * (2. * p + phase * 0.4).sin());
                    if i == 0 {
                        cr.move_to(x, y);
                    } else {
                        cr.line_to(x, y);
                    }
                }
                cr.stroke().map_err(|e| e.to_string())?;
            }
        }
    }
    drop(cr);
    Ok(surface)
}

pub fn widget(config: Animation) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_widget_name("procedural-background");
    picture.set_can_target(false);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Fill);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    if config.effect == Effect::None {
        return picture;
    }
    let origin = Cell::new(None);
    let last = Cell::new(0i64);
    picture.add_tick_callback(move |picture, clock| {
        let now = clock.frame_time();
        if now - last.get() < 1_000_000 / config.fps.max(1) as i64 {
            return glib::ControlFlow::Continue;
        }
        last.set(now);
        let start = origin.get().unwrap_or_else(|| {
            origin.set(Some(now));
            now
        });
        let width = picture.width().max(1) as f64;
        let height = picture.height().max(1) as f64;
        let scale = (640. / width.max(height)).min(1.);
        let result = render(
            &config,
            (now - start) as f64 / 1_000_000.,
            (width * scale).max(1.) as i32,
            (height * scale).max(1.) as i32,
        );
        let Ok(mut surface) = result else {
            return glib::ControlFlow::Break;
        };
        let (w, h, stride) = (surface.width(), surface.height(), surface.stride());
        if let Ok(data) = surface.data() {
            let bytes = glib::Bytes::from_owned(data.to_vec());
            #[cfg(target_endian = "little")]
            let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
            #[cfg(target_endian = "big")]
            let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
            picture.set_paintable(Some(&gdk::MemoryTexture::new(
                w,
                h,
                format,
                &bytes,
                stride as usize,
            )));
        }
        glib::ControlFlow::Continue
    });
    picture
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(effect: Effect, time: f64) -> Vec<u8> {
        render(
            &Animation {
                effect,
                ..Default::default()
            },
            time,
            320,
            180,
        )
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
    }
    #[test]
    fn scenes_are_repeatable_animated_and_transparent() {
        for effect in [Effect::Starfield, Effect::Particles, Effect::Lissajous] {
            assert_eq!(frame(effect, 2.), frame(effect, 2.));
            assert_ne!(frame(effect, 2.), frame(effect, 3.));
            assert!(frame(effect, 2.).contains(&0));
        }
        assert!(frame(Effect::None, 0.).iter().all(|b| *b == 0));
    }
    #[test]
    fn rejects_unbounded_work_and_invalid_parameters() {
        for text in [
            "fps=0",
            "fps=31",
            "density=301",
            "density=0",
            "speed=nan",
            "speed=3",
            "color='red'",
            "effect='script'",
        ] {
            let parsed = toml::from_str::<Animation>(text);
            assert!(
                parsed.is_err() || parsed.unwrap().validate().is_err(),
                "{text}"
            );
        }
        assert!(render(&Animation::default(), 0., 1920, 1080).is_err());
    }
}
