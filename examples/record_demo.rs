//! Scripted interactions with the actual preview UI; fictitious text and no authentication.
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

fn key(view: &ui::View, label: &str) -> gtk::Button {
    descendants(view.keyboard.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("Missing key: {label}"))
}

fn main() {
    gtk::init().unwrap();
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock.Demo"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    app.register(None::<&gio::Cancellable>).unwrap();
    let settings = Rc::new(ui::Settings {
        config: Config {
            background: Some(std::env::args_os().nth(1).expect("video path").into()),
            system_keyboard: false,
            ..Config::default()
        },
        theme: Theme::load(None).unwrap(),
        strings: I18n::new(Some("en-US"), None).unwrap(),
        preview: true,
        show_keyboard: false,
        start_idle: false,
        username: "Demo".into(),
    });
    ui::apply_css(&settings.theme.css).unwrap();
    let view = ui::build(
        &app,
        settings,
        Rc::new(|_| panic!("Preview must never authenticate")),
    );
    view.window.fullscreen();
    view.window.present();
    let main_loop = glib::MainLoop::new(None, false);
    let stop = main_loop.clone();
    let step = Cell::new(0);
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let n = step.get() + 1;
        step.set(n);
        let target = match n {
            6 | 25 => Some((view.entry.clone().upcast::<gtk::Widget>(), 0.93, true)),
            8 => Some((key(&view, "d").upcast(), 0.5, true)),
            9 => Some((key(&view, "e").upcast(), 0.5, true)),
            10 => Some((key(&view, "m").upcast(), 0.5, true)),
            11 => Some((key(&view, "o").upcast(), 0.5, true)),
            14 | 15 => Some((key(&view, "⇧").upcast(), 0.5, true)),
            19 => Some((key(&view, "A").upcast(), 0.5, true)),
            21 => Some((key(&view, "⇪").upcast(), 0.5, true)),
            27 | 32 => Some((view.entry.clone().upcast(), 0.07, true)),
            35 | 39 | 43 | 47 => {
                let power = descendants(view.window.upcast_ref())
                    .into_iter()
                    .find(|w| w.widget_name() == "power")
                    .unwrap();
                let button = descendants(&power)
                    .into_iter()
                    .filter(|w| w.is::<gtk::Button>())
                    .nth(((n - 35) / 4) as usize)
                    .unwrap();
                Some((button, 0.5, false))
            }
            60 => {
                view.window.close();
                stop.quit();
                return glib::ControlFlow::Break;
            }
            _ => None,
        };
        if let Some((widget, fraction, click)) = target {
            if !click {
                let hovered = widget.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                    hovered.trigger_tooltip_query()
                });
            }
            let bounds = widget.compute_bounds(&view.window).unwrap();
            let path = std::env::args_os().nth(2).expect("pointer command file");
            std::fs::write(
                path,
                format!(
                    "{} {} {} {}",
                    n,
                    bounds.x() + bounds.width() * fraction,
                    bounds.y() + bounds.height() / 2.0,
                    u8::from(click)
                ),
            )
            .unwrap();
        }
        glib::ControlFlow::Continue
    });
    main_loop.run();
}
