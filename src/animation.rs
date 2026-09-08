//! Bounded procedural media. No scripts, files, decoder or authentication access.
use gtk::{cairo, gdk, glib, prelude::*};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    #[default]
    None,
    Starfield,
    Particles,
    Lissajous,
    Matrix,
    DoomFire,
}
impl Effect {
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Starfield => "starfield",
            Self::Particles => "particles",
            Self::Lissajous => "lissajous",
            Self::Matrix => "matrix",
            Self::DoomFire => "doom-fire",
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
    pub background: String,
    pub seed: u32,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            effect: Effect::None,
            density: 120,
            speed: 1.0,
            fps: 30,
            color: "#b4befe".into(),
            background: "#141725".into(),
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
        parse_color(&self.background)?;
        self.rgb().map(|_| ())
    }
    fn rgb(&self) -> Result<(f64, f64, f64), String> {
        parse_color(&self.color)
    }
}
fn parse_color(color: &str) -> Result<(f64, f64, f64), String> {
    let s = color.as_bytes();
    if s.len() != 7 || s[0] != b'#' || !s[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err("Animation color must be #RRGGBB".into());
    }
    let value = u32::from_str_radix(&color[1..], 16).map_err(|e| e.to_string())?;
    Ok((
        ((value >> 16) & 255) as f64 / 255.,
        ((value >> 8) & 255) as f64 / 255.,
        (value & 255) as f64 / 255.,
    ))
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
    let (br, bg, bb) = parse_color(&config.background)?;
    cr.set_source_rgb(br, bg, bb);
    cr.paint().map_err(|e| e.to_string())?;
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
        Effect::Matrix => matrix(&cr, config, t, w, h, (r, g, b))?,
        Effect::DoomFire => fire(&cr, config, t, w, h, (r, g, b))?,
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

// Tiny original bitmap glyphs avoid font discovery and keep previews repeatable.
fn matrix(
    cr: &cairo::Context,
    config: &Animation,
    t: f64,
    w: f64,
    h: f64,
    color: (f64, f64, f64),
) -> Result<(), String> {
    const GLYPHS: [[u8; 7]; 16] = [
        [14, 17, 19, 21, 25, 17, 14],
        [4, 12, 4, 4, 4, 4, 14],
        [14, 17, 1, 2, 4, 8, 31],
        [30, 1, 1, 14, 1, 1, 30],
        [2, 6, 10, 18, 31, 2, 2],
        [31, 16, 16, 30, 1, 1, 30],
        [14, 16, 16, 30, 17, 17, 14],
        [31, 1, 2, 4, 8, 8, 8],
        [14, 17, 17, 14, 17, 17, 14],
        [14, 17, 17, 15, 1, 1, 14],
        [14, 17, 17, 31, 17, 17, 17],
        [30, 17, 17, 30, 17, 17, 30],
        [14, 17, 16, 16, 16, 17, 14],
        [30, 17, 17, 17, 17, 17, 30],
        [31, 16, 16, 30, 16, 16, 31],
        [31, 16, 16, 30, 16, 16, 16],
    ];
    let columns = ((w / 10.) * (config.density as f64 / 120.)).clamp(4., 80.) as u32;
    let cell = (w / columns as f64).max(1.);
    let step = cell * 1.5;
    let rows = (h / step).ceil() as u32;
    let tick = (t * 8.) as u32;
    for column in 0..columns {
        let tail = 5. + random(config.seed, column * 3) * 12.;
        let head = (t * (5. + random(config.seed, column * 3 + 1) * 7.)
            + random(config.seed, column * 3 + 2) * (rows as f64 + tail))
            .rem_euclid(rows as f64 + tail);
        for row in 0..rows {
            let age = (head - row as f64).rem_euclid(rows as f64 + tail);
            if age > tail {
                continue;
            }
            let alpha = (1. - age / tail).powi(2);
            let lead = if age < 1. { 0.8 } else { 0. };
            cr.set_source_rgba(
                color.0 + (1. - color.0) * lead,
                color.1 + (1. - color.1) * lead,
                color.2 + (1. - color.2) * lead,
                alpha,
            );
            let index = (random(
                config.seed,
                column
                    .wrapping_mul(997)
                    .wrapping_add(row * 31)
                    .wrapping_add(tick),
            ) * 15.) as usize;
            for (y, bits) in GLYPHS[index].iter().enumerate() {
                for x in 0..5 {
                    if bits & (1 << (4 - x)) != 0 {
                        cr.rectangle(
                            column as f64 * cell + x as f64 * cell / 6.,
                            row as f64 * step + y as f64 * cell / 6.,
                            cell / 6. * 0.85,
                            cell / 6. * 0.85,
                        );
                    }
                }
            }
            cr.fill().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
// Recompute the finite propagation history: no frame-rate-dependent mutable simulation.
fn fire(
    cr: &cairo::Context,
    config: &Animation,
    t: f64,
    w: f64,
    h: f64,
    color: (f64, f64, f64),
) -> Result<(), String> {
    const W: usize = 96;
    const H: usize = 48;
    let mut heat = vec![0u8; W * H];
    let mut next = heat.clone();
    let tick = (t * 18.) as u32;
    for step in 0..H as u32 {
        for x in 0..W {
            heat[(H - 1) * W + x] = 36;
            next[(H - 1) * W + x] = 36;
        }
        for y in 0..H - 1 {
            for x in 0..W {
                let noise = random(
                    config.seed,
                    tick.wrapping_add(step)
                        .wrapping_mul(7919)
                        .wrapping_add((y * W + x) as u32),
                );
                let shift = (noise * 5.) as usize;
                let from = (x + W + shift - 2) % W;
                next[y * W + x] = heat[(y + 1) * W + from].saturating_sub((noise * 3.) as u8);
            }
        }
        std::mem::swap(&mut heat, &mut next);
    }
    for y in 0..H {
        for x in 0..W {
            let v = heat[y * W + x] as f64 / 36.;
            if v == 0. {
                continue;
            }
            let hot = ((v - 0.55) / 0.45).clamp(0., 1.);
            let red = (v * 2.5).min(1.);
            let green = ((v - 0.25) * 1.7).clamp(0., 1.);
            let blue = ((v - 0.75) * 4.).clamp(0., 1.);
            cr.set_source_rgb(
                red * (1. - hot) + color.0 * hot,
                green * (1. - hot) + color.1 * hot,
                blue * (1. - hot) + color.2 * hot,
            );
            cr.rectangle(
                x as f64 * w / W as f64,
                y as f64 * h / H as f64,
                w / W as f64 + 0.1,
                h / H as f64 + 0.1,
            );
            cr.fill().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn texture(
    config: &Animation,
    time: f64,
    width: i32,
    height: i32,
) -> Result<gdk::MemoryTexture, String> {
    let mut surface = render(config, time, width, height)?;
    let (w, h, stride) = (surface.width(), surface.height(), surface.stride());
    let data = surface.data().map_err(|e| e.to_string())?;
    let bytes = glib::Bytes::from_owned(data.to_vec());
    #[cfg(target_endian = "little")]
    let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
    #[cfg(target_endian = "big")]
    let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
    Ok(gdk::MemoryTexture::new(
        w,
        h,
        format,
        &bytes,
        stride as usize,
    ))
}

pub fn widget(config: Animation) -> gtk::Picture {
    widget_with_stats(config, None)
}
pub fn widget_with_stats(
    config: Animation,
    metrics: Option<crate::preview_stats::Metrics>,
) -> gtk::Picture {
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
    // The tick callback is Fn, so the accumulator needs interior mutability too.
    let metrics = RefCell::new(metrics);
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
        let cpu_start = metrics
            .borrow()
            .is_some()
            .then(|| crate::preview_stats::cpu_seconds(true))
            .flatten();
        let result = texture(
            &config,
            (now - start) as f64 / 1_000_000.,
            (width * scale).max(1.) as i32,
            (height * scale).max(1.) as i32,
        );
        let Ok(texture) = result else {
            return glib::ControlFlow::Break;
        };
        let bytes = texture.width() as usize * texture.height() as usize * 4;
        picture.set_paintable(Some(&texture));
        if let Some(metrics) = metrics.borrow_mut().as_mut() {
            let spent = cpu_start
                .zip(crate::preview_stats::cpu_seconds(true))
                .map(|(before, after)| (after - before).max(0.))
                .unwrap_or(0.);
            metrics.record(spent, bytes);
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
    fn scenes_are_repeatable_animated_and_opaque() {
        for effect in [
            Effect::Starfield,
            Effect::Particles,
            Effect::Lissajous,
            Effect::Matrix,
            Effect::DoomFire,
        ] {
            assert_eq!(frame(effect, 2.), frame(effect, 2.));
            assert_ne!(frame(effect, 2.), frame(effect, 3.));
            assert!(frame(effect, 2.).chunks_exact(4).all(|p| p[3] == 255));
        }
        assert!(frame(Effect::None, 0.).chunks_exact(4).all(|p| p[3] == 255));
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
