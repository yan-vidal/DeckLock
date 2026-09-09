//! DESCARTAVEL: mede o custo de desenhar fundos procedurais em Cairo a 1080p.
use gtk::cairo::{Context, Format, ImageSurface};
use std::time::Instant;

const W: i32 = 1920;
const H: i32 = 1080;

fn gradient_drift(cr: &Context, t: f64) {
    let g = gtk::cairo::LinearGradient::new(0.0, 0.0, W as f64, H as f64);
    g.add_color_stop_rgb(0.0, 0.05, 0.06 + 0.04 * t.sin(), 0.12);
    g.add_color_stop_rgb(0.5, 0.10 + 0.05 * (t * 0.7).cos(), 0.08, 0.18);
    g.add_color_stop_rgb(1.0, 0.02, 0.03, 0.08 + 0.03 * (t * 0.3).sin());
    cr.set_source(&g).unwrap();
    cr.paint().unwrap();
}

fn particles(cr: &Context, t: f64, count: usize) {
    for i in 0..count {
        let f = i as f64;
        let x = (W as f64) * (0.5 + 0.45 * (t * 0.11 + f * 0.37).sin());
        let y = (H as f64) * (0.5 + 0.45 * (t * 0.13 + f * 0.61).cos());
        let r = 1.5 + 2.5 * ((f * 0.9).sin().abs());
        cr.set_source_rgba(0.8, 0.85, 1.0, 0.10 + 0.20 * ((t + f).sin().abs()));
        cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
        cr.fill().unwrap();
    }
}

fn lissajous(cr: &Context, t: f64, steps: usize) {
    cr.set_line_width(2.0);
    cr.set_source_rgba(0.55, 0.75, 0.95, 0.35);
    for s in 0..steps {
        let p = s as f64 / steps as f64 * std::f64::consts::TAU;
        let x = (W as f64) * (0.5 + 0.4 * (3.0 * p + t * 0.2).sin());
        let y = (H as f64) * (0.5 + 0.4 * (2.0 * p).sin());
        if s == 0 { cr.move_to(x, y); } else { cr.line_to(x, y); }
    }
    cr.stroke().unwrap();
}

fn bench(name: &str, frames: u32, draw: impl Fn(&Context, f64)) {
    let surface = ImageSurface::create(Format::ARgb32, W, H).unwrap();
    let cr = Context::new(&surface).unwrap();
    draw(&cr, 0.0); // aquece
    let start = Instant::now();
    for f in 0..frames {
        draw(&cr, f as f64 / 60.0);
    }
    let per = start.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!("{name:34} {per:6.2} ms/frame   {:5.0} fps teoricos   {:4.1}% de um quadro a 60Hz",
             1000.0 / per, per / 16.67 * 100.0);
}

fn main() {
    println!("Cairo software, {W}x{H}, ARgb32\n");
    bench("gradiente a deriva", 120, |cr, t| gradient_drift(cr, t));
    bench("gradiente + 200 particulas", 120, |cr, t| { gradient_drift(cr, t); particles(cr, t, 200); });
    bench("gradiente + 800 particulas", 60, |cr, t| { gradient_drift(cr, t); particles(cr, t, 800); });
    bench("gradiente + particulas + curva", 60, |cr, t| {
        gradient_drift(cr, t); particles(cr, t, 200); lissajous(cr, t, 600);
    });
    bench("so a curva (600 segmentos)", 120, |cr, t| lissajous(cr, t, 600));
}
