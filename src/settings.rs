//! GTK4 settings editor. Settings previews update in place without session-lock or controller capture.
use crate::{
    config::{Alignment, Arrangement, Config, Layout, Theme},
    i18n::I18n,
};
use gtk::{gio, prelude::*};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

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
    fn mark(widget: &gtk::Widget) {
        if widget.is::<gtk::Popover>() {
            widget.add_css_class("decklock-menu");
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            mark(&node);
        }
    }
    mark(widget.upcast_ref());
    widget.connect_map(|widget| mark(widget.upcast_ref()));
    Choice { widget, ids }
}
fn validate_theme(config: &Config) -> Result<(), String> {
    config.validate()?;
    let theme = Theme::from_config(config)?;
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

// Translate existing widgets without replacing their values, selection or drafts.
fn translate_widgets(widget: &gtk::Widget, messages: &std::collections::HashMap<String, String>) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>()
        && let Some(text) = messages.get(label.text().as_str())
    {
        label.set_text(text);
    }
    if let Some(text) = widget
        .tooltip_text()
        .and_then(|old| messages.get(old.as_str()).cloned())
    {
        widget.set_tooltip_text(Some(&text));
    }
    if let Some(window) = widget.downcast_ref::<gtk::Window>()
        && let Some(text) = window
            .title()
            .and_then(|old| messages.get(old.as_str()).cloned())
    {
        window.set_title(Some(&text));
    }
    if let Some(dropdown) = widget.downcast_ref::<gtk::DropDown>()
        && let Some(model) = dropdown.model().and_downcast::<gtk::StringList>()
    {
        let selected = dropdown.selected();
        for index in 0..model.n_items() {
            if let Some(text) = model.string(index).and_then(|old| {
                messages
                    .get(old.as_str())
                    .filter(|new| new.as_str() != old.as_str())
                    .cloned()
            }) {
                model.splice(index, 1, &[&text]);
            }
        }
        dropdown.set_selected(selected);
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        translate_widgets(&node, messages);
    }
}

pub fn build(
    app: &gtk::Application,
    path: PathBuf,
    locale: Option<&str>,
    _executable: PathBuf,
) -> Result<gtk::ApplicationWindow, String> {
    crate::branding::install();
    let path = absolute(&path)?;
    let original = Config::load(path.exists().then_some(path.as_path()))?;
    let strings = Rc::new(I18n::new(locale.or(original.locale.as_deref()), None)?);
    let translating = Rc::new(std::cell::Cell::new(false));
    let theme = Theme::from_config(&original)?;
    let drafts = crate::theme_editor::Drafts::new()?;
    let layout = original.layout.as_ref().unwrap_or(&theme.layout);
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(strings.text("settings-title"))
        .default_width(1000)
        .default_height(820)
        .build();
    window.add_css_class("settings");
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
    hint.add_css_class("subtitle");
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
    let language = combo(
        &[
            ("auto", strings.text("settings-system")),
            ("pt-BR", "Português (Brasil)".into()),
            ("en-US", "English".into()),
        ],
        locale.or(original.locale.as_deref()).unwrap_or("auto"),
        "settings-language",
    );
    row(&form, &strings.text("settings-language"), &language.widget);
    let presets = crate::themes::presets();
    let mut options: Vec<_> = presets
        .iter()
        .map(|p| (p.id.as_str(), p.name.clone()))
        .collect();
    options.push(("external", strings.text("settings-external-theme")));
    let theme_choice = combo(
        &options,
        if original.theme.is_some() {
            "external"
        } else {
            &original.theme_preset
        },
        "settings-theme-selector",
    );
    row(&form, &strings.text("settings-theme"), &theme_choice.widget);
    let theme_path = path_entry(original.theme.as_deref(), "settings-theme");
    theme_path.set_placeholder_text(Some(&strings.text("settings-theme-folder")));
    let external = chooser(&window, &theme_path, true, &strings);
    external.set_visible(original.theme.is_some());
    form.append(&external);
    let theme_error = gtk::Label::new(None);
    theme_error.set_wrap(true);
    theme_error.set_visible(false);
    form.append(&theme_error);
    let provider: Rc<RefCell<Option<gtk::CssProvider>>> = Rc::new(RefCell::new(None));
    let (weak_window, weak_choice, weak_path, weak_external, weak_error) = (
        window.downgrade(),
        theme_choice.widget.downgrade(),
        theme_path.downgrade(),
        external.downgrade(),
        theme_error.downgrade(),
    );
    let ids = theme_choice.ids.clone();
    let base_theme = path.parent().unwrap().to_path_buf();
    let active_provider = provider.clone();
    let missing_theme = strings.text("settings-theme-required");
    let refresh_theme: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(window), Some(choice), Some(entry), Some(external), Some(error_label)) = (
            weak_window.upgrade(),
            weak_choice.upgrade(),
            weak_path.upgrade(),
            weak_external.upgrade(),
            weak_error.upgrade(),
        ) else {
            return;
        };
        let id = ids[choice.selected() as usize].clone();
        external.set_visible(id == "external");
        if id == "external" && selected_path(&entry, &base_theme).is_none() {
            error_label.set_visible(true);
            error_label.set_text(&missing_theme);
            return;
        }
        let config = Config {
            theme: if id == "external" {
                selected_path(&entry, &base_theme)
            } else {
                None
            },
            theme_preset: if id == "external" {
                "classic".into()
            } else {
                id
            },
            ..Config::default()
        };
        let result = (|| -> Result<(), String> {
            let theme = Theme::from_config(&config)?;
            let css = gtk::CssProvider::new();
            let errors = Rc::new(RefCell::new(Vec::new()));
            let capture = errors.clone();
            css.connect_parsing_error(move |_, _, error| {
                capture.borrow_mut().push(error.to_string())
            });
            css.load_from_string(&theme.css);
            if !errors.borrow().is_empty() {
                return Err(errors.borrow().join("; "));
            }
            let display = gtk::prelude::WidgetExt::display(&window);
            if let Some(old) = active_provider.replace(Some(css.clone())) {
                gtk::style_context_remove_provider_for_display(&display, &old);
            }
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            Ok(())
        })();
        error_label.set_visible(result.is_err());
        error_label.set_text(&result.err().unwrap_or_default());
    });
    refresh_theme();
    let refresh = refresh_theme.clone();
    let reset_drafts = drafts.clone();
    let theme_translation = translating.clone();
    theme_choice.widget.connect_selected_notify(move |_| {
        if theme_translation.get() {
            return;
        }
        reset_drafts.clear();
        refresh();
    });
    theme_path.connect_changed(move |_| refresh_theme());
    let document_provider = provider.clone();
    let close_drafts = drafts.clone();
    let display = gtk::prelude::WidgetExt::display(&window);
    window.connect_destroy(move |_| {
        close_drafts.close();
        if let Some(css) = provider.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &css);
        }
    });
    let media_tabs = gtk::Stack::new();
    media_tabs.set_widget_name("settings-media-tabs");
    media_tabs.set_vhomogeneous(false);
    media_tabs.set_hhomogeneous(false);
    media_tabs.set_transition_type(gtk::StackTransitionType::Crossfade);
    media_tabs.set_transition_duration(150);
    let media_switcher = gtk::StackSwitcher::new();
    media_switcher.set_stack(Some(&media_tabs));
    media_switcher.add_css_class("section-tabs");
    media_switcher.set_halign(gtk::Align::Start);
    form.append(&media_switcher);
    form.append(&media_tabs);
    let background_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let idle_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    media_tabs.add_titled(
        &background_page,
        Some("background"),
        &strings.text("settings-background"),
    );
    media_tabs.add_titled(&idle_page, Some("rest"), &strings.text("settings-rest"));
    let catalog = crate::media_editor::Catalog::new(crate::library::catalog(&original, &theme));
    let procedurals = catalog.procedurals.clone();
    procedurals.replace(original.procedurals.clone());
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
    background_page.append(&background.widget);
    let slideshow = spin(
        original.slideshow_seconds as f64,
        1.0,
        86400.0,
        1.0,
        "settings-slideshow",
    );
    row(
        &background_page,
        &strings.text("media-interval"),
        &slideshow,
    );
    let appearance = gtk::Box::new(gtk::Orientation::Vertical, 10);
    appearance.set_margin_top(12);
    let expander = gtk::Expander::new(Some(&strings.text("settings-layout-options")));
    expander.set_widget_name("settings-layout-options");
    expander.add_css_class("layout-options");
    expander.set_child(Some(&appearance));
    form.append(&expander);
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
        &appearance,
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
        &appearance,
        &strings.text("settings-arrangement"),
        &arrangement.widget,
    );
    let spacing = spin(layout.spacing as f64, 0.0, 128.0, 1.0, "settings-spacing");
    row(&appearance, &strings.text("settings-spacing"), &spacing);
    let padding = spin(layout.padding as f64, 0.0, 256.0, 1.0, "settings-padding");
    row(&appearance, &strings.text("settings-padding"), &padding);
    let scale = spin(layout.keyboard_scale, 0.5, 2.0, 0.05, "settings-scale");
    scale.set_digits(2);
    row(&appearance, &strings.text("settings-scale"), &scale);
    let clock = gtk::CheckButton::with_label(&strings.text("settings-clock"));
    clock.set_active(layout.clock_visible);
    clock.set_widget_name("settings-clock");
    appearance.append(&clock);
    let avatar = gtk::CheckButton::with_label(&strings.text("settings-avatar"));
    avatar.set_active(layout.avatar_visible);
    avatar.set_widget_name("settings-avatar");
    appearance.append(&avatar);
    let power = gtk::CheckButton::with_label(&strings.text("settings-power"));
    power.set_active(layout.power_visible);
    power.set_widget_name("settings-power");
    appearance.append(&power);
    let system_keyboard = gtk::CheckButton::with_label(&strings.text("settings-system-keyboard"));
    system_keyboard.set_active(original.system_keyboard);
    appearance.append(&system_keyboard);
    let disable_idle = gtk::CheckButton::with_label(&strings.text("idle-disable"));
    disable_idle.set_widget_name("settings-disable-idle");
    disable_idle.set_active(!original.idle_enabled);
    idle_page.append(&disable_idle);
    let idle_options = gtk::Box::new(gtk::Orientation::Vertical, 10);
    idle_options.set_widget_name("settings-idle-options");
    idle_page.append(&idle_options);
    let reuse = gtk::CheckButton::with_label(&strings.text("idle-reuse"));
    reuse.set_widget_name("settings-reuse-background");
    reuse.set_active(original.idle_reuse_background);
    idle_options.append(&reuse);
    let idle = spin(
        original.idle_seconds as f64,
        1.0,
        86400.0,
        30.0,
        "settings-idle",
    );
    row(&idle_options, &strings.text("settings-idle"), &idle);
    let idle_group = gtk::Box::new(gtk::Orientation::Vertical, 8);
    idle_group.set_widget_name("settings-idle-media");
    idle_options.append(&idle_group);
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
    let weak_options = idle_options.downgrade();
    let update: Rc<dyn Fn()> = Rc::new(move || {
        if let (Some(disable), Some(reuse), Some(group), Some(idle)) = (
            weak_disable.upgrade(),
            weak_reuse.upgrade(),
            weak_group.upgrade(),
            weak_idle.upgrade(),
        ) {
            if let Some(options) = weak_options.upgrade() {
                options.set_visible(!disable.is_active());
            }
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
    appearance.append(&controller);
    let edit_theme = gtk::Button::with_label(&strings.text("theme-editor-title"));
    edit_theme.set_widget_name("settings-edit-theme");
    appearance.append(&edit_theme);
    let restore_layout = gtk::Button::with_label(&strings.text("restore-defaults"));
    restore_layout.set_widget_name("settings-restore-layout");
    restore_layout.set_tooltip_text(Some(&strings.text("restore-layout-help")));
    appearance.append(&restore_layout);
    {
        // Only the layout group returns to its defaults; media, pools and the rest
        // of the form are untouched, and nothing is written until Save.
        let defaults = crate::config::Layout::default();
        let (widgets, checks) = (
            (
                spacing.clone(),
                padding.clone(),
                scale.clone(),
                alignment.widget.clone(),
                arrangement.widget.clone(),
            ),
            (clock.clone(), avatar.clone(), power.clone()),
        );
        restore_layout.connect_clicked(move |_| {
            widgets.0.set_value(defaults.spacing as f64);
            widgets.1.set_value(defaults.padding as f64);
            widgets.2.set_value(defaults.keyboard_scale);
            widgets.3.set_selected(match defaults.alignment {
                Alignment::Start => 0,
                Alignment::Center => 1,
                Alignment::End => 2,
            });
            widgets
                .4
                .set_selected(u32::from(defaults.arrangement == Arrangement::Horizontal));
            checks.0.set_active(defaults.clock_visible);
            checks.1.set_active(defaults.avatar_visible);
            checks.2.set_active(defaults.power_visible);
        });
    }
    let (
        weak_spacing,
        weak_padding,
        weak_scale,
        weak_clock,
        weak_avatar,
        weak_power,
        weak_alignment,
        weak_arrangement,
    ) = (
        spacing.downgrade(),
        padding.downgrade(),
        scale.downgrade(),
        clock.downgrade(),
        avatar.downgrade(),
        power.downgrade(),
        alignment.widget.downgrade(),
        arrangement.widget.downgrade(),
    );
    let idle_clock_visible = Rc::new(std::cell::Cell::new(layout.idle_clock_visible));
    let document_idle_clock = idle_clock_visible.clone();
    let weak_theme_window = window.downgrade();
    let apply_document: Rc<dyn Fn(Theme)> = Rc::new(move |theme| {
        document_idle_clock.set(theme.layout.idle_clock_visible);
        if let Some(window) = weak_theme_window.upgrade() {
            let display = gtk::prelude::WidgetExt::display(&window);
            let css = gtk::CssProvider::new();
            css.load_from_string(&theme.css);
            if let Some(old) = document_provider.replace(Some(css.clone())) {
                gtk::style_context_remove_provider_for_display(&display, &old);
            }
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        if let Some(w) = weak_spacing.upgrade() {
            w.set_value(theme.layout.spacing as f64);
        }
        if let Some(w) = weak_padding.upgrade() {
            w.set_value(theme.layout.padding as f64);
        }
        if let Some(w) = weak_scale.upgrade() {
            w.set_value(theme.layout.keyboard_scale);
        }
        if let Some(w) = weak_clock.upgrade() {
            w.set_active(theme.layout.clock_visible);
        }
        if let Some(w) = weak_avatar.upgrade() {
            w.set_active(theme.layout.avatar_visible);
        }
        if let Some(w) = weak_power.upgrade() {
            w.set_active(theme.layout.power_visible);
        }
        if let Some(w) = weak_alignment.upgrade() {
            w.set_selected(match theme.layout.alignment {
                Alignment::Start => 0,
                Alignment::Center => 1,
                Alignment::End => 2,
            });
        }
        if let Some(w) = weak_arrangement.upgrade() {
            w.set_selected(if theme.layout.arrangement == Arrangement::Vertical {
                0
            } else {
                1
            });
        }
    });
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
    let missing_theme = strings.text("settings-theme-required");
    let read_drafts = drafts.clone();
    let language_widget = language.widget.clone();
    let read_idle_clock = idle_clock_visible.clone();
    let read: Rc<dyn Fn() -> Result<Config, String>> = Rc::new(move || {
        read_drafts.validate()?;
        let mut config = original.clone();
        config.procedurals = procedurals.borrow().clone();
        let selected_theme = theme_choice.active_id().unwrap_or_else(|| "classic".into());
        config.theme = if selected_theme == "external" {
            Some(selected_path(&theme_path, &base).ok_or_else(|| missing_theme.clone())?)
        } else {
            None
        };
        if selected_theme != "external" {
            config.theme_preset = selected_theme;
        }
        if let Some(path) = read_drafts.path() {
            config.theme = Some(path);
        }
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
            idle_clock_visible: read_idle_clock.get(),
            avatar_visible: avatar.is_active(),
            power_visible: power.is_active(),
            keyboard_scale: scale.value(),
        });
        validate_theme(&config)?;
        Ok(config)
    });
    let language_strings = strings.clone();
    let language_window = window.downgrade();
    language_widget.connect_selected_notify(move |selector| {
        if translating.replace(true) {
            return;
        }
        let locale = match selector.selected() {
            1 => Some("pt-BR"),
            2 => Some("en-US"),
            _ => None,
        };
        let Ok(next) = I18n::new(locale, None) else {
            translating.set(false);
            return;
        };
        let messages: std::collections::HashMap<_, _> = I18n::keys()
            .map(|key| (language_strings.text(key), next.text(key)))
            .collect();
        if let Some(window) = language_window.upgrade() {
            translate_widgets(window.upcast_ref(), &messages);
            for child in gtk::Window::list_toplevels() {
                if let Some(child_window) = child.downcast_ref::<gtk::Window>()
                    && child_window.transient_for().as_ref() == Some(window.upcast_ref())
                {
                    translate_widgets(&child, &messages);
                }
            }
        }
        let _ = language_strings.set_locale(locale);
        translating.set(false);
    });
    let edit_read = read.clone();
    let edit_drafts = drafts.clone();
    let weak_edit_parent = window.downgrade();
    let edit_strings = strings.clone();
    let edit_status = status.downgrade();
    edit_theme.connect_clicked(move |_| {
        let Some(parent) = weak_edit_parent.upgrade() else {
            return;
        };
        let result = edit_read().and_then(|config| {
            edit_drafts.open(
                &parent,
                &config,
                edit_strings.clone(),
                apply_document.clone(),
            )
        });
        if let Err(error) = result
            && let Some(status) = edit_status.upgrade()
        {
            status.set_text(&error);
        }
    });
    let read_save = read.clone();
    let save_status = status.downgrade();
    let save_strings = strings.clone();
    save.connect_clicked(move |_| {
        let result = read_save().and_then(|mut config| {
            drafts.persist(&mut config, &path)?;
            config.save(&path)
        });
        if let Some(status) = save_status.upgrade() {
            status.set_text(&match result {
                Ok(()) => save_strings.text("settings-saved"),
                Err(error) => error,
            });
        }
    });
    let live = Rc::new(RefCell::new(crate::live_preview::LivePreview::default()));
    let cleanup = live.clone();
    window.connect_destroy(move |_| cleanup.borrow_mut().close());
    let preview_status = status.downgrade();
    let preview_strings = strings.clone();
    let preview_read = read.clone();
    let preview_live = live.clone();
    let preview_app = app.clone();
    let preview_tabs = media_tabs.clone();
    preview.connect_clicked(move |_| {
        preview_live
            .borrow_mut()
            .set_idle(preview_tabs.visible_child_name().as_deref() == Some("rest"));
        let result = preview_read()
            .and_then(|config| preview_live.borrow_mut().update(&preview_app, config, true));
        if let Some(status) = preview_status.upgrade() {
            status.set_text(&match result {
                Ok(()) => preview_strings.text("settings-preview-open"),
                Err(e) => e,
            });
        }
    });
    let weak_window = window.downgrade();
    let weak_status = status.downgrade();
    let live_app = app.clone();
    let timer = glib::timeout_add_local(std::time::Duration::from_millis(350), move || {
        if weak_window.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        let open = live.borrow().is_open();
        if !open {
            return glib::ControlFlow::Continue;
        }
        live.borrow_mut()
            .set_idle(media_tabs.visible_child_name().as_deref() == Some("rest"));
        let result = read().and_then(|config| live.borrow_mut().update(&live_app, config, false));
        if let Err(error) = result
            && let Some(status) = weak_status.upgrade()
        {
            status.set_text(&error);
        }
        glib::ControlFlow::Continue
    });
    let timer = RefCell::new(Some(timer));
    window.connect_destroy(move |_| {
        if let Some(timer) = timer.borrow_mut().take() {
            timer.remove();
        }
    });
    Ok(window)
}

pub fn run(path: PathBuf, locale: Option<String>) -> Result<(), String> {
    gtk::init().map_err(|e| e.to_string())?;
    let app = gtk::Application::new(
        Some(crate::branding::APP_ID),
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
