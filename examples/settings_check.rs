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
    let window = settings::build(&app, path.clone(), Some("en-US"), executable.clone()).unwrap();
    assert!(
        window.titlebar().is_some(),
        "Settings must provide its own close control"
    );
    window.present();
    while glib::MainContext::default().iteration(false) {}
    let get =
        |name| find(window.upcast_ref(), name).unwrap_or_else(|| panic!("Missing widget {name}"));
    let save = || {
        get("settings-save")
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        while glib::MainContext::default().iteration(false) {}
    };
    // Three tabs: Lock, Login and Rest. The login mirrors the lock until told
    // otherwise, and an untouched login writes no section.
    // Scoped: references held past the window's close would keep its controls alive.
    {
        let tabs = get("settings-media-tabs").downcast::<gtk::Stack>().unwrap();
        let pages = tabs.pages();
        let names: Vec<String> = (0..pages.n_items())
            .filter_map(|i| pages.item(i).and_downcast::<gtk::StackPage>())
            .filter_map(|page| page.name())
            .map(|name| name.to_string())
            .collect();
        assert_eq!(names, ["background", "login", "rest"]);
        let same = get("settings-login-same")
            .downcast::<gtk::CheckButton>()
            .unwrap();
        assert!(same.is_active(), "The login mirrors the lock by default");
        assert!(!get("settings-login-own").is_visible());
        save();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("login"),
            "Untouched login writes nothing: {text}"
        );
        same.set_active(false);
        assert!(get("settings-login-own").is_visible());
        get("settings-login-avatar")
            .downcast::<gtk::CheckButton>()
            .unwrap()
            .set_active(false);
        get("settings-login-slideshow")
            .downcast::<gtk::SpinButton>()
            .unwrap()
            .set_value(9.0);
        save();
        let config = Config::load(Some(&path)).unwrap();
        assert!(!config.login.same_as_lock);
        assert_eq!(config.login.slideshow_seconds, Some(9));
        assert!(!config.login.layout.as_ref().unwrap().avatar_visible);
        assert!(
            config.layout.as_ref().unwrap().avatar_visible,
            "The lock keeps its own layout"
        );
        assert_eq!(config.pam_service, "preserve-me");
        // Restore affects the login's layout only.
        get("settings-login-restore-layout")
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        assert!(
            get("settings-login-avatar")
                .downcast::<gtk::CheckButton>()
                .unwrap()
                .is_active()
        );
        same.set_active(true);
        assert!(!get("settings-login-own").is_visible());
        save();
        assert!(Config::load(Some(&path)).unwrap().login.same_as_lock);
    }
    // "Apply to login" saves, then runs the login refresh through pkexec. A fake
    // pkexec in a private PATH records the command; never the real polkit or sudo.
    {
        use std::os::unix::fs::PermissionsExt;
        let fake = dir.path().join("fake-bin");
        std::fs::create_dir(&fake).unwrap();
        let marker = dir.path().join("pkexec-args");
        std::fs::write(
            fake.join("pkexec"),
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s' \"$FAKE_PKEXEC_ERR\" >&2\nexit \"$FAKE_PKEXEC_EXIT\"\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(fake.join("pkexec"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let empty = dir.path().join("empty-bin");
        std::fs::create_dir(&empty).unwrap();
        let original_path = std::env::var_os("PATH");
        let apply = get("settings-apply-login")
            .downcast::<gtk::Button>()
            .unwrap();
        let status = get("settings-status").downcast::<gtk::Label>().unwrap();
        let run = |bin: &std::path::Path, code: &str, err: &str| {
            let _ = std::fs::remove_file(&marker);
            // SAFETY: this GTK contract is single-process; the fake PATH and the
            // fake's knobs are restored after these cases.
            unsafe {
                std::env::set_var("PATH", bin);
                std::env::set_var("FAKE_PKEXEC_EXIT", code);
                std::env::set_var("FAKE_PKEXEC_ERR", err);
            }
            status.set_text("");
            apply.emit_clicked();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while (status.text().is_empty() || !apply.is_sensitive())
                && std::time::Instant::now() < deadline
            {
                glib::MainContext::default().iteration(false);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(apply.is_sensitive(), "Apply to login must finish");
            status.text().to_string()
        };
        let done = run(&fake, "0", "");
        assert!(done.contains("next time"), "{done}");
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            format!("{}\nsetup\nlogin-appearance\n", executable.display())
        );
        let cancelled = run(&fake, "126", "");
        assert!(cancelled.contains("nothing was changed"), "{cancelled}");
        let no_agent = run(
            &fake,
            "127",
            "Error executing command as another user: No authentication agent found.",
        );
        assert!(
            no_agent.contains("sudo decklock setup login-appearance"),
            "{no_agent}"
        );
        let refused = run(&fake, "1", "The login does not use DeckLock yet");
        assert!(refused.contains("does not use DeckLock"), "{refused}");
        let missing = run(&empty, "0", "");
        assert!(
            missing.contains("sudo decklock setup login-appearance"),
            "{missing}"
        );
        assert!(!marker.exists(), "No pkexec, nothing ran");
        unsafe {
            match original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
            std::env::remove_var("FAKE_PKEXEC_EXIT");
            std::env::remove_var("FAKE_PKEXEC_ERR");
        }
    }
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
    // The battery shows its percentage by default; without it only the icon remains.
    let battery_percentage = get("settings-battery-percentage")
        .downcast::<gtk::CheckButton>()
        .unwrap();
    assert!(
        battery_percentage.is_active(),
        "The battery percentage must default to visible"
    );
    battery_percentage.set_active(false);
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
    assert!(!layout.battery_percentage);
    // Restore returns the layout group to its defaults and leaves the rest alone,
    // and writes nothing until the form is saved again.
    get("settings-restore-layout")
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    while glib::MainContext::default().iteration(false) {}
    assert!(
        power.is_active()
            && battery_percentage.is_active()
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
    assert!(restored.power_visible && restored.clock_visible && restored.battery_percentage);
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
