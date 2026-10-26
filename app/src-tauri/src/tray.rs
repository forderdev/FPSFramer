//! The notification-area icon and its menu. The menu mirrors the engine's [`TrayState`], so it
//! always shows what is really on (overlay, recording, current style).

use crate::main_window;
use crate::state::Shell;
use fpsframer_engine::types::TrayState;
use serde_json::json;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

fn build_menu(app: &AppHandle, t: &TrayState) -> tauri::Result<Menu<Wry>> {
    let status = match (&t.game, t.recording) {
        (Some(g), true) => format!("Recording {g}"),
        (Some(g), false) => format!("Watching {g}"),
        (None, true) => "Recording".to_string(),
        (None, false) => "Waiting for a game".to_string(),
    };
    let header = MenuItem::with_id(app, "status", status, false, None::<&str>)?;
    let profile = MenuItem::with_id(app, "profile", format!("Profile: {}", t.profile_name), false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open FPSFramer", true, None::<&str>)?;
    let overlay = CheckMenuItem::with_id(app, "toggleOverlay", "Show the overlay in games", true, t.overlay_enabled, None::<&str>)?;
    let rec = MenuItem::with_id(app, "toggleRecording", if t.recording { "Stop recording" } else { "Start recording" }, true, None::<&str>)?;
    let shot = MenuItem::with_id(app, "screenshot", "Take a screenshot", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit FPSFramer", true, None::<&str>)?;

    let scene_items: Vec<CheckMenuItem<Wry>> = t
        .scenes
        .iter()
        .map(|s| CheckMenuItem::with_id(app, format!("scene:{}", s.id), &s.name, true, s.active, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let scene_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = scene_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    let scenes = Submenu::with_items(app, "Overlay style", scene_refs.len() > 1, &scene_refs)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&header, &profile, &sep1, &open, &overlay, &scenes, &rec, &shot, &sep2, &sep3, &quit])
}

fn tooltip(t: &TrayState) -> String {
    match &t.game {
        Some(g) if t.alert_count > 0 => format!("FPSFramer: {g} ({} alerts)", t.alert_count),
        Some(g) => format!("FPSFramer: {g}"),
        None => "FPSFramer".to_string(),
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let initial = TrayState {
        overlay_enabled: true,
        recording: false,
        profile_id: String::new(),
        profile_name: "All games".into(),
        scenes: vec![],
        game: None,
        alert_count: 0,
    };
    let menu = build_menu(app, &initial)?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("FPSFramer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                main_window::show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    *app.state::<Shell>().tray.lock() = Some(tray);
    Ok(())
}

pub fn update(app: &AppHandle, t: &TrayState) {
    let shell = app.state::<Shell>();
    *shell.last_tray.lock() = Some(t.clone());
    let Ok(menu) = build_menu(app, t) else { return };
    let guard = shell.tray.lock();
    if let Some(tray) = guard.as_ref() {
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(tooltip(t)));
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    // Menu events arrive on the UI thread; the engine and window creation must not run there.
    std::thread::spawn(move || {
        let shell = app.state::<Shell>();
        match id.as_str() {
            "open" => main_window::show(&app),
            "quit" => app.exit(0),
            "toggleOverlay" | "toggleRecording" | "screenshot" => shell.engine.action(&id),
            s if s.starts_with("scene:") => {
                let profile = shell.last_tray.lock().as_ref().map(|t| t.profile_id.clone()).unwrap_or_default();
                if let Err(e) = shell.engine.call("profiles.setScene", json!({ "profileId": profile, "sceneId": &s[6..] })) {
                    log::warn!("Could not switch the overlay style: {}", e.message);
                }
            }
            _ => {}
        }
    });
}
