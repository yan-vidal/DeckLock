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
        self.submit.set_sensitive(!busy);
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
    if path.is_file() {
        return Some(path.into());
    }
    let mut files: Vec<_> = std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some(
                    "png"
                        | "jpg"
                        | "jpeg"
                        | "webp"
                        | "avif"
                        | "bmp"
                        | "mp4"
                        | "mkv"
                        | "webm"
                        | "mov"
                )
            )
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return None;
    }
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    Some(files.swap_remove(seed % files.len()))
}

fn set_background(
    container: &gtk::Box,
    path: Option<&Path>,
    stream: &RefCell<Option<crate::media::Playback>>,
) {
    stream.borrow_mut().take();
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let Some(path) = path.and_then(media_file) else {
        return;
    };
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Cover);
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
    container.append(&picture);
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

fn refresh_keys(keys: &Keys, model: &Keyboard, strings: &I18n) {
    for (button, key) in keys.iter() {
        let label = match key.normal {
            "Space" => strings.text("space"),
            "Shift" => "⇧".into(),
            "Caps" => "⇪".into(),
            "Backspace" => "⌫".into(),
            "Enter" => "↵".into(),
            _ => model.label(key),
        };
        button.set_label(&label);
        if (key.normal == "Shift" && model.shift) || (key.normal == "Caps" && model.caps) {
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
    let close = gtk::Button::with_label(&settings.strings.text("close-keyboard"));
    close.set_halign(gtk::Align::End);
    close.set_focusable(false);
    let weak = container.downgrade();
    close.connect_clicked(move |_| {
        if let Some(c) = weak.upgrade() {
            c.set_visible(false);
        }
    });
    container.append(&close);
    let grid = gtk::Grid::builder()
        .column_spacing(4)
        .row_spacing(4)
        .column_homogeneous(true)
        .row_homogeneous(true)
        .build();
    let mut keys = Vec::new();
    for (row, keyrow) in keyboard::rows().into_iter().enumerate() {
        let mut col = 0;
        for key in keyrow {
            let button = gtk::Button::new();
            button.add_css_class("key");
            button.set_focusable(false);
            button.set_size_request(
                (34.0 * settings.theme.layout.keyboard_scale) as i32,
                (32.0 * settings.theme.layout.keyboard_scale) as i32,
            );
            grid.attach(&button, col, row as i32, key.width, 1);
            col += key.width;
            keys.push((button, key));
        }
    }
    container.append(&grid);
    let keys = Rc::new(keys);
    let model = Rc::new(RefCell::new(Keyboard::default()));
    refresh_keys(&keys, &model.borrow(), &settings.strings);
    for (button, key) in keys.iter() {
        let (key, keys, model, entry, settings) = (
            key.clone(),
            Rc::downgrade(&keys),
            model.clone(),
            entry.downgrade(),
            settings.clone(),
        );
        button.connect_clicked(move |_| {
            let (Some(keys), Some(entry)) = (keys.upgrade(), entry.upgrade()) else {
                return;
            };
            let action = model.borrow_mut().press(&key);
            edit_entry(&entry, action);
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
    let pads = RefCell::new([None, None]);
    let triggers = RefCell::new([false, false]);
    let event_handler = Rc::new(move |event| {
        let Some(container) = weak.upgrade() else {
            return;
        };
        match event {
            ControllerEvent::Pad { side, x, y } if container.is_visible() => {
                let idx = if side == Side::Left { 0 } else { 1 };
                let px = (x.clamp(-32767, 32767) as f64 / 65534.0 + 0.5) * grid.width() as f64;
                let py = (0.5 - y.clamp(-32767, 32767) as f64 / 65534.0) * grid.height() as f64;
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
                if pressed {
                    if let Some(side) = side {
                        if let Some(i) = pads.borrow()[side] {
                            keys[i].0.emit_clicked();
                        }
                    } else if name == "B" {
                        container.set_visible(false);
                    } else if (name == "LB" || name == "RB")
                        && let Some((button, _)) =
                            keys.iter().find(|(_, key)| key.normal == "Shift")
                    {
                        button.emit_clicked();
                    }
                } else if name == "LPADTOUCH" || name == "RPADTOUCH" {
                    pads.borrow_mut()[if name == "LPADTOUCH" { 0 } else { 1 }] = None;
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
                eprintln!("Controller: {error}");
                status.set_text(&settings.strings.text("controller-unavailable"));
            }
            ControllerEvent::Disconnected => {
                pads.replace([None, None]);
                triggers.replace([false, false]);
                for (b, _) in keys.iter() {
                    b.remove_css_class("key-hover");
                }
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
    let background_path = settings
        .config
        .background
        .as_deref()
        .or(settings.theme.background.as_deref());
    set_background(&background, background_path, &stream);
    let close_stream = stream.clone();
    window.connect_destroy(move |_| {
        close_stream.borrow_mut().take();
    });
    let veil = gtk::Box::new(gtk::Orientation::Vertical, 0);
    veil.set_widget_name("veil");
    overlay.add_overlay(&veil);
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
    outer.set_hexpand(true);
    outer.set_vexpand(true);
    veil.append(&outer);
    if settings.preview {
        let banner = gtk::Label::new(Some(&settings.strings.text("preview-banner")));
        banner.set_widget_name("preview-banner");
        banner.set_wrap(true);
        outer.append(&banner);
    }
    let power = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    power.set_widget_name("power");
    power.set_halign(gtk::Align::End);
    for (label, command) in [
        ("suspend", "suspend"),
        ("restart", "reboot"),
        ("shutdown", "poweroff"),
    ] {
        let button = gtk::Button::with_label(&settings.strings.text(label));
        if settings.preview {
            button.set_sensitive(false);
            button.set_tooltip_text(Some(&settings.strings.text("preview-power")));
        } else {
            button.connect_clicked(move |_| {
                if let Err(err) = std::process::Command::new("systemctl").arg(command).spawn() {
                    eprintln!("Power action failed: {err}");
                }
            });
        }
        power.append(&button);
    }
    outer.append(&power);
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();
    outer.append(&scrolled);
    let center = gtk::Box::new(gtk::Orientation::Vertical, 12);
    center.set_valign(gtk::Align::Center);
    scrolled.set_child(Some(&center));
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
    content.set_halign(match layout.alignment {
        Alignment::Start => gtk::Align::Start,
        Alignment::Center => gtk::Align::Center,
        Alignment::End => gtk::Align::End,
    });
    for set in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        set(&content, layout.padding);
    }
    center.append(&content);
    let clock_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    clock_box.set_visible(layout.clock_visible);
    let clock = gtk::Label::new(None);
    clock.set_widget_name("clock");
    let date = gtk::Label::new(None);
    date.set_widget_name("date");
    clock_box.append(&clock);
    clock_box.append(&date);
    content.append(&clock_box);
    let update_clock = {
        let clock = clock.downgrade();
        let date = date.downgrade();
        let date_format = settings.strings.text("date-format");
        move || {
            let (Some(clock), Some(date)) = (clock.upgrade(), date.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            if let Ok(now) = glib::DateTime::now_local() {
                clock.set_text(&now.format("%H:%M").unwrap_or_default());
                date.set_text(&now.format(&date_format).unwrap_or_default());
            }
            glib::ControlFlow::Continue
        }
    };
    update_clock();
    glib::timeout_add_seconds_local(1, update_clock);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&form);
    let avatar = gtk::Image::from_icon_name("avatar-default-symbolic");
    if let Some(home) = std::env::var_os("HOME") {
        let face = PathBuf::from(home).join(".face");
        if face.is_file() {
            avatar.set_from_file(Some(face));
        }
    }
    avatar.set_pixel_size(72);
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
    let submit = gtk::Button::with_label(&settings.strings.text("unlock"));
    submit.set_widget_name("submit");
    row.append(&entry);
    row.append(&submit);
    form.append(&row);
    form.append(&caps);
    form.append(&status);
    let (keyboard, controller_event) = build_keyboard(&entry, &settings, &status);
    center.append(&keyboard);
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
        caps.set_visible(modifiers.contains(gdk::ModifierType::LOCK_MASK));
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
        let is_idle = settings_idle.config.idle_seconds > 0
            && idle_activity.get().elapsed().as_secs() >= settings_idle.config.idle_seconds as u64;
        if idle.replace(is_idle) != is_idle {
            if let Some(form) = weak_form.upgrade() {
                form.set_visible(!is_idle);
            }
            if is_idle && let Some(k) = weak_keyboard.upgrade() {
                k.set_visible(false);
            }
            let normal = settings_idle
                .config
                .background
                .as_deref()
                .or(settings_idle.theme.background.as_deref());
            set_background(
                &background,
                if is_idle {
                    settings_idle.config.idle_background.as_deref().or(normal)
                } else {
                    normal
                },
                &stream,
            );
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
