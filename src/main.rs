use decklock::{auth, config, controller, i18n, lock, session, settings, shortcut, ui};

use clap::Parser;
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    version,
    about = "Customizable Wayland screen locker. Defaults to a safe preview."
)]
struct Args {
    /// Open a normal window; authentication and power actions are disabled.
    #[arg(long, conflicts_with = "lock")]
    preview: bool,
    /// Open the configuration window (never acquires a lock).
    #[arg(long, conflicts_with_all = ["lock", "preview", "toggle_keyboard", "check_config"])]
    settings: bool,
    /// Acquire a real Wayland session lock (requires a supported compositor).
    #[arg(long)]
    lock: bool,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    theme: Option<PathBuf>,
    #[arg(long)]
    locale: Option<String>,
    #[arg(long)]
    translations: Option<PathBuf>,
    #[arg(long)]
    background: Option<PathBuf>,
    /// Show the embedded keyboard initially.
    #[arg(long)]
    keyboard: bool,
    /// Start preview in idle mode.
    #[arg(long, conflicts_with = "lock")]
    preview_idle: bool,
    /// Fill the monitor in preview; Escape still exits without unlocking anything.
    #[arg(long, conflicts_with = "lock")]
    preview_fullscreen: bool,
    /// Explicitly enable optional controller integration (also in preview).
    #[arg(long)]
    controller_socket: Option<PathBuf>,
    /// Enable sc-controller at its default socket and route deck-osk --toggle here.
    #[arg(long)]
    controller: bool,
    /// Toggle the embedded keyboard of an already running DeckLock instance.
    #[arg(long, conflicts_with_all = ["lock", "preview"])]
    toggle_keyboard: bool,
    /// Validate configuration/catalogs without opening a display.
    #[arg(long, conflicts_with = "lock")]
    check_config: bool,
    /// Close preview automatically, for visual smoke tests only.
    #[arg(long, conflicts_with="lock", value_parser=clap::value_parser!(u64).range(1..=300))]
    preview_exit_after: Option<u64>,
}

fn main() {
    let raw: Vec<_> = std::env::args_os().collect();
    if raw.get(1).is_some_and(|arg| arg == "--auth-helper") {
        let code = if raw.len() == 3 {
            raw[2].to_str().map(auth::helper_main).unwrap_or(1)
        } else {
            1
        };
        std::process::exit(code);
    }
    if let Err(error) = run(Args::parse()) {
        eprintln!("DeckLock: {error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    if args.toggle_keyboard {
        return shortcut::toggle();
    }
    if args.settings {
        let path = args.config.unwrap_or(
            shortcut::config_dir()
                .ok_or("Cannot locate configuration")?
                .join("decklock/config.toml"),
        );
        return settings::run(path, args.locale);
    }
    let default_config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .map(|p| p.join("decklock/config.toml"))
        .filter(|p| p.is_file());
    let mut config = config::Config::load(args.config.as_deref().or(default_config.as_deref()))?;
    if let Some(theme) = args.theme {
        config.theme = Some(theme);
    }
    if let Some(locale) = args.locale {
        config.locale = Some(locale);
    }
    if let Some(path) = args.background {
        config.background = Some(path);
    }
    // Preview never uses the saved controller socket without explicit opt-in.
    if args.controller_socket.is_some() || !args.lock {
        config.controller_socket = args.controller_socket;
    }
    if args.controller && config.controller_socket.is_none() {
        config.controller_socket = Some(
            shortcut::config_dir()
                .ok_or("Cannot locate sc-controller configuration")?
                .join("scc/daemon.socket"),
        );
    }
    let shortcut_dir = if args.lock || config.controller_socket.is_some() {
        shortcut::config_dir().map(|p| p.join("scc"))
    } else {
        None
    };
    let mut theme = config::Theme::load(config.theme.as_deref())?;
    if let Some(layout) = &config.layout {
        theme.layout = layout.clone();
    }
    let strings = i18n::I18n::new(config.locale.as_deref(), args.translations.as_deref())?;
    if args.check_config {
        println!(
            "Configuration and translations valid; theme: {}",
            theme.name
        );
        return Ok(());
    }
    // A locker handles passwords: never include its address space in a core dump.
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid pointer to a stack rlimit; no ownership transfer.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        return Err("Could not disable core dumps".into());
    }
    gtk::init().map_err(|e| e.to_string())?;
    if args.lock && !gtk4_session_lock::is_supported() {
        return Err(strings.text("unsupported"));
    }
    ui::apply_css(&theme.css)?;
    let settings = Rc::new(ui::Settings {
        config,
        theme,
        strings,
        preview: !args.lock,
        show_keyboard: args.keyboard,
        start_idle: args.preview_idle,
        username: auth::current_username()?,
    });
    let app = gtk::Application::new(
        Some("io.github.yan_vidal.DeckLock"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    let views: Rc<RefCell<Vec<ui::View>>> = Rc::new(RefCell::new(Vec::new()));
    let session = Rc::new(RefCell::new(session::Session::new(settings.preview)));
    if let Some(path) = &settings.config.controller_socket {
        let client = controller::ControllerClient::connect(path.clone())?;
        let targets = Rc::downgrade(&views);
        let mut active = false;
        let cooldown_dir = shortcut_dir.clone();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(targets) = targets.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let views = targets.borrow();
            let target = views.iter().find(|v| v.window.is_active());
            let should_capture =
                target.is_some_and(|v| v.keyboard.is_visible() && v.keyboard.is_sensitive());
            if should_capture != active {
                if active && let Some(dir) = &cooldown_dir {
                    shortcut::cooldown(dir);
                }
                let _ = client.set_active(should_capture);
                active = should_capture;
            }
            for event in client.events.try_iter().take(128) {
                if let Some(view) = target {
                    if matches!(
                        event,
                        controller::ControllerEvent::Pad { .. }
                            | controller::ControllerEvent::Button { .. }
                            | controller::ControllerEvent::Trigger { .. }
                    ) {
                        view.activity.set(std::time::Instant::now());
                    }
                    (view.controller_event)(event);
                }
            }
            glib::ControlFlow::Continue
        });
    }
    let (tx, rx) = mpsc::channel();
    let submit: Rc<dyn Fn(Zeroizing<String>)> = {
        let session = session.clone();
        let settings = settings.clone();
        let views = Rc::downgrade(&views);
        Rc::new(move |password| {
            let Some(attempt) = session.borrow_mut().begin_auth() else {
                return;
            };
            if let Some(views) = views.upgrade() {
                for view in views.borrow().iter() {
                    view.busy(true, &settings.strings.text("authenticating"));
                }
            }
            let tx = tx.clone();
            let service = settings.config.pam_service.clone();
            std::thread::spawn(move || {
                let result = auth::authenticate(password, &service);
                let _ = tx.send((attempt, result));
            });
        })
    };
    let exit_code = Rc::new(Cell::new(0));
    let lock = if args.lock {
        Some(lock::configure(
            &app,
            settings.clone(),
            session.clone(),
            views.clone(),
            submit.clone(),
            exit_code.clone(),
        ))
    } else {
        None
    };
    let app_weak = app.downgrade();
    let result_views = views.clone();
    let result_settings = settings.clone();
    let result_session = session.clone();
    let result_lock = lock.clone();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        if app_weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        for (attempt, result) in rx.try_iter() {
            let ok = result.unwrap_or(false);
            if result_session.borrow_mut().complete_auth(attempt, ok) {
                if let Some(lock) = &result_lock {
                    lock.unlock();
                }
            } else if !result_session.borrow().is_authenticating() {
                for view in result_views.borrow().iter() {
                    view.busy(
                        false,
                        &result_settings.strings.text("authentication-failed"),
                    );
                    view.entry.grab_focus();
                }
            }
        }
        glib::ControlFlow::Continue
    });
    let signal_views = views.clone();
    glib_unix::unix_signal_add_local(libc::SIGUSR1, move || {
        let views = signal_views.borrow();
        if let Some(view) = views
            .iter()
            .find(|v| v.window.is_active())
            .or_else(|| views.first())
        {
            view.keyboard.set_visible(!view.keyboard.is_visible());
        }
        glib::ControlFlow::Continue
    });
    // No SIGTERM/SIGINT handlers: process termination cannot invoke unlock.
    let hold = if args.lock { Some(app.hold()) } else { None };
    app.connect_activate(move |app| {
        if let Some(lock) = &lock {
            lock.lock();
        } else {
            let view = ui::build(app, settings.clone(), submit.clone());
            let app_weak = app.downgrade();
            view.window.connect_close_request(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.quit();
                }
                glib::Propagation::Proceed
            });
            if args.preview_fullscreen {
                view.window.fullscreen();
            }
            view.window.present();
            views.borrow_mut().push(view);
            if let Some(seconds) = args.preview_exit_after {
                let app = app.downgrade();
                glib::timeout_add_seconds_local_once(seconds as u32, move || {
                    if let Some(app) = app.upgrade() {
                        app.quit();
                    }
                });
            }
        }
    });
    // Register only after SIGUSR1 has a handler. Preview opts in with --controller.
    let _shortcut_registration = shortcut_dir
        .map(|dir| shortcut::Registration::acquire(dir.join("deck-lock.pid")))
        .transpose()?;
    app.run_with_args::<&str>(&[]);
    session.borrow_mut().terminate();
    drop(hold);
    if exit_code.get() != 0 {
        return Err("Could not acquire session lock".into());
    }
    Ok(())
}
