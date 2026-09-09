//! Settings UAT on an isolated config; never modifies the user's configuration.
use decklock::{
    config::{Alignment, Config},
    settings,
};
use gtk::{gio, prelude::*};
fn find(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if widget.widget_name() == name {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if let Some(found) = find(&node, name) {
            return Some(found);
        }
    }
    None
}
fn main() {
    gtk::init().unwrap();
    decklock::branding::install();
    decklock::branding::install();
    let icons = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
    for name in [
        decklock::branding::APP_ID,
        "io.github.yan_vidal.DeckLock-symbolic",
    ] {
        assert!(icons.has_icon(name), "Embedded icon missing: {name}");
    }
    assert_eq!(
        icons
            .resource_path()
            .iter()
            .filter(|p| p.as_str() == "/io/github/yan_vidal/DeckLock/icons")
            .count(),
        1
    );

    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.SettingsCheck"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config {
        pam_service: "preserve-me".into(),
        ..Config::default()
    }
    .save(&path)
    .unwrap();
    let executable = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("decklock");
    let window = settings::build(&app, path.clone(), Some("en-US"), executable).unwrap();
    assert!(
        window.titlebar().is_some(),
        "Settings must provide its own close control"
    );
    window.present();
    while glib::MainContext::default().iteration(false) {}
    let get =
        |name| find(window.upcast_ref(), name).unwrap_or_else(|| panic!("Missing widget {name}"));
    get("settings-layout-options")
        .downcast::<gtk::Expander>()
        .unwrap()
        .set_expanded(true);
    get("settings-padding")
        .downcast::<gtk::SpinButton>()
        .unwrap()
        .set_value(48.0);
    get("settings-alignment")
        .downcast::<gtk::DropDown>()
        .unwrap()
        .set_selected(2);
    get("settings-clock")
        .downcast::<gtk::CheckButton>()
        .unwrap()
        .set_active(false);
    // Power options ship visible; hiding them is a layout preference like the rest.
    let power = get("settings-power")
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(power.is_active(), "Power options must default to visible");
    power.set_active(false);
    get("settings-save")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let config = Config::load(Some(&path)).unwrap();
    assert_eq!(config.pam_service, "preserve-me");
    let layout = config.layout.clone().unwrap();
    assert_eq!(layout.padding, 48);
    assert_eq!(layout.alignment, Alignment::End);
    assert!(!layout.clock_visible);
    assert!(!layout.power_visible);
    // Restore returns the layout group to its defaults and leaves the rest alone,
    // and writes nothing until the form is saved again.
    get("settings-restore-layout")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    while glib::MainContext::default().iteration(false) {}
    assert!(
        power.is_active()
            && get("settings-clock")
                .downcast::<gtk::CheckButton>()
                .unwrap()
                .is_active()
    );
    assert_eq!(
        get("settings-padding")
            .downcast::<gtk::SpinButton>()
            .unwrap()
            .value_as_int(),
        decklock::config::Layout::default().padding
    );
    assert_eq!(
        Config::load(Some(&path)).unwrap().layout.unwrap().padding,
        48,
        "Restore must not write to disk on its own"
    );
    assert_eq!(
        Config::load(Some(&path)).unwrap().pam_service,
        "preserve-me"
    );
    get("settings-save")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let restored = Config::load(Some(&path)).unwrap().layout.unwrap();
    assert_eq!(
        restored.padding,
        decklock::config::Layout::default().padding
    );
    assert!(restored.power_visible && restored.clock_visible);
    // Put the edited layout back so the checks below see what they expect.
    get("settings-padding")
        .downcast::<gtk::SpinButton>()
        .unwrap()
        .set_value(48.0);
    get("settings-alignment")
        .downcast::<gtk::DropDown>()
        .unwrap()
        .set_selected(2);
    get("settings-clock")
        .downcast::<gtk::CheckButton>()
        .unwrap()
        .set_active(false);
    get("settings-save")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    get("settings-media-tabs")
        .downcast::<gtk::Stack>()
        .unwrap()
        .set_visible_child_name("rest");
    let disable = get("settings-disable-idle")
        .downcast::<gtk::CheckButton>()
        .unwrap();
    let reuse = get("settings-reuse-background")
        .downcast::<gtk::CheckButton>()
        .unwrap();
    let media = get("settings-idle-media");
    assert!(media.is_visible());
    reuse.set_active(true);
    assert!(!media.is_visible());
    disable.set_active(true);
    assert!(!get("settings-idle-options").is_visible());
    assert!(!reuse.is_sensitive());
    assert!(!get("settings-idle").is_sensitive());
    disable.set_active(false);
    assert!(get("settings-idle-options").is_visible());
    assert!(reuse.is_sensitive());
    assert!(!media.is_visible());
    reuse.set_active(false);
    assert!(media.is_visible());
    drop(disable);
    drop(reuse);
    drop(media);
    let selector = get("settings-theme-selector")
        .downcast::<gtk::DropDown>()
        .unwrap();
    let mut previous_color = None;
    for (index, preset) in decklock::themes::presets().iter().enumerate() {
        selector.set_selected(index as u32);
        while glib::MainContext::default().iteration(false) {}
        get("settings-save")
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        assert_eq!(Config::load(Some(&path)).unwrap().theme_preset, preset.id);
        let color = window.color();
        if let Some(previous) = previous_color {
            assert_ne!(color, previous, "Theme did not change settings colors");
        }
        previous_color = Some(color);
    }
    let tabs = get("settings-media-tabs").downcast::<gtk::Stack>().unwrap();
    assert!(tabs.child_by_name("background").is_some());
    assert!(tabs.child_by_name("rest").is_some());
    tabs.set_visible_child_name("rest");
    assert!(
        get("settings-idle-background-info")
            .tooltip_text()
            .unwrap()
            .contains("random")
    );
    tabs.set_visible_child_name("background");
    assert!(
        get("settings-background-info")
            .tooltip_text()
            .unwrap()
            .contains("does not delete")
    );
    drop(tabs);
    // Preview must not persist unsaved edits.
    let saved = std::fs::read(&path).unwrap();
    get("settings-padding")
        .downcast::<gtk::SpinButton>()
        .unwrap()
        .set_value(64.0);
    get("settings-preview")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(
        get("settings-status")
            .downcast::<gtk::Label>()
            .unwrap()
            .text()
            .contains("Preview opened")
    );
    // Invalid theme must fail without replacing the file.
    selector.set_selected(decklock::themes::presets().len() as u32);
    drop(selector);
    get("settings-save")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        saved,
        "Empty external theme must not save"
    );
    get("settings-theme")
        .downcast::<gtk::Entry>()
        .unwrap()
        .set_text("/nonexistent/decklock-theme");
    get("settings-save")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let weak = get("settings-padding").downgrade();
    window.destroy();
    drop(window);
    while glib::MainContext::default().iteration(false) {}
    assert!(
        weak.upgrade().is_none(),
        "Settings retained controls after close"
    );
    println!(
        "PASS: settings save/reload, layout edits, preserve PAM, unsaved preview, invalid theme rejection, cleanup"
    );
}
