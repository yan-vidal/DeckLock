//! Per-media draft editor. Apply updates shared settings; the main Save persists them.
use crate::{
    i18n::I18n,
    procedural::{Parameters, Presets},
};
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

pub fn open(
    parent: &gtk::ApplicationWindow,
    id: &str,
    presets: Rc<RefCell<Presets>>,
    strings: Rc<I18n>,
    changed: impl Fn(Parameters) + 'static,
) -> gtk::Window {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .destroy_with_parent(true)
        .title(strings.text("procedural-configure"))
        .default_width(440)
        .modal(false)
        .build();
    window.set_widget_name("procedural-editor");
    window.add_css_class("settings");
    crate::window_chrome::install(&window);
    let (_, help_key) = crate::help::controls(&window, strings.clone());
    window.add_controller(help_key);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for setter in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        setter(&body, 20);
    }
    window.set_child(Some(&body));
    body.append(&gtk::Label::new(Some(
        &strings.text(&format!("animation-{id}")),
    )));
    let current = presets.borrow().get(id).clone();
    fn row(body: &gtk::Box, label: &str, child: &impl IsA<gtk::Widget>) {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let title = gtk::Label::new(Some(label));
        title.set_hexpand(true);
        title.set_xalign(0.);
        line.append(&title);
        line.append(child);
        body.append(&line);
    }
    fn number(value: f64, min: f64, max: f64, step: f64, name: &str) -> gtk::SpinButton {
        let spin = gtk::SpinButton::with_range(min, max, step);
        spin.set_value(value);
        spin.set_widget_name(name);
        spin
    }
    let density = number(current.density as f64, 1., 300., 1., "procedural-density");
    density.set_sensitive(!matches!(id, "lissajous" | "doom-fire"));
    row(&body, &strings.text("animation-density"), &density);
    let speed = number(current.speed, 0.01, 2., 0.01, "procedural-speed");
    speed.set_digits(2);
    row(&body, &strings.text("animation-speed"), &speed);
    let fps = number(current.fps as f64, 1., 30., 1., "procedural-fps");
    row(&body, &strings.text("animation-fps"), &fps);
    let seed = number(
        current.seed as f64,
        0.,
        u32::MAX as f64,
        1.,
        "procedural-seed",
    );
    seed.set_sensitive(id != "lissajous");
    row(&body, &strings.text("animation-seed"), &seed);
    let color = gtk::Entry::new();
    color.set_text(&current.color);
    color.set_widget_name("procedural-color");
    row(&body, &strings.text("animation-color"), &color);
    let background = gtk::Entry::new();
    background.set_text(&current.background);
    background.set_widget_name("procedural-background-color");
    row(
        &body,
        &strings.text("procedural-background-color"),
        &background,
    );
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_widget_name("procedural-status");
    body.append(&status);
    let note = gtk::Label::new(Some(&strings.text("procedural-save-note")));
    note.set_wrap(true);
    body.append(&note);
    let apply = gtk::Button::with_label(&strings.text("procedural-apply"));
    apply.set_widget_name("procedural-apply");
    body.append(&apply);
    let changed: Rc<dyn Fn(Parameters)> = Rc::new(changed);
    let controls = (
        density.clone(),
        speed.clone(),
        fps.clone(),
        seed.clone(),
        color.clone(),
        background.clone(),
    );
    let read: Rc<dyn Fn() -> Result<Parameters, String>> = Rc::new(move || {
        let value = Parameters {
            density: controls.0.value_as_int() as u32,
            speed: controls.1.value(),
            fps: controls.2.value_as_int() as u32,
            seed: controls.3.value() as u32,
            color: controls.4.text().to_string(),
            background: controls.5.text().to_string(),
        };
        value.validate()?;
        Ok(value)
    });
    let update: Rc<dyn Fn()> = Rc::new({
        let read = read.clone();
        let changed = changed.clone();
        let status = status.clone();
        move || match read() {
            Ok(value) => {
                status.set_text("");
                changed(value);
            }
            Err(error) => status.set_text(&error),
        }
    });
    for spin in [density, speed, fps, seed] {
        let update = update.clone();
        spin.connect_value_changed(move |_| update());
    }
    for entry in [color, background] {
        let update = update.clone();
        entry.connect_changed(move |_| update());
    }
    let accepted = Rc::new(std::cell::Cell::new(false));
    let rollback = changed.clone();
    let kept = accepted.clone();
    window.connect_close_request(move |_| {
        if !kept.get() {
            rollback(current.clone());
        }
        glib::Propagation::Proceed
    });
    let weak = window.downgrade();
    let id = id.to_string();
    apply.connect_clicked(move |_| {
        let value = match read() {
            Ok(value) => value,
            Err(error) => {
                status.set_text(&error);
                return;
            }
        };
        presets.borrow_mut().set(&id, value.clone());
        changed(value);
        accepted.set(true);
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    window.present();
    window
}
