//! GTK4 settings editor. Previews always run separately with --preview.
use crate::{
    config::{Alignment, Arrangement, Config, Layout, Theme},
    i18n::I18n,
};
use gtk::{gio, prelude::*};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    process::{Child, Command},
    rc::Rc,
};

struct Preview {
    child: Child,
    _files: tempfile::TempDir,
}
impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}

fn path_entry(value: Option<&Path>, name: &str) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_widget_name(name);
    if let Some(path) = value {
        entry.set_text(&path.to_string_lossy());
    }
    entry
}
fn selected_path(entry: &gtk::Entry, base: &Path) -> Option<PathBuf> {
    let text = entry.text();
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        let path = PathBuf::from(text);
        Some(if path.is_absolute() {
            path
        } else {
            base.join(path)
        })
    }
}
fn row(form: &gtk::Box, title: &str, widget: &impl IsA<gtk::Widget>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_width_chars(22);
    label.set_wrap(true);
    row.append(&label);
    widget.set_hexpand(true);
    row.append(widget);
    form.append(&row);
}
fn chooser(
    window: &gtk::ApplicationWindow,
    entry: &gtk::Entry,
    folder: bool,
    strings: &I18n,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(entry);
    let button = gtk::Button::with_label(&strings.text("settings-browse"));
    row.append(&button);
    let weak_window = window.downgrade();
    let entry = entry.downgrade();
    let title = strings.text("settings-browse");
    button.connect_clicked(move |_| {
        let (Some(window), Some(entry)) = (weak_window.upgrade(), entry.upgrade()) else {
            return;
        };
        let dialog = gtk::FileDialog::builder().title(&title).modal(true).build();
        let callback = move |result: Result<gio::File, glib::Error>| {
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                entry.set_text(&path.to_string_lossy());
            }
        };
        if folder {
            dialog.select_folder(Some(&window), None::<&gio::Cancellable>, callback);
        } else {
            dialog.open(Some(&window), None::<&gio::Cancellable>, callback);
        }
    });
    row
}
fn spin(value: f64, min: f64, max: f64, step: f64, name: &str) -> gtk::SpinButton {
    let spin = gtk::SpinButton::with_range(min, max, step);
    spin.set_value(value);
    spin.set_widget_name(name);
    spin
}
struct Choice {
    widget: gtk::DropDown,
    ids: Vec<String>,
}
impl Choice {
    fn active_id(&self) -> Option<String> {
        self.ids.get(self.widget.selected() as usize).cloned()
    }
}
fn combo(values: &[(&str, String)], selected: &str, name: &str) -> Choice {
    let mut ids: Vec<_> = values.iter().map(|(id, _)| id.to_string()).collect();
    let mut labels: Vec<_> = values.iter().map(|(_, label)| label.clone()).collect();
    if !ids.iter().any(|id| id == selected) {
        ids.push(selected.into());
        labels.push(selected.into());
    }
    let widget =
        gtk::DropDown::from_strings(&labels.iter().map(String::as_str).collect::<Vec<_>>());
    widget.set_selected(ids.iter().position(|id| id == selected).unwrap_or(0) as u32);
    widget.set_widget_name(name);
    Choice { widget, ids }
}
fn validate_theme(config: &Config) -> Result<(), String> {
    config.validate()?;
    let theme = Theme::load(config.theme.as_deref())?;
    let provider = gtk::CssProvider::new();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let capture = errors.clone();
    provider.connect_parsing_error(move |_, _, error| capture.borrow_mut().push(error.to_string()));
    provider.load_from_string(&theme.css);
    if !errors.borrow().is_empty() {
        return Err(errors.borrow().join("; "));
    }
    Ok(())
}

pub fn build(
    app: &gtk::Application,
    path: PathBuf,
    locale: Option<&str>,
    executable: PathBuf,
) -> Result<gtk::ApplicationWindow, String> {
    let path = absolute(&path)?;
    let original = Config::load(path.exists().then_some(path.as_path()))?;
    let strings = Rc::new(I18n::new(locale.or(original.locale.as_deref()), None)?);
    let theme = Theme::load(original.theme.as_deref())?;
    let layout = original.layout.as_ref().unwrap_or(&theme.layout);
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(strings.text("settings-title"))
        .default_width(1000)
        .default_height(720)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 16);
    for set in [
        gtk::prelude::WidgetExt::set_margin_top,
        gtk::prelude::WidgetExt::set_margin_bottom,
        gtk::prelude::WidgetExt::set_margin_start,
        gtk::prelude::WidgetExt::set_margin_end,
    ] {
        set(&root, 20);
    }
    window.set_child(Some(&root));
    let title = gtk::Label::new(Some(&strings.text("settings-title")));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    root.append(&title);
    let hint = gtk::Label::new(Some(&strings.text("settings-hint")));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    root.append(&hint);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    root.append(&scroll);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    scroll.set_child(Some(&form));
    let theme_path = path_entry(original.theme.as_deref(), "settings-theme");
    theme_path.set_placeholder_text(Some(&strings.text("settings-default-theme")));
    row(
        &form,
        &strings.text("settings-theme"),
        &chooser(&window, &theme_path, true, &strings),
    );
    let catalog = crate::media_editor::Catalog::new(crate::library::catalog(&original, &theme));
    let background = crate::media_editor::build(
        &window,
        catalog.clone(),
        original
            .background_pool
            .clone()
            .unwrap_or_else(|| crate::library::pool(&original, &theme, false)),
        strings.clone(),
        "settings-background",
    );
    form.append(&gtk::Label::new(Some(&strings.text("settings-background"))));
    form.append(&background.widget);
    let slideshow = spin(
        original.slideshow_seconds as f64,
        1.0,
        86400.0,
        1.0,
        "settings-slideshow",
    );
    row(&form, &strings.text("media-interval"), &slideshow);
    let language = combo(
        &[
            ("auto", strings.text("settings-system")),
            ("pt-BR", "Português (Brasil)".into()),
            ("en-US", "English".into()),
        ],
        original.locale.as_deref().unwrap_or("auto"),
        "settings-language",
    );
    row(&form, &strings.text("settings-language"), &language.widget);
    let alignment = combo(
        &[
            ("start", strings.text("settings-left")),
            ("center", strings.text("settings-center")),
            ("end", strings.text("settings-right")),
        ],
        match layout.alignment {
            Alignment::Start => "start",
            Alignment::Center => "center",
            Alignment::End => "end",
        },
        "settings-alignment",
    );
    row(
        &form,
        &strings.text("settings-alignment"),
        &alignment.widget,
    );
    let arrangement = combo(
        &[
            ("vertical", strings.text("settings-vertical")),
            ("horizontal", strings.text("settings-horizontal")),
        ],
        if layout.arrangement == Arrangement::Vertical {
            "vertical"
        } else {
            "horizontal"
        },
        "settings-arrangement",
    );
    row(
        &form,
        &strings.text("settings-arrangement"),
        &arrangement.widget,
    );
    let spacing = spin(layout.spacing as f64, 0.0, 128.0, 1.0, "settings-spacing");
    row(&form, &strings.text("settings-spacing"), &spacing);
    let padding = spin(layout.padding as f64, 0.0, 256.0, 1.0, "settings-padding");
    row(&form, &strings.text("settings-padding"), &padding);
    let scale = spin(layout.keyboard_scale, 0.5, 2.0, 0.05, "settings-scale");
    scale.set_digits(2);
    row(&form, &strings.text("settings-scale"), &scale);
    let clock = gtk::CheckButton::with_label(&strings.text("settings-clock"));
    clock.set_active(layout.clock_visible);
    clock.set_widget_name("settings-clock");
    form.append(&clock);
    let avatar = gtk::CheckButton::with_label(&strings.text("settings-avatar"));
    avatar.set_active(layout.avatar_visible);
    avatar.set_widget_name("settings-avatar");
    form.append(&avatar);
    let system_keyboard = gtk::CheckButton::with_label(&strings.text("settings-system-keyboard"));
    system_keyboard.set_active(original.system_keyboard);
    form.append(&system_keyboard);
    let disable_idle = gtk::CheckButton::with_label(&strings.text("idle-disable"));
    disable_idle.set_widget_name("settings-disable-idle");
    disable_idle.set_active(!original.idle_enabled);
    form.append(&disable_idle);
    let reuse = gtk::CheckButton::with_label(&strings.text("idle-reuse"));
    reuse.set_widget_name("settings-reuse-background");
    reuse.set_active(original.idle_reuse_background);
    form.append(&reuse);
    let idle = spin(
        original.idle_seconds as f64,
        1.0,
        86400.0,
        30.0,
        "settings-idle",
    );
    row(&form, &strings.text("settings-idle"), &idle);
    let idle_group = gtk::Box::new(gtk::Orientation::Vertical, 8);
    idle_group.set_widget_name("settings-idle-media");
    form.append(&idle_group);
    idle_group.append(&gtk::Label::new(Some(
        &strings.text("settings-idle-background"),
    )));
    let idle_background = crate::media_editor::build(
        &window,
        catalog,
        original
            .idle_pool
            .clone()
            .unwrap_or_else(|| crate::library::pool(&original, &theme, true)),
        strings.clone(),
        "settings-idle-background",
    );
    idle_group.append(&idle_background.widget);
    let idle_slideshow = spin(
        original.idle_slideshow_seconds as f64,
        1.0,
        86400.0,
        1.0,
        "settings-idle-slideshow",
    );
    row(
        &idle_group,
        &strings.text("media-interval"),
        &idle_slideshow,
    );
    let (weak_disable, weak_reuse, weak_group, weak_idle) = (
        disable_idle.downgrade(),
        reuse.downgrade(),
        idle_group.downgrade(),
        idle.downgrade(),
    );
    let update: Rc<dyn Fn()> = Rc::new(move || {
        if let (Some(disable), Some(reuse), Some(group), Some(idle)) = (
            weak_disable.upgrade(),
            weak_reuse.upgrade(),
            weak_group.upgrade(),
            weak_idle.upgrade(),
        ) {
            reuse.set_sensitive(!disable.is_active());
            idle.set_sensitive(!disable.is_active());
            group.set_visible(!disable.is_active() && !reuse.is_active());
        }
    });
    update();
    let changed = update.clone();
    disable_idle.connect_toggled(move |_| changed());
    reuse.connect_toggled(move |_| update());
    let controller = gtk::CheckButton::with_label(&strings.text("settings-controller"));
    controller.set_active(original.controller_socket.is_some());
    form.append(&controller);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::End);
    let preview = gtk::Button::with_label(&strings.text("settings-preview"));
    preview.set_widget_name("settings-preview");
    let save = gtk::Button::with_label(&strings.text("settings-save"));
    save.add_css_class("suggested-action");
    save.set_widget_name("settings-save");
    buttons.append(&preview);
    buttons.append(&save);
    root.append(&buttons);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_selectable(true);
    status.set_widget_name("settings-status");
    root.append(&status);
    let base = path.parent().unwrap().to_path_buf();
    let read: Rc<dyn Fn() -> Result<Config, String>> = Rc::new(move || {
        let mut config = original.clone();
        config.theme = selected_path(&theme_path, &base);
        config.background_pool = Some(background.paths.borrow().clone());
        config.idle_pool = Some(idle_background.paths.borrow().clone());
        config.slideshow_seconds = slideshow.value_as_int() as u32;
        config.idle_slideshow_seconds = idle_slideshow.value_as_int() as u32;
        config.idle_enabled = !disable_idle.is_active();
        config.idle_reuse_background = reuse.is_active();
        config.locale = language
            .active_id()
            .filter(|id| id != "auto")
            .map(|id| id.to_string());
        config.idle_seconds = idle.value_as_int() as u32;
        config.system_keyboard = system_keyboard.is_active();
        config.controller_socket = if controller.is_active() {
            config
                .controller_socket
                .or_else(|| crate::shortcut::config_dir().map(|p| p.join("scc/daemon.socket")))
        } else {
            None
        };
        config.layout = Some(Layout {
            alignment: match alignment.active_id().as_deref() {
                Some("start") => Alignment::Start,
                Some("end") => Alignment::End,
                _ => Alignment::Center,
            },
            arrangement: if arrangement.active_id().as_deref() == Some("horizontal") {
                Arrangement::Horizontal
            } else {
                Arrangement::Vertical
            },
            spacing: spacing.value_as_int(),
            padding: padding.value_as_int(),
            clock_visible: clock.is_active(),
            avatar_visible: avatar.is_active(),
            keyboard_scale: scale.value(),
        });
        validate_theme(&config)?;
        Ok(config)
    });
    let read_save = read.clone();
    let save_status = status.downgrade();
    let saved = strings.text("settings-saved");
    save.connect_clicked(move |_| {
        let result = read_save().and_then(|config| config.save(&path));
        if let Some(status) = save_status.upgrade() {
            status.set_text(&match result {
                Ok(()) => saved.clone(),
                Err(error) => error,
            });
        }
    });
    let child: Rc<RefCell<Option<Preview>>> = Rc::new(RefCell::new(None));
    let cleanup = child.clone();
    window.connect_destroy(move |_| {
        cleanup.borrow_mut().take();
    });
    let preview_status = status.downgrade();
    let previewed = strings.text("settings-preview-open");
    preview.connect_clicked(move |_| {
        let result = (|| -> Result<(), String> {
            let config = read()?;
            let files = tempfile::tempdir().map_err(|e| e.to_string())?;
            let path = files.path().join("config.toml");
            config.save(&path)?;
            child.borrow_mut().take();
            let process = Command::new(&executable)
                .arg("--preview")
                .arg("--config")
                .arg(path)
                .spawn()
                .map_err(|e| e.to_string())?;
            child.replace(Some(Preview {
                child: process,
                _files: files,
            }));
            Ok(())
        })();
        if let Some(status) = preview_status.upgrade() {
            status.set_text(&match result {
                Ok(()) => previewed.clone(),
                Err(error) => error,
            });
        }
    });
    Ok(window)
}

pub fn run(path: PathBuf, locale: Option<String>) -> Result<(), String> {
    gtk::init().map_err(|e| e.to_string())?;
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.Settings"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    app.register(None::<&gio::Cancellable>)
        .map_err(|e| e.to_string())?;
    let window = build(&app, path, locale.as_deref(), executable)?;
    app.connect_activate(move |_| window.present());
    app.run_with_args::<&str>(&[]);
    Ok(())
}
