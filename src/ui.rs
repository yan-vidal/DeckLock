use crate::{
    config::{Alignment, Arrangement, Config, Theme},
    controller::{ControllerEvent, Side},
    i18n::I18n,
    keyboard::{self, Action, Keyboard},
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct Settings {
    pub config: Config,
    pub theme: Theme,
    pub strings: I18n,
    pub preview: bool,
    pub show_keyboard: bool,
    pub start_idle: bool,
    pub username: String,
}

#[derive(Clone)]
pub struct View {
    pub window: gtk::ApplicationWindow,
    pub entry: gtk::Entry,
    pub status: gtk::Label,
    pub submit: gtk::Button,
    pub keyboard: gtk::Box,
    pub controller_event: Rc<dyn Fn(ControllerEvent)>,
    pub activity: Rc<Cell<Instant>>,
}

impl View {
    pub fn busy(&self, busy: bool, message: &str) {
        self.entry.set_sensitive(!busy);
        self.submit
            .set_sensitive(!busy && !self.entry.text().is_empty());
        self.keyboard.set_sensitive(!busy);
        self.status.set_text(message);
    }
}

pub fn apply_css(css: &str) -> Result<(), String> {
    let provider = gtk::CssProvider::new();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let capture = errors.clone();
    provider.connect_parsing_error(move |_, _, err| capture.borrow_mut().push(err.to_string()));
    provider.load_from_string(css);
    if !errors.borrow().is_empty() {
        return Err(errors.borrow().join("; "));
    }
    let display = gdk::Display::default().ok_or("No display")?;
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Ok(())
}

fn media_file(path: &Path) -> Option<PathBuf> {
    let files = crate::library::files(path);
    if files.is_empty() {
        None
    } else {
        Some(files[crate::library::seed() % files.len()].clone())
    }
}

fn set_background(
    container: &gtk::Box,
    path: Option<&Path>,
    stream: &RefCell<Option<crate::media::Playback>>,
) {
    stream.borrow_mut().take();
    let stack = container
        .first_child()
        .and_downcast::<gtk::Stack>()
        .unwrap_or_else(|| {
            let stack = gtk::Stack::new();
            stack.set_hexpand(true);
            stack.set_vexpand(true);
            stack.set_transition_type(gtk::StackTransitionType::Crossfade);
            stack.set_transition_duration(600);
            container.append(&stack);
            stack
        });
    let visible = stack.visible_child();
    let mut child = stack.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if Some(&widget) != visible.as_ref() {
            stack.remove(&widget);
        }
    }
    let Some(path) = path.and_then(media_file) else {
        if let Some(visible) = visible {
            stack.remove(&visible);
        }
        return;
    };
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Fill);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    if matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp4" | "mkv" | "webm" | "mov")
    ) {
        match crate::media::Playback::new(&path, &picture) {
            Ok(media) => {
                stream.replace(Some(media));
            }
            Err(error) => eprintln!("Background video unavailable: {error}"),
        }
    } else {
        picture.set_file(Some(&gio::File::for_path(path)));
    }
    stack.add_child(&picture);
    stack.set_visible_child(&picture);
}

fn edit_entry(entry: &gtk::Entry, action: Action) {
    if !entry.is_sensitive() {
        return;
    }
    match action {
        Action::Insert(text) => {
            if let Some((start, end)) = entry.selection_bounds() {
                entry.delete_text(start, end);
                entry.set_position(start);
            }
            let mut pos = entry.position();
            entry.insert_text(&text, &mut pos);
            entry.set_position(pos);
        }
        Action::Backspace => {
            if let Some((start, end)) = entry.selection_bounds() {
                entry.delete_text(start, end);
                entry.set_position(start);
            } else {
                let pos = entry.position();
                if pos > 0 {
                    entry.delete_text(pos - 1, pos);
                    entry.set_position(pos - 1);
                }
            }
        }
        Action::Submit => entry.emit_activate(),
        _ => {}
    }
}

type Keys = Rc<Vec<(gtk::Button, keyboard::Key)>>;

fn refresh_keys(keys: &Keys, model: &Keyboard, _strings: &I18n) {
    for (button, key) in keys.iter() {
        let label = match key.normal {
            "Space" => "␣".into(),
            "Shift" if model.shift_locked => "⇪".into(),
            "Shift" => "⇧".into(),
            "Caps" => "⇪".into(),
            "AltGr" => "Alt".into(),
            "Backspace" => "←".into(),
            "Enter" => "↲".into(),
            _ => model.label(key),
        };
        button.set_label(&label);
        if (key.normal == "Shift" && model.shift)
            || (key.normal == "Caps" && model.caps)
            || (key.normal == "AltGr" && model.altgr)
        {
            button.add_css_class("modifier-active");
        } else {
            button.remove_css_class("modifier-active");
        }
    }
}

fn build_keyboard(
    entry: &gtk::Entry,
    settings: &Rc<Settings>,
    status: &gtk::Label,
) -> (gtk::Box, Rc<dyn Fn(ControllerEvent)>) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.set_widget_name("keyboard");
    container.set_halign(gtk::Align::Center);
    container.set_visible(settings.show_keyboard);
    let grid = gtk::Fixed::new();
    // Python's mouse keyboard uses the original 800x405 SVG at 62%.
    let ghost = settings.config.controller_socket.is_some();
    if ghost {
        container.add_css_class("ghost");
    }
    let scale = (if ghost { 1.0 } else { 0.62 }) * settings.theme.layout.keyboard_scale;
    grid.set_size_request(
        (800.0 * scale).round() as i32,
        (405.0 * scale).round() as i32,
    );
    let mut keys = Vec::new();
    let geometry = Rc::new(keyboard::geometry());
    for (key, x, y, width, height) in geometry.iter().cloned() {
        let button = gtk::Button::new();
        button.add_css_class("key");
        if matches!(
            key.normal,
            "]" | "ç"
                | "Enter"
                | "-"
                | "'"
                | ","
                | "."
                | ";"
                | "Shift"
                | "AltGr"
                | "Backspace"
                | "Space"
                | "´"
                | "["
                | "~"
                | "="
        ) {
            button.add_css_class("key-dark");
        }
        button.set_focusable(false);
        button.set_opacity(if ghost { 0.0 } else { 1.0 });
        button.set_size_request(
            (width * scale).round() as i32,
            (height * scale).round() as i32,
        );
        grid.put(&button, x * scale, y * scale);
        keys.push((button, key));
    }
    let keyboard_overlay = gtk::Overlay::new();
    keyboard_overlay.set_child(Some(&grid));
    let hint = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hint.set_widget_name("keyboard-idle-hint");
    hint.set_size_request((400.0 * scale) as i32, 2);
    hint.set_halign(gtk::Align::Center);
    hint.set_valign(gtk::Align::End);
    hint.set_margin_bottom(3);
    hint.set_can_target(false);
    hint.set_visible(ghost);
    keyboard_overlay.add_overlay(&hint);
    container.append(&keyboard_overlay);
    let keys = Rc::new(keys);
    let mut keyboard_model = Keyboard::default();
    if settings.config.system_keyboard {
        let display = entry.display();
        // GDK exposes the compositor's keymap; codes are Linux evdev + XKB offset 8.
        let codes = [
            ("1", 2),
            ("2", 3),
            ("3", 4),
            ("4", 5),
            ("5", 6),
            ("6", 7),
            ("7", 8),
            ("8", 9),
            ("9", 10),
            ("0", 11),
            ("-", 12),
            ("=", 13),
            ("q", 16),
            ("w", 17),
            ("e", 18),
            ("r", 19),
            ("t", 20),
            ("y", 21),
            ("u", 22),
            ("i", 23),
            ("o", 24),
            ("p", 25),
            ("´", 26),
            ("[", 27),
            ("a", 30),
            ("s", 31),
            ("d", 32),
            ("f", 33),
            ("g", 34),
            ("h", 35),
            ("j", 36),
            ("k", 37),
            ("l", 38),
            ("ç", 39),
            ("~", 40),
            ("'", 41),
            ("]", 43),
            ("z", 44),
            ("x", 45),
            ("c", 46),
            ("v", 47),
            ("b", 48),
            ("n", 49),
            ("m", 50),
            (",", 51),
            (".", 52),
            (";", 53),
        ];
        for (name, code) in codes {
            let mut levels = Vec::new();
            if let Some(mapped) = display.map_keycode(code + 8) {
                for level in 0..4 {
                    let Some((_, symbol)) = mapped
                        .iter()
                        .find(|(k, _)| k.group() == 0 && k.level() == level)
                    else {
                        break;
                    };
                    let dead = match *symbol {
                        gdk::Key::dead_acute => Some('´'),
                        gdk::Key::dead_grave => Some('`'),
                        gdk::Key::dead_circumflex => Some('^'),
                        gdk::Key::dead_tilde => Some('~'),
                        gdk::Key::dead_diaeresis => Some('¨'),
                        _ => None,
                    };
                    let Some(character) = dead.or_else(|| symbol.to_unicode()) else {
                        break;
                    };
                    levels.push((character.to_string(), dead.is_some()));
                }
            }
            if !levels.is_empty() {
                keyboard_model.system_levels.insert(name, levels);
            }
        }
    }
    let model = Rc::new(RefCell::new(keyboard_model));
    refresh_keys(&keys, &model.borrow(), &settings.strings);
    for (button, key) in keys.iter() {
        let (key, keys, model, entry, settings) = (
            key.clone(),
            Rc::downgrade(&keys),
            model.clone(),
            entry.downgrade(),
            settings.clone(),
        );
        let keyboard = container.downgrade();
        button.connect_clicked(move |_| {
            let (Some(keys), Some(entry)) = (keys.upgrade(), entry.upgrade()) else {
                return;
            };
            let action = model.borrow_mut().press(&key);
            edit_entry(&entry, action);
            if let Some(keyboard) = keyboard.upgrade() {
                if model.borrow().shift_locked || model.borrow().caps {
                    keyboard.add_css_class("caps-active");
                } else {
                    keyboard.remove_css_class("caps-active");
                }
            }
            refresh_keys(&keys, &model.borrow(), &settings.strings);
        });
    }
    let owned_keys = keys.clone();
    container.connect_destroy(move |_| {
        for (button, _) in owned_keys.iter() {
            button.set_sensitive(false);
        }
    });
    let weak = container.downgrade();
    let status = status.clone();
    let settings = settings.clone();
    let points = Rc::new(RefCell::new([None::<(f64, f64)>, None]));
    let fade: Rc<dyn Fn()> = {
        let keys = keys.clone();
        let points = points.clone();
        let hint = hint.downgrade();
        let container = container.downgrade();
        Rc::new(move || {
            if !container
                .upgrade()
                .is_some_and(|c| c.has_css_class("ghost"))
            {
                return;
            }
            for ((button, _), (_, x, y, width, height)) in keys.iter().zip(geometry.iter()) {
                let alpha = points
                    .borrow()
                    .iter()
                    .flatten()
                    .map(|(px, py)| {
                        let distance = (x + width / 2.0 - px).hypot(y + height / 2.0 - py);
                        let t = (1.0 - distance / 180.0).clamp(0.0, 1.0);
                        t * t * (3.0 - 2.0 * t)
                    })
                    .fold(0.0, f64::max);
                button.set_opacity(alpha);
            }
            if let Some(hint) = hint.upgrade() {
                hint.set_visible(points.borrow().iter().all(Option::is_none));
            }
        })
    };
    let clear_points = points.clone();
    let clear_fade = fade.clone();
    container.connect_visible_notify(move |_| {
        clear_points.replace([None, None]);
        clear_fade();
    });
    let touching = Rc::new(RefCell::new([false, false]));
    let reset_touching = touching.clone();
    let pads = Rc::new(RefCell::new([None, None]));
    let triggers = Rc::new(RefCell::new([false, false]));
    let reset_pads = pads.clone();
    let reset_triggers = triggers.clone();
    let reset_model = model.clone();
    let reset_keys = Rc::downgrade(&keys);
    let reset_settings = settings.clone();
    container.connect_visible_notify(move |container| {
        reset_touching.replace([false, false]);
        reset_pads.replace([None, None]);
        reset_triggers.replace([false, false]);
        if !container.is_visible() {
            let mut model = reset_model.borrow_mut();
            model.held_shift = false;
            model.held_altgr = false;
            model.shift = false;
            model.shift_locked = false;
            container.remove_css_class("caps-active");
            model.altgr = false;
            if let Some(keys) = reset_keys.upgrade() {
                refresh_keys(&keys, &model, &reset_settings.strings);
            }
        }
    });
    let fallback_scale = 0.62 * settings.theme.layout.keyboard_scale;
    let fallback_hint = hint.downgrade();
    let fallback = move |container: &gtk::Box, grid: &gtk::Fixed, keys: &Keys| {
        container.remove_css_class("ghost");
        if let Some(hint) = fallback_hint.upgrade() {
            hint.set_visible(false);
        }
        grid.set_size_request(
            (800.0 * fallback_scale).round() as i32,
            (405.0 * fallback_scale).round() as i32,
        );
        for ((button, _), (_, x, y, width, height)) in keys.iter().zip(keyboard::geometry()) {
            button.set_opacity(1.0);
            button.set_size_request(
                (width * fallback_scale).round() as i32,
                (height * fallback_scale).round() as i32,
            );
            grid.move_(button, x * fallback_scale, y * fallback_scale);
        }
    };
    let event_handler = Rc::new(move |event| {
        let Some(container) = weak.upgrade() else {
            return;
        };
        match event {
            ControllerEvent::Pad { side, x, y } if container.is_visible() => {
                let idx = if side == Side::Left { 0 } else { 1 };
                if !touching.borrow()[idx] {
                    return;
                }
                // The two pads cover overlapping halves of the original SVG,
                // exactly as LIMIT_LPAD / LIMIT_RPAD in the Python keyboard.
                let (left, width) = if side == Side::Left {
                    (13.05, 425.33261)
                } else {
                    (384.23, 388.67133)
                };
                let px = (left + (x.clamp(-32767, 32767) as f64 / 65534.0 + 0.5) * width) * scale;
                let py = (5.0 + (0.5 - y.clamp(-32767, 32767) as f64 / 65534.0) * 393.0) * scale;
                points.borrow_mut()[idx] = Some((px / scale, py / scale));
                fade();
                pads.borrow_mut()[idx] = keys.iter().position(|(b, _)| {
                    b.compute_bounds(&grid).is_some_and(|a| {
                        px >= a.x() as f64
                            && px < (a.x() + a.width()) as f64
                            && py >= a.y() as f64
                            && py < (a.y() + a.height()) as f64
                    })
                });
                for (i, (b, _)) in keys.iter().enumerate() {
                    if pads.borrow().contains(&Some(i)) {
                        b.add_css_class("key-hover");
                    } else {
                        b.remove_css_class("key-hover");
                    }
                }
            }
            ControllerEvent::Button { name, pressed } if container.is_visible() => {
                let side = match name.as_str() {
                    "LPADPRESS" => Some(0),
                    "RPADPRESS" => Some(1),
                    _ => None,
                };
                if name == "LPADTOUCH" || name == "RPADTOUCH" {
                    touching.borrow_mut()[if name == "LPADTOUCH" { 0 } else { 1 }] = pressed;
                }
                if name == "LGRIP" || name == "RGRIP" {
                    let mut state = model.borrow_mut();
                    if name == "LGRIP" {
                        state.held_shift = pressed;
                        state.shift = pressed || state.shift_locked;
                    } else {
                        state.held_altgr = pressed;
                        state.altgr = pressed;
                    }
                    refresh_keys(&keys, &state, &settings.strings);
                } else if pressed {
                    if let Some(side) = side {
                        if let Some(i) = pads.borrow()[side] {
                            keys[i].0.emit_clicked();
                        }
                    } else if matches!(name.as_str(), "B" | "X" | "Y" | "C") {
                        container.set_visible(false);
                    } else if (name == "LB" || name == "RB")
                        && let Some((button, _)) = keys.iter().find(|(_, key)| {
                            key.normal == if name == "LB" { "Backspace" } else { "Space" }
                        })
                    {
                        button.emit_clicked();
                    }
                } else if name == "LPADTOUCH" || name == "RPADTOUCH" {
                    let side = if name == "LPADTOUCH" { 0 } else { 1 };
                    pads.borrow_mut()[side] = None;
                    points.borrow_mut()[side] = None;
                    fade();
                    for (i, (b, _)) in keys.iter().enumerate() {
                        if !pads.borrow().contains(&Some(i)) {
                            b.remove_css_class("key-hover");
                        }
                    }
                }
            }
            ControllerEvent::Trigger { side, value } if container.is_visible() => {
                let side = if side == Side::Left { 0 } else { 1 };
                let pressed = value > 128;
                if pressed
                    && !triggers.borrow()[side]
                    && let Some(i) = pads.borrow()[side]
                {
                    keys[i].0.emit_clicked();
                }
                triggers.borrow_mut()[side] = pressed;
            }
            ControllerEvent::Error(error) => {
                fallback(&container, &grid, &keys);
                eprintln!("Controller: {error}");
                status.set_text(&settings.strings.text("controller-unavailable"));
            }
            ControllerEvent::Disconnected => {
                touching.replace([false, false]);
                pads.replace([None, None]);
                points.replace([None, None]);
                fade();
                triggers.replace([false, false]);
                for (b, _) in keys.iter() {
                    b.remove_css_class("key-hover");
                }
                fallback(&container, &grid, &keys);
                status.set_text(&settings.strings.text("controller-unavailable"));
            }
            _ => {}
        }
    });
    (container, event_handler)
}

pub fn build(
    app: &gtk::Application,
    settings: Rc<Settings>,
    on_submit: Rc<dyn Fn(Zeroizing<String>)>,
) -> View {
    let window = gtk::ApplicationWindow::builder()
        .title(settings.strings.text(if settings.preview {
            "preview-title"
        } else {
            "lock-title"
        }))
        .default_width(1000)
        .default_height(720)
        .build();
    // Session-lock destroys/unrealizes surfaces itself. Registering those windows
    // with GtkApplication makes GTK 4.22's removal handler access a gone surface.
    // The lock mode holds the application explicitly; only preview uses its
    // ordinary application-window lifecycle.
    if settings.preview {
        window.set_application(Some(app));
    }
    window.add_css_class("decklock");
    if !settings.preview {
        window.set_title(Some("DeckLock"));
        window.set_decorated(false);
    }
    let overlay = gtk::Overlay::new();
    window.set_child(Some(&overlay));
    let background = gtk::Box::new(gtk::Orientation::Vertical, 0);
    background.set_widget_name("background");
    overlay.set_child(Some(&background));
    let stream = Rc::new(RefCell::new(None));
    let mut normal_selection = crate::library::Selection::new(
        crate::library::pool(&settings.config, &settings.theme, false),
        crate::library::seed(),
    );
    let mut idle_selection = crate::library::Selection::new(
        crate::library::pool(&settings.config, &settings.theme, true),
        crate::library::seed(),
    );
    set_background(&background, normal_selection.current(), &stream);
    let close_stream = stream.clone();
    window.connect_destroy(move |_| {
        close_stream.borrow_mut().take();
    });
    let veil = gtk::Box::new(gtk::Orientation::Vertical, 0);
    veil.set_widget_name("veil");
    overlay.add_overlay(&veil);
    // Separate overlays preserve the Python geometry: the veil fills the
    // monitor, power stays in the upper-right corner, and the keyboard reserves
    // its own bottom strip without moving the background or its gradient.
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.set_hexpand(true);
    outer.set_vexpand(true);
    overlay.add_overlay(&outer);
    if settings.preview {
        let banner = gtk::Label::new(Some(&settings.strings.text("preview-banner")));
        banner.set_widget_name("preview-banner");
        banner.set_max_width_chars(24);
        banner.set_ellipsize(gtk::pango::EllipsizeMode::End);
        banner.set_halign(gtk::Align::Start);
        banner.set_valign(gtk::Align::Start);
        banner.set_can_target(false);
        overlay.add_overlay(&banner);
    }
    let power = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    power.set_widget_name("power");
    power.set_halign(gtk::Align::End);
    power.set_valign(gtk::Align::Start);
    power.set_margin_top(12);
    power.set_margin_end(12);
    for (label, command, icon) in [
        ("suspend", "suspend", "system-suspend-symbolic"),
        ("hibernate", "hibernate", "weather-clear-night-symbolic"),
        ("restart", "reboot", "system-reboot-symbolic"),
        ("shutdown", "poweroff", "system-shutdown-symbolic"),
    ] {
        let button = gtk::Button::new();
        let image = gtk::Image::from_icon_name(icon);
        // Some icon themes draw the power glyph smaller inside the same canvas.
        // Equal slots preserve button sizes while giving that glyph more room.
        image.set_pixel_size(if command == "poweroff" { 32 } else { 24 });
        image.set_halign(gtk::Align::Center);
        image.set_valign(gtk::Align::Center);
        let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        slot.set_size_request(32, 32);
        slot.set_halign(gtk::Align::Center);
        slot.set_valign(gtk::Align::Center);
        image.set_hexpand(true);
        slot.append(&image);
        button.set_child(Some(&slot));
        let title = settings.strings.text(label);
        button.set_tooltip_text(Some(&title));
        if settings.preview {
            // Keep hover/tooltips for theme previews; no power handler is attached.
            button.set_tooltip_text(Some(&format!(
                "{title}\n{}",
                settings.strings.text("preview-power")
            )));
        } else {
            button.connect_clicked(move |_| {
                if let Err(err) = std::process::Command::new("systemctl").arg(command).spawn() {
                    eprintln!("Power action failed: {err}");
                }
            });
        }
        power.append(&button);
    }
    overlay.add_overlay(&power);
    let layout = &settings.theme.layout;
    let content = gtk::Box::new(
        if layout.arrangement == Arrangement::Horizontal {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        },
        layout.spacing,
    );
    content.set_widget_name("content");
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_halign(gtk::Align::Fill);
    let content_align = match layout.alignment {
        Alignment::Start => gtk::Align::Start,
        Alignment::Center => gtk::Align::Center,
        Alignment::End => gtk::Align::End,
    };
    for set in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        set(&content, layout.padding);
    }
    outer.append(&content);
    let clock_region = gtk::Box::new(gtk::Orientation::Vertical, 0);
    clock_region.set_vexpand(true);
    clock_region.set_halign(content_align);
    let clock_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    clock_box.set_widget_name("clock-block");
    clock_box.set_vexpand(true);
    clock_box.set_valign(gtk::Align::End);
    clock_region.append(&clock_box);
    clock_box.set_visible(layout.clock_visible);
    let clock = gtk::Label::new(None);
    clock.set_widget_name("clock");
    let date = gtk::Label::new(None);
    date.set_widget_name("date");
    clock_box.append(&clock);
    clock_box.append(&date);
    content.append(&clock_region);
    let update_clock = {
        let clock = clock.downgrade();
        let date = date.downgrade();
        let clock_settings = settings.clone();
        move || {
            let (Some(clock), Some(date)) = (clock.upgrade(), date.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            if let Ok(now) = glib::DateTime::now_local() {
                clock.set_text(&now.format("%H:%M").unwrap_or_default());
                let date_format = clock_settings
                    .strings
                    .text("date-format")
                    .replace(
                        "%A",
                        &clock_settings
                            .strings
                            .text(&format!("weekday-{}", now.day_of_week())),
                    )
                    .replace(
                        "%B",
                        &clock_settings
                            .strings
                            .text(&format!("month-{}", now.month())),
                    );
                let text = now.format(&date_format).unwrap_or_default();
                let mut chars = text.chars();
                let text = chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default();
                date.set_text(&text);
            }
            glib::ControlFlow::Continue
        }
    };
    update_clock();
    glib::timeout_add_seconds_local(1, update_clock);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_widget_name("credentials");
    form.set_halign(content_align);
    form.set_valign(gtk::Align::Center);
    form.set_vexpand(true);
    form.set_margin_top(60);
    form.set_margin_bottom(80);
    content.append(&form);
    let face = std::env::var_os("HOME").and_then(|home| {
        [".face", ".face.icon"].into_iter().find_map(|name| {
            gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(
                PathBuf::from(&home).join(name),
                96,
                96,
                false,
            )
            .ok()
        })
    });
    let avatar: gtk::Widget = if let Some(face) = face {
        let drawing = gtk::DrawingArea::new();
        drawing.set_draw_func(move |_, context, width, height| {
            use gdk::prelude::GdkCairoContextExt;
            let size = width.min(height) as f64;
            context.arc(
                width as f64 / 2.0,
                height as f64 / 2.0,
                size / 2.0,
                0.0,
                std::f64::consts::TAU,
            );
            context.clip();
            context.set_source_pixbuf(
                &face,
                (width - face.width()) as f64 / 2.0,
                (height - face.height()) as f64 / 2.0,
            );
            let _ = context.paint();
        });
        drawing.upcast()
    } else {
        let image = gtk::Image::from_icon_name("avatar-default");
        image.set_pixel_size(96);
        image.upcast()
    };
    avatar.set_size_request(108, 108);
    avatar.set_halign(gtk::Align::Center);
    avatar.set_widget_name("avatar");
    avatar.set_visible(layout.avatar_visible);
    form.append(&avatar);
    let username = gtk::Label::new(Some(&settings.username));
    username.set_widget_name("username");
    form.append(&username);
    let entry = gtk::Entry::builder()
        .visibility(false)
        .input_purpose(gtk::InputPurpose::Password)
        .placeholder_text(settings.strings.text("password"))
        .max_length(511)
        .width_chars(22)
        .build();
    entry.set_widget_name("password");
    gtk::prelude::EntryExt::set_alignment(&entry, 0.5);
    entry.set_icon_from_icon_name(
        gtk::EntryIconPosition::Primary,
        Some("view-reveal-symbolic"),
    );
    entry.set_icon_tooltip_text(
        gtk::EntryIconPosition::Primary,
        Some(&settings.strings.text("show-password")),
    );
    entry.set_icon_from_icon_name(
        gtk::EntryIconPosition::Secondary,
        Some("input-keyboard-symbolic"),
    );
    entry.set_icon_tooltip_text(
        gtk::EntryIconPosition::Secondary,
        Some(&settings.strings.text("virtual-keyboard")),
    );
    let status = gtk::Label::new(None);
    status.set_widget_name("status");
    status.set_wrap(true);
    let caps = gtk::Label::new(Some(&settings.strings.text("caps-lock")));
    caps.set_widget_name("caps");
    caps.set_visible(false);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_halign(gtk::Align::Center);
    let submit = gtk::Button::new();
    let submit_icon = gtk::Image::from_icon_name("go-next-symbolic");
    submit_icon.set_pixel_size(24);
    submit.set_child(Some(&submit_icon));
    submit.set_tooltip_text(Some(&settings.strings.text("unlock")));
    submit.set_sensitive(false);
    let weak_submit = submit.downgrade();
    entry.connect_changed(move |entry| {
        if let Some(submit) = weak_submit.upgrade() {
            submit.set_sensitive(entry.is_sensitive() && !entry.text().is_empty());
        }
    });
    submit.set_widget_name("submit");
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let size_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    size_group.add_widget(&spacer);
    size_group.add_widget(&submit);
    row.append(&spacer);
    row.append(&entry);
    row.append(&submit);
    form.append(&caps);
    form.append(&row);
    form.append(&status);
    let (keyboard, controller_event) = build_keyboard(&entry, &settings, &status);
    let physical_caps = Rc::new(Cell::new(false));
    let caps_label = caps.downgrade();
    let physical = physical_caps.clone();
    keyboard.connect_notify_local(Some("css-classes"), move |keyboard, _| {
        if let Some(label) = caps_label.upgrade() {
            label.set_visible(physical.get() || keyboard.has_css_class("caps-active"));
        }
    });
    keyboard.set_valign(gtk::Align::End);
    keyboard.set_margin_bottom(16);
    overlay.add_overlay(&keyboard);
    // Compact mode is triggered by visibility, so mouse, signal and controller
    // toggles all keep exactly the same space available for the password.
    let compact: Rc<dyn Fn(&gtk::Box)> = Rc::new({
        let clock_region = clock_region.downgrade();
        let avatar = avatar.downgrade();
        let username = username.downgrade();
        let form = form.downgrade();
        let content = content.downgrade();
        let padding = layout.padding;
        let scale = layout.keyboard_scale;
        let clock_visible = layout.clock_visible;
        let avatar_visible = layout.avatar_visible;
        move |keyboard: &gtk::Box| {
            let active = keyboard.is_visible() && !keyboard.has_css_class("ghost");
            if let Some(content) = content.upgrade() {
                content.set_margin_bottom(if active {
                    (if keyboard.height() < 50 {
                        (405.0 * scale).round() as i32
                    } else {
                        keyboard.height()
                    }) + padding
                } else {
                    padding
                });
            }
            if let Some(clock) = clock_region.upgrade() {
                clock.set_visible(!active && clock_visible);
            }
            if let Some(avatar) = avatar.upgrade() {
                avatar.set_visible(!active && avatar_visible);
            }
            if let Some(username) = username.upgrade() {
                username.set_visible(!active);
            }
            if let Some(form) = form.upgrade() {
                form.set_margin_top(if active { 0 } else { 60 });
                form.set_margin_bottom(if active { 0 } else { 80 });
            }
        }
    });
    compact(&keyboard);
    let visible_compact = compact.clone();
    keyboard.connect_visible_notify(move |keyboard| visible_compact(keyboard));
    keyboard.connect_notify_local(Some("css-classes"), move |keyboard, _| compact(keyboard));
    let weak_keyboard = keyboard.downgrade();
    let strings = settings.clone();
    entry.connect_icon_press(move |entry, position| {
        if position == gtk::EntryIconPosition::Primary {
            let visible = !gtk::prelude::EntryExt::is_visible(entry);
            entry.set_visibility(visible);
            entry.set_icon_tooltip_text(
                position,
                Some(&strings.strings.text(if visible {
                    "hide-password"
                } else {
                    "show-password"
                })),
            );
        } else if let Some(k) = weak_keyboard.upgrade() {
            k.set_visible(!k.is_visible());
        }
    });
    let weak_entry = entry.downgrade();
    submit.connect_clicked(move |_| {
        if let Some(e) = weak_entry.upgrade() {
            e.emit_activate();
        }
    });
    let preview = settings.preview;
    let status_submit = status.clone();
    let strings = settings.clone();
    entry.connect_activate(move |entry| {
        if !entry.is_sensitive() || entry.text().is_empty() {
            return;
        }
        let password = Zeroizing::new(entry.text().to_string());
        entry.set_text("");
        entry.set_visibility(false);
        if preview {
            status_submit.set_text(&strings.strings.text("preview-submit"));
        } else {
            on_submit(password);
        }
    });
    let activity = Rc::new(Cell::new(Instant::now()));
    let key_events = gtk::EventControllerKey::new();
    let activity_key = activity.clone();
    let weak_window = window.downgrade();
    let weak_keyboard = keyboard.downgrade();
    let weak_form = form.downgrade();
    key_events.connect_key_pressed(move |_, key, _, modifiers| {
        activity_key.set(Instant::now());
        physical_caps.set(modifiers.contains(gdk::ModifierType::LOCK_MASK));
        caps.set_visible(
            physical_caps.get()
                || weak_keyboard
                    .upgrade()
                    .is_some_and(|k| k.has_css_class("caps-active")),
        );
        if let Some(form) = weak_form.upgrade() {
            form.set_visible(true);
        }
        if key == gdk::Key::Escape {
            if let Some(k) = weak_keyboard.upgrade().filter(|k| k.is_visible()) {
                k.set_visible(false);
            } else if preview && let Some(w) = weak_window.upgrade() {
                w.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_events);
    let motion = gtk::EventControllerMotion::new();
    let activity_motion = activity.clone();
    motion.connect_motion(move |_, _, _| activity_motion.set(Instant::now()));
    window.add_controller(motion);
    let gesture = gtk::GestureClick::new();
    let activity_click = activity.clone();
    gesture.connect_pressed(move |_, _, _, _| activity_click.set(Instant::now()));
    window.add_controller(gesture);
    let weak_window = window.downgrade();
    let weak_form = form.downgrade();
    let weak_keyboard = keyboard.downgrade();
    let idle = Rc::new(Cell::new(false));
    let weak_clock_box = clock_box.downgrade();
    let weak_power = power.downgrade();
    let mut changed_at = Instant::now();
    let settings_idle = settings.clone();
    if settings.start_idle {
        activity.set(
            Instant::now()
                .checked_sub(Duration::from_secs(
                    settings.config.idle_seconds.max(1) as u64 + 1,
                ))
                .unwrap_or_else(Instant::now),
        );
    }
    let idle_activity = activity.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        if weak_window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        let is_idle = settings_idle.config.idle_enabled
            && settings_idle.config.idle_seconds > 0
            && idle_activity.get().elapsed().as_secs() >= settings_idle.config.idle_seconds as u64;
        if idle.replace(is_idle) != is_idle {
            if let Some(power) = weak_power.upgrade() {
                power.set_visible(!is_idle);
            }
            if let Some(clock) = weak_clock_box.upgrade() {
                clock.set_visible(
                    settings_idle.theme.layout.clock_visible
                        && !(is_idle && settings_idle.config.idle_reuse_background),
                );
                clock.set_valign(if is_idle {
                    gtk::Align::Center
                } else {
                    gtk::Align::End
                });
            }
            if let Some(form) = weak_form.upgrade() {
                form.set_visible(!is_idle);
            }
            if is_idle && let Some(k) = weak_keyboard.upgrade() {
                k.set_visible(false);
            }
            if !settings_idle.config.idle_reuse_background && idle_selection.current().is_some() {
                let selected = if is_idle {
                    &idle_selection
                } else {
                    &normal_selection
                };
                set_background(&background, selected.current(), &stream);
                changed_at = Instant::now();
            }
        }
        let separate_idle = is_idle
            && !settings_idle.config.idle_reuse_background
            && idle_selection.current().is_some();
        let interval = if separate_idle {
            settings_idle.config.idle_slideshow_seconds
        } else {
            settings_idle.config.slideshow_seconds
        };
        let selection = if separate_idle {
            &mut idle_selection
        } else {
            &mut normal_selection
        };
        if changed_at.elapsed().as_secs() >= interval as u64 {
            if selection.advance() {
                set_background(&background, selection.current(), &stream);
            }
            changed_at = Instant::now();
        }
        glib::ControlFlow::Continue
    });
    let entry_focus = entry.downgrade();
    window.connect_map(move |_| {
        if let Some(entry) = entry_focus.upgrade() {
            entry.grab_focus();
        }
    });
    if !settings.preview {
        window.connect_close_request(|_| glib::Propagation::Stop);
    }
    View {
        window,
        entry,
        status,
        submit,
        keyboard,
        controller_event,
        activity,
    }
}

#[cfg(test)]
mod media_tests {
    #[test]
    fn media_folder_supports_legacy_subfolders_and_empty_fallback() {
        let dir = tempfile::tempdir().unwrap();
        assert!(super::media_file(dir.path()).is_none());
        std::fs::create_dir(dir.path().join("videos")).unwrap();
        let video = dir.path().join("videos/background.MP4");
        std::fs::write(&video, b"test").unwrap();
        assert_eq!(super::media_file(dir.path()), Some(video));
        std::fs::create_dir(dir.path().join("fake.png")).unwrap();
        assert!(super::media_file(dir.path()).unwrap().is_file());
    }
}
