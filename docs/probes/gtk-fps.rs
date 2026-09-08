//! DESCARTAVEL: mede FPS real em janela GTK e reporta o renderer do GSK.
use gtk::{glib, prelude::*};
use std::{cell::Cell, rc::Rc, time::Instant};

const MODE: &str = "DECKLOCK_PROBE_MODE"; // fullscreen | overlay | static

fn main() {
    let app = gtk::Application::builder().application_id("dev.decklock.probe").build();
    app.connect_activate(|app| {
        let mode = std::env::var(MODE).unwrap_or_else(|_| "overlay".into());
        let area = gtk::DrawingArea::new();
        if mode == "small" {
            area.set_size_request(480, 270);
            area.set_halign(gtk::Align::Start);
            area.set_valign(gtk::Align::Start);
        } else {
            area.set_hexpand(true);
            area.set_vexpand(true);
        }
        let t = Rc::new(Cell::new(0.0f64));
        let draw_t = t.clone();
        let mode_draw = mode.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let (w, h, t) = (w as f64, h as f64, draw_t.get());
            if mode_draw != "static" && mode_draw != "small" {
                // fundo: gradiente de tela cheia redesenhado por quadro
                let g = gtk::cairo::LinearGradient::new(0.0, 0.0, w, h);
                g.add_color_stop_rgb(0.0, 0.05, 0.06 + 0.04 * t.sin(), 0.12);
                g.add_color_stop_rgb(1.0, 0.02, 0.03, 0.08 + 0.03 * (t * 0.3).sin());
                let _ = cr.set_source(&g);
                let _ = cr.paint();
            }
            if mode_draw != "fullscreen" {  // small tambem desenha a camada vetorial
                // camada vetorial: particulas + curva
                for i in 0..200 {
                    let f = i as f64;
                    cr.set_source_rgba(0.8, 0.85, 1.0, 0.10 + 0.20 * ((t + f).sin().abs()));
                    cr.arc(w * (0.5 + 0.45 * (t * 0.11 + f * 0.37).sin()),
                           h * (0.5 + 0.45 * (t * 0.13 + f * 0.61).cos()),
                           1.5 + 2.5 * ((f * 0.9).sin().abs()), 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
                cr.set_line_width(2.0);
                cr.set_source_rgba(0.55, 0.75, 0.95, 0.35);
                for s in 0..600 {
                    let p = s as f64 / 600.0 * std::f64::consts::TAU;
                    let (x, y) = (w * (0.5 + 0.4 * (3.0 * p + t * 0.2).sin()), h * (0.5 + 0.4 * (2.0 * p).sin()));
                    if s == 0 { cr.move_to(x, y); } else { cr.line_to(x, y); }
                }
                let _ = cr.stroke();
            }
        });
        let window = gtk::ApplicationWindow::builder().application(app)
            .default_width(1920).default_height(1080).child(&area).build();
        let frames = Rc::new(Cell::new(0u32));
        let start = Instant::now();
        let (fc, aw, win) = (frames.clone(), area.clone(), window.clone());
        let mode_tick = mode.clone();
        area.add_tick_callback(move |_, _| {
            t.set(start.elapsed().as_secs_f64());
            fc.set(fc.get() + 1);
            aw.queue_draw();
            if start.elapsed().as_secs_f64() >= 4.0 {
                let renderer = win.native().and_then(|n| n.renderer())
                    .map(|r| r.type_().name().to_string()).unwrap_or("?".into());
                println!("{:12} {:5.1} fps reais   renderer={renderer}", mode_tick, fc.get() as f64 / 4.0);
                win.close();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        window.present();
    });
    app.run_with_args::<&str>(&[]);
}
