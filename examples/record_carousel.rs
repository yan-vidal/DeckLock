//! Scripted recording of the greeter user carousel in 1280x800.
//! Runs the preview UI with greeter=true, cycles between users,
//! and exits after the demo sequence completes.
use decklock::{
    config::{Config, Theme},
    i18n::I18n,
    ui,
};
use gtk::{gio, prelude::*};
use std::{cell::Cell, rc::Rc};

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        found.extend(descendants(&node));
        found.push(node);
    }
    found
}

fn find_button(view: &ui::View, name: &str) -> gtk::Button {
    descendants(view.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == name)
        .unwrap_or_else(|| panic!("Missing widget: {name}"))
        .downcast::<gtk::Button>()
        .unwrap_or_else(|_| panic!("Widget {name} is not a Button"))
}

fn main() {
    gtk::init().expect("Display required for recording");
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.RecordCarousel"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();

    let settings = Rc::new(ui::Settings {
        config: Config {
            system_keyboard: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("pt-BR"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "yan".into(),
        greeter: true,
    });

    ui::apply_css(&settings.theme.css).unwrap();
    let view = ui::build(
        &app,
        settings,
        Rc::new(|_| panic!("Preview must never authenticate")),
    );

    view.window.set_default_size(1280, 800);
    view.window.set_cursor_from_name(Some("none"));
    view.window.fullscreen();
    view.window.present();

    let main_loop = glib::MainLoop::new(None, false);
    let stop = main_loop.clone();
    let step = Cell::new(0);
    let ready_signal = std::path::Path::new("/tmp/decklock_carousel_ready");
    let _ = std::fs::remove_file(ready_signal);

    // Run a timeline of actions (each tick 400ms)
    glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
        let n = step.get() + 1;
        step.set(n);

        match n {
            // Signal recorder that window is fully up and displayed
            2 => {
                let _ = std::fs::write(ready_signal, "ready\n");
            }
            // Step 6 (~2.4s from start): Click translucent side avatar directly -> switches to gravador
            6 => {
                let next_avatar = find_button(&view, "avatar-next");
                next_avatar.emit_clicked();
            }
            // Step 12 (~4.8s from start): Click translucent side avatar directly -> switches back to yan
            12 => {
                let prev_avatar = find_button(&view, "avatar-prev");
                prev_avatar.emit_clicked();
            }
            // Step 17 (~6.8s from start): Exit cleanly
            17 => {
                view.window.close();
                stop.quit();
                return glib::ControlFlow::Break;
            }
            _ => {}
        }
        glib::ControlFlow::Continue
    });

    main_loop.run();
}
