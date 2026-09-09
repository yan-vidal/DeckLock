//! Settings integration: retained preview identity, editor validation and media viewer reuse.
use decklock::{config::Config, settings};
use gtk::{gio, prelude::*};
use std::time::{Duration, Instant};
fn find(widget: &gtk::Widget, name: &str) -> gtk::Widget {
    fn walk(w: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if w.widget_name() == name {
            return Some(w.clone());
        }
        let mut child = w.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if let Some(found) = walk(&node, name) {
                return Some(found);
            }
        }
        None
    }
    walk(widget, name).unwrap_or_else(|| panic!("Missing widget {name}"))
}
fn top(name: &str) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap_or_else(|| panic!("Missing window {name}"))
        .downcast()
        .unwrap()
}
fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn click(widget: gtk::Widget) {
    widget.downcast::<gtk::Button>().unwrap().emit_clicked();
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.LiveCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let a = dir.path().join("a.svg");
    let b = dir.path().join("b.svg");
    for (path, color) in [(&a, "red"), (&b, "blue")] {
        std::fs::write(path,format!("<svg xmlns='http://www.w3.org/2000/svg' width='160' height='90'><rect width='160' height='90' fill='{color}'/></svg>")).unwrap();
    }
    Config {
        background_pool: Some(vec![a.clone(), b.clone()]),
        pam_service: "preserve-me".into(),
        ..Config::default()
    }
    .save(&path)
    .unwrap();
    let window = settings::build(
        &app,
        path.clone(),
        Some("en-US"),
        std::env::current_exe().unwrap(),
    )
    .unwrap();
    window.present();
    pump(100);
    let get = |name| find(window.upcast_ref(), name);
    // Both eye controls reuse one viewer; closing it must not close settings.
    let images = get("settings-background-images")
        .downcast::<gtk::ListBox>()
        .unwrap();
    let eye_for = |path: &std::path::Path| {
        let mut row = images.first_child();
        while let Some(widget) = row {
            row = widget.next_sibling();
            if let Some(child) = widget.first_child()
                && child.tooltip_text().as_deref() == Some(path.to_string_lossy().as_ref())
            {
                return find(&child, "media-eye");
            }
        }
        panic!("Missing fixture media")
    };
    click(eye_for(&a));
    let viewer = top("media-viewer");
    assert_eq!(viewer.title().as_deref(), Some("a.svg"));
    click(eye_for(&b));
    assert_eq!(viewer, top("media-viewer"));
    assert_eq!(viewer.title().as_deref(), Some("b.svg"));
    viewer.close();
    assert!(window.is_visible());
    drop(viewer);
    drop(images);
    get("settings-layout-options")
        .downcast::<gtk::Expander>()
        .unwrap()
        .set_expanded(true);
    click(get("settings-preview"));
    pump(150);
    let preview = top("settings-live-preview");
    let count = preview.observe_controllers().n_items();
    let old_entry = find(preview.upcast_ref(), "password").downgrade();
    find(preview.upcast_ref(), "password")
        .downcast::<gtk::Entry>()
        .unwrap()
        .set_text("demo");
    let saved = std::fs::read(&path).unwrap();
    for padding in [48, 52, 64] {
        get("settings-padding")
            .downcast::<gtk::SpinButton>()
            .unwrap()
            .set_value(padding as f64);
        pump(600);
        assert_eq!(preview, top("settings-live-preview"));
        assert!(preview.is_visible());
        assert_eq!(find(preview.upcast_ref(), "content").margin_top(), padding);
        assert_eq!(
            preview.observe_controllers().n_items(),
            count,
            "Rebuild accumulated input controllers"
        );
        assert_eq!(
            find(preview.upcast_ref(), "password")
                .downcast::<gtk::Entry>()
                .unwrap()
                .text(),
            "demo"
        );
    }
    assert!(old_entry.upgrade().is_none(), "Old preview widgets leaked");
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    // Palette-only updates must retain video/picture widgets, avoiding restart flashes.
    let same_entry = find(preview.upcast_ref(), "password");
    get("settings-theme-selector")
        .downcast::<gtk::DropDown>()
        .unwrap()
        .set_selected(2);
    pump(600);
    assert_eq!(same_entry, find(preview.upcast_ref(), "password"));
    let language = get("settings-language")
        .downcast::<gtk::DropDown>()
        .unwrap();
    language.set_selected(1);
    pump(700);
    assert_eq!(
        get("settings-save")
            .downcast::<gtk::Button>()
            .unwrap()
            .label()
            .as_deref(),
        Some("Salvar")
    );
    assert_eq!(
        find(preview.upcast_ref(), "password")
            .downcast::<gtk::Entry>()
            .unwrap()
            .placeholder_text()
            .as_deref(),
        Some("Senha")
    );
    language.set_selected(2);
    pump(700);
    assert_eq!(
        get("settings-save")
            .downcast::<gtk::Button>()
            .unwrap()
            .label()
            .as_deref(),
        Some("Save")
    );
    let tabs = get("settings-media-tabs").downcast::<gtk::Stack>().unwrap();
    tabs.set_visible_child_name("rest");
    pump(800);
    assert!(!find(preview.upcast_ref(), "credentials").is_visible());
    assert!(find(preview.upcast_ref(), "clock-block").is_visible());
    get("settings-reuse-background")
        .downcast::<gtk::CheckButton>()
        .unwrap()
        .set_active(true);
    pump(800);
    assert!(find(preview.upcast_ref(), "clock-block").is_visible());
    tabs.set_visible_child_name("background");
    pump(800);
    assert!(find(preview.upcast_ref(), "credentials").is_visible());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    // Hiding power options must reach the live preview like the other switches.
    let power_switch = get("settings-power")
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(power_switch.is_active(), "Power options default to visible");
    assert!(find(preview.upcast_ref(), "power").is_visible());
    power_switch.set_active(false);
    pump(800);
    assert!(!find(preview.upcast_ref(), "power").is_visible());
    power_switch.set_active(true);
    pump(800);
    assert!(find(preview.upcast_ref(), "power").is_visible());
    click(get("settings-edit-theme"));
    let editor = top("theme-editor");
    // Saving is not obvious in a window that previews as you type, so the editor
    // says where it happens; restoring reloads the built-in theme into both tabs.
    assert!(
        !find(editor.upcast_ref(), "theme-editor-save-note")
            .downcast::<gtk::Label>()
            .unwrap()
            .text()
            .is_empty()
    );
    let document = find(editor.upcast_ref(), "theme.toml")
        .downcast::<gtk::TextView>()
        .unwrap()
        .buffer();
    let read_document = || {
        document
            .text(&document.start_iter(), &document.end_iter(), true)
            .to_string()
    };
    let mut edited: toml::Value = toml::from_str(&read_document()).unwrap();
    edited["layout"]["padding"] = toml::Value::Integer(96);
    document.set_text(&toml::to_string_pretty(&edited).unwrap());
    pump(850);
    assert_eq!(find(preview.upcast_ref(), "content").margin_top(), 96);
    click(find(editor.upcast_ref(), "theme-editor-restore"));
    pump(900);
    let default_padding =
        toml::from_str::<toml::Value>(&read_document()).unwrap()["layout"]["padding"]
            .as_integer()
            .unwrap() as i32;
    assert_ne!(default_padding, 96, "Restore did not reload the defaults");
    assert_eq!(
        find(preview.upcast_ref(), "content").margin_top(),
        default_padding,
        "Restore must reach the preview like any other edit"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        saved,
        "Restore must not write to disk on its own"
    );
    let css = find(editor.upcast_ref(), "style.css")
        .downcast::<gtk::TextView>()
        .unwrap()
        .buffer();
    let original = css
        .text(&css.start_iter(), &css.end_iter(), true)
        .to_string();
    css.set_text("window { broken");
    pump(700);
    click(get("settings-save"));
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(preview.is_visible());
    css.set_text(&(original + "\n#clock { color: #00ff00; }"));
    pump(850);
    let color = find(preview.upcast_ref(), "clock").color();
    assert!(color.green() > 0.99 && color.red() < 0.01);
    find(editor.upcast_ref(), "theme-editor-tabs")
        .downcast::<gtk::Notebook>()
        .unwrap()
        .set_current_page(Some(1));
    let metadata = find(editor.upcast_ref(), "theme.toml")
        .downcast::<gtk::TextView>()
        .unwrap()
        .buffer();
    let mut parsed: toml::Value =
        toml::from_str(&metadata.text(&metadata.start_iter(), &metadata.end_iter(), true)).unwrap();
    parsed["layout"]["padding"] = toml::Value::Integer(24);
    metadata.set_text(&toml::to_string_pretty(&parsed).unwrap());
    pump(850);
    assert_eq!(find(preview.upcast_ref(), "content").margin_top(), 24);
    assert_eq!(
        get("settings-padding")
            .downcast::<gtk::SpinButton>()
            .unwrap()
            .value_as_int(),
        24
    );
    parsed["layout"]["idle_clock_visible"] = toml::Value::Boolean(false);
    metadata.set_text(&toml::to_string_pretty(&parsed).unwrap());
    pump(800);
    tabs.set_visible_child_name("rest");
    pump(800);
    assert!(!find(preview.upcast_ref(), "clock-block").is_visible());
    tabs.set_visible_child_name("background");
    pump(800);
    assert!(find(preview.upcast_ref(), "clock-block").is_visible());
    language.set_selected(1);
    pump(700);
    assert_eq!(
        editor,
        top("theme-editor"),
        "Locale change discarded editor"
    );
    language.set_selected(2);
    pump(700);
    parsed["layout"]["padding"] = toml::Value::Integer(25);
    metadata.set_text(&toml::to_string_pretty(&parsed).unwrap());
    editor.close();
    pump(800);
    assert_eq!(
        find(preview.upcast_ref(), "content").margin_top(),
        25,
        "Closing must flush pending edits"
    );
    click(get("settings-save"));
    let config = Config::load(Some(&path)).unwrap();
    assert_eq!(config.pam_service, "preserve-me");
    let theme = config.theme.unwrap();
    assert!(theme.starts_with(dir.path().join("themes")));
    editor.close();
    drop(editor);
    drop(css);
    drop(metadata);
    window.destroy();
    drop(window);
    pump(400);
    assert!(!preview.is_visible());
    assert!(theme.join("style.css").is_file());
    assert!(decklock::config::Theme::load(Some(&theme)).is_ok());
    println!(
        "PASS: power visibility, theme restore, viewer reuse, live preview identity, palette retention, language switching, rest clock overrides, input cleanup, draft isolation and persistent theme copy"
    );
}
