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
    window.present();
    while glib::MainContext::default().iteration(false) {}
    let get = |name| find(window.upcast_ref(), name).unwrap();
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
    let config = Config::load(Some(&path)).unwrap();
    assert_eq!(config.pam_service, "preserve-me");
    let layout = config.layout.unwrap();
    assert_eq!(layout.padding, 48);
    assert_eq!(layout.alignment, Alignment::End);
    assert!(!layout.clock_visible);
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
    assert!(!reuse.is_sensitive());
    assert!(!get("settings-idle").is_sensitive());
    disable.set_active(false);
    assert!(reuse.is_sensitive());
    assert!(!media.is_visible());
    reuse.set_active(false);
    assert!(media.is_visible());
    drop(disable);
    drop(reuse);
    drop(media);
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
