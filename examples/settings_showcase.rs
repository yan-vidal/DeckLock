//! Instrumented documentation demo. Waits for the recorder's pointer before each action.
//! Uses real settings widgets and preview rendering, but never saves or locks.
use gtk::{gio, prelude::*};
use std::time::{Duration, Instant};
fn walk(w: &gtk::Widget, predicate: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if predicate(w) {
        return Some(w.clone());
    }
    let mut child = w.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if let Some(found) = walk(&node, predicate) {
            return Some(found);
        }
    }
    None
}
fn find(w: &gtk::Widget, name: &str) -> gtk::Widget {
    walk(w, &|w| w.widget_name() == name).unwrap_or_else(|| panic!("Missing {name}"))
}
fn label(w: &gtk::Widget, text: &str) -> gtk::Widget {
    walk(w, &|w| {
        w.is_visible()
            && w.downcast_ref::<gtk::Label>()
                .is_some_and(|l| l.text() == text)
    })
    .unwrap_or_else(|| panic!("Missing label {text}"))
}
fn top(name: &str) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap_or_else(|| panic!("Missing window {name}"))
        .downcast()
        .unwrap()
}
fn click(w: gtk::Widget) {
    w.downcast::<gtk::Button>().unwrap().emit_clicked();
}
#[derive(serde::Serialize)]
struct Pointer {
    step: u32,
    title: String,
    x: f32,
    y: f32,
}
fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.SettingsShowcase"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let path = std::env::args_os()
        .nth(1)
        .expect("Read-only config path")
        .into();
    let ipc = std::path::PathBuf::from(std::env::args_os().nth(2).expect("Recorder IPC path"));
    let window =
        decklock::settings::build(&app, path, Some("en-US"), std::env::current_exe().unwrap())
            .unwrap();
    let selector = find(window.upcast_ref(), "settings-theme-selector")
        .downcast::<gtk::DropDown>()
        .unwrap();
    selector.set_selected(1);
    window.present();
    let main_loop = glib::MainLoop::new(None, false);
    let stop = main_loop.clone();
    let mut step = 0;
    let mut pending = false;
    let mut scrolled = false;
    let mut deadline = Instant::now() + Duration::from_secs(2);
    glib::timeout_add_local(Duration::from_millis(50), move || {
        if Instant::now() < deadline {
            return glib::ControlFlow::Continue;
        }
        if step == 19 {
            std::fs::write(ipc.with_extension("done"), "done").unwrap();
            window.destroy();
            stop.quit();
            return glib::ControlFlow::Break;
        }
        if step == 15 && !scrolled {
            let scroll = walk(window.upcast_ref(), &|w| w.is::<gtk::ScrolledWindow>())
                .unwrap()
                .downcast::<gtk::ScrolledWindow>()
                .unwrap();
            let adjustment = scroll.vadjustment();
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            scrolled = true;
            deadline = Instant::now() + Duration::from_millis(500);
            return glib::ControlFlow::Continue;
        }
        let root: gtk::Window = if step == 13 {
            top("media-viewer")
        } else if step >= 16 {
            top("theme-editor")
        } else {
            window.clone().upcast()
        };
        let get = |name| find(window.upcast_ref(), name);
        let target = match step {
            0 => get("settings-preview"),
            1 | 3 => selector.clone().upcast(),
            2 => label(selector.upcast_ref(), "Catppuccin Latte"),
            4 => label(selector.upcast_ref(), "Tokyo Night"),
            5 => label(window.upcast_ref(), "Rest"),
            6 | 7 => get("settings-disable-idle"),
            8 | 9 => get("settings-reuse-background"),
            10 => label(window.upcast_ref(), "Background"),
            11 => label(&get("settings-background"), "Videos"),
            12 => find(&get("settings-background-videos"), "media-eye"),
            13 => walk(root.upcast_ref(), &|w| {
                w.downcast_ref::<gtk::Button>()
                    .is_some_and(|b| b.icon_name().as_deref() == Some("window-close-symbolic"))
                    || w.has_css_class("close")
            })
            .unwrap_or_else(|| root.clone().upcast()),
            14 => get("settings-layout-options"),
            15 => get("settings-edit-theme"),
            16 => find(root.upcast_ref(), "style.css"),
            17 => label(root.upcast_ref(), "theme.toml"),
            18 => find(root.upcast_ref(), "theme.toml"),
            _ => unreachable!(),
        };
        if !pending {
            let bounds = target.compute_bounds(&root).expect("Widget bounds");
            let message = Pointer {
                step,
                title: root.title().unwrap().to_string(),
                x: bounds.x() + bounds.width() / 2.0,
                y: bounds.y() + bounds.height() / 2.0,
            };
            let temp = ipc.with_extension("tmp");
            std::fs::write(&temp, toml::to_string(&message).unwrap()).unwrap();
            std::fs::rename(temp, &ipc).unwrap();
            pending = true;
            deadline = Instant::now() + Duration::from_millis(100);
            return glib::ControlFlow::Continue;
        }
        if std::fs::read_to_string(ipc.with_extension("ack")).unwrap_or_default()
            != step.to_string()
        {
            return glib::ControlFlow::Continue;
        }
        match step {
            0 | 12 | 15 => click(target),
            1 | 3 => {
                selector.activate();
            }
            2 | 4 => {
                selector.set_selected(if step == 2 { 2 } else { 5 });
                if let Some(popover) = walk(selector.upcast_ref(), &|w| w.is::<gtk::Popover>()) {
                    popover.downcast::<gtk::Popover>().unwrap().popdown();
                }
            }
            5 | 10 => get("settings-media-tabs")
                .downcast::<gtk::Stack>()
                .unwrap()
                .set_visible_child_name(if step == 5 { "rest" } else { "background" }),
            6..=9 => {
                let checkbox = target.downcast::<gtk::CheckButton>().unwrap();
                checkbox.set_active(!checkbox.is_active());
            }
            11 => {
                walk(&get("settings-background"), &|w| w.is::<gtk::Notebook>())
                    .unwrap()
                    .downcast::<gtk::Notebook>()
                    .unwrap()
                    .set_current_page(Some(1));
            }
            13 => {
                top("media-viewer").close();
            }
            14 => get("settings-layout-options")
                .downcast::<gtk::Expander>()
                .unwrap()
                .set_expanded(true),
            17 => find(root.upcast_ref(), "theme-editor-tabs")
                .downcast::<gtk::Notebook>()
                .unwrap()
                .set_current_page(Some(1)),
            _ => {}
        }
        println!("STEP {step} applied after pointer acknowledgement");
        let pause = match step {
            12 | 16 => 5,
            18 => 4,
            _ => 2,
        };
        step += 1;
        pending = false;
        deadline = Instant::now() + Duration::from_secs(pause);
        glib::ControlFlow::Continue
    });
    main_loop.run();
}
