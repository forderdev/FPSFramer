//! FPSFramer desktop shell.
//!
//! The engine (`fpsframer-engine`) owns all behaviour. This crate only does what needs the
//! operating system's windowing: the main window, transparent overlay windows, the tray icon,
//! global shortcuts, notifications and start-with-Windows.

mod commands;
mod hotkeys;
mod main_window;
mod overlays;
mod state;
mod tray;

use crossbeam_channel::Receiver;
use fpsframer_core::model::Settings;
use fpsframer_core::storage::Paths;
use fpsframer_engine::types::{HotkeyBinding, Notification, OverlaySpec, TrayState};
use fpsframer_engine::{Engine, EngineOptions, Event};
use state::{Prefs, Shell};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_notification::NotificationExt;

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|a| a == "--demo") || std::env::var("FPSFRAMER_DEMO").is_ok_and(|v| v == "1");
    let autostarted = args.iter().any(|a| a == "--autostart");

    let engine = Engine::new(EngineOptions { paths: Paths::from_env(), demo, version: env!("CARGO_PKG_VERSION").to_string() });
    // Keep the default message on stderr, and also put panics in the log file.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default_hook(info);
    }));

    let engine_for_exit = engine.clone();
    let engine_for_setup = engine.clone();

    let app = tauri::Builder::default()
        // Must be first: a second launch just brings the running app forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| main_window::show(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, shortcut, event| hotkeys::pressed(app, shortcut, event.state())).build())
        .invoke_handler(tauri::generate_handler![commands::rpc, commands::overlay_size, commands::list_monitors, commands::save_text_file])
        .on_window_event(on_window_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            app.manage(Shell::new(engine_for_setup.clone(), demo));

            // Two independent subscribers: one feeds the web pages, one performs native effects,
            // so a slow window operation can never delay the live numbers.
            let ui_rx = engine_for_setup.subscribe();
            let native_rx = engine_for_setup.subscribe();
            forward_to_pages(handle.clone(), ui_rx);
            native_effects(handle.clone(), native_rx);
            overlays::start_upkeep(&handle);
            engine_for_setup.start();

            tray::create(&handle)?;
            let (first_run, start_minimized) = sync_initial_state(&handle);
            // Autostart and "start minimised" go straight to the tray, except on the very first run.
            if first_run || !(autostarted || start_minimized) {
                main_window::create(&handle)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("FPSFramer could not start its window system");

    app.run(move |app, event| match event {
        // Closing the last window keeps FPSFramer running in the tray; Quit (and `exit`) still end it.
        RunEvent::ExitRequested { api, code, .. } => {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            let _ = app;
            engine_for_exit.shutdown();
        }
        _ => {}
    });
}

fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    let label = window.label();
    let app = window.app_handle();
    if label == main_window::LABEL {
        match event {
            WindowEvent::CloseRequested { .. } => {
                // With "keep running in the tray" the window is simply destroyed; otherwise closing it quits.
                if !app.state::<Shell>().prefs.lock().close_to_tray {
                    app.exit(0);
                }
            }
            WindowEvent::Destroyed => app.state::<Shell>().engine.set_ui_visible(false),
            _ => {}
        }
    } else if label.starts_with(overlays::PREFIX) {
        match event {
            WindowEvent::Moved(_) => overlays::on_moved(app, label),
            WindowEvent::Destroyed => {
                overlays::forget(app, label);
                app.state::<Shell>().engine.set_ui_visible(false);
            }
            _ => {}
        }
    }
}

/// Everything the engine publishes goes to every web page (main window and overlays alike).
fn forward_to_pages(app: AppHandle, rx: Receiver<Arc<Event>>) {
    std::thread::spawn(move || {
        for ev in rx.iter() {
            let _ = app.emit("engine-event", &*ev);
        }
    });
}

/// Events that need the operating system rather than a web page.
fn native_effects(app: AppHandle, rx: Receiver<Arc<Event>>) {
    std::thread::spawn(move || {
        for ev in rx.iter() {
            match &*ev {
                Event::Overlays(specs) => overlays::reconcile(&app, specs),
                Event::Hotkeys(b) => hotkeys::apply(&app, b),
                Event::Tray(t) => tray::update(&app, t),
                Event::Notify(n) => notify(&app, n),
                Event::Settings(s) => settings_changed(&app, s),
                Event::Quit => app.exit(0),
                _ => {}
            }
        }
    });
}

fn notify(app: &AppHandle, n: &Notification) {
    let mut b = app.notification().builder().title(&n.title).body(&n.body);
    if n.sound {
        b = b.sound("Default");
    }
    if let Err(e) = b.show() {
        log::warn!("Could not show a notification: {e}");
    }
}

fn settings_changed(app: &AppHandle, s: &Settings) {
    let shell = app.state::<Shell>();
    *shell.prefs.lock() = Prefs::from(s);
    if shell.demo {
        return; // never register a demo build to start with Windows
    }
    let launch = app.autolaunch();
    if let Ok(current) = launch.is_enabled() {
        if current != s.launch_at_startup {
            let r = if s.launch_at_startup { launch.enable() } else { launch.disable() };
            if let Err(e) = r {
                log::warn!("Could not change start-with-Windows: {e}");
            }
        }
    }
}

/// The engine already published its first overlays/hotkeys/tray before we subscribed; ask for
/// them once so nothing is missed. Returns `(first run, start minimised)`.
fn sync_initial_state(app: &AppHandle) -> (bool, bool) {
    let shell = app.state::<Shell>();
    let Ok(state) = shell.engine.call("app.getState", serde_json::Value::Null) else { return (false, false) };
    let mut start_minimized = false;
    if let Some(s) = state.get("settings").and_then(|v| serde_json::from_value::<Settings>(v.clone()).ok()) {
        *shell.prefs.lock() = Prefs::from(&s);
        start_minimized = s.start_minimized;
        settings_changed(app, &s);
    }
    if let Some(o) = state.get("overlays").and_then(|v| serde_json::from_value::<Vec<OverlaySpec>>(v.clone()).ok()) {
        let app = app.clone();
        std::thread::spawn(move || overlays::reconcile(&app, &o));
    }
    if let Some(h) = state.get("hotkeys").and_then(|v| serde_json::from_value::<Vec<HotkeyBinding>>(v.clone()).ok()) {
        hotkeys::apply(app, &h);
    }
    if let Some(t) = state.get("tray").and_then(|v| serde_json::from_value::<TrayState>(v.clone()).ok()) {
        tray::update(app, &t);
    }
    (state.get("firstRun").and_then(|v| v.as_bool()).unwrap_or(false), start_minimized)
}
