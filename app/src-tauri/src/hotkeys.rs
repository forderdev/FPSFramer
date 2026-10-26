//! Global keyboard shortcuts. The engine decides which exist; this registers them with Windows.

use crate::main_window;
use crate::state::Shell;
use fpsframer_engine::types::HotkeyBinding;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Replace every registered shortcut with `bindings`. Failures are reported, not fatal.
///
/// The engine announces its shortcuts at startup and again whenever they change, and the shell also asks for
/// them once itself, so this is called from several threads. It must therefore be serialised (two interleaved
/// runs make every registration after the first fail with "already registered" and leave an empty action
/// table, which silently disables all shortcuts) and it does nothing when the list hasn't changed.
pub fn apply(app: &AppHandle, bindings: &[HotkeyBinding]) {
    let shell = app.state::<Shell>();
    let _one_at_a_time = shell.hotkey_lock.lock();
    let wanted: Vec<(String, String)> = bindings.iter().map(|b| (b.action.clone(), b.accelerator.clone())).collect();
    if *shell.applied_hotkeys.lock() == wanted && !shell.hotkeys.lock().is_empty() {
        return;
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut map: HashMap<u32, String> = HashMap::new();
    let mut failed: Vec<String> = Vec::new();
    for b in bindings {
        match b.accelerator.parse::<Shortcut>() {
            Ok(sc) => match gs.register(sc) {
                Ok(()) => {
                    map.insert(sc.id(), b.action.clone());
                }
                Err(e) => {
                    log::warn!("Shortcut {} could not be registered: {e}", b.accelerator);
                    failed.push(b.accelerator.clone());
                }
            },
            Err(_) => failed.push(b.accelerator.clone()),
        }
    }
    log::info!("Global shortcuts registered: {}{}", map.len(), if failed.is_empty() { String::new() } else { format!(" (failed: {})", failed.join(", ")) });
    *shell.hotkeys.lock() = map;
    *shell.applied_hotkeys.lock() = wanted;
    if !failed.is_empty() {
        // Shown as an ordinary in-app toast through the same channel the engine uses.
        let _ = app.emit(
            "engine-event",
            serde_json::json!({
                "type": "toast",
                "data": {
                    "level": "warning",
                    "message": format!("These shortcuts couldn't be set because another program may already use them: {}. Pick different ones in Settings → Hotkeys.", failed.join(", ")),
                    "action": { "label": "Open hotkey settings", "route": "/settings/hotkeys" }
                }
            }),
        );
    }
}

/// A registered shortcut was pressed.
pub fn pressed(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }
    let action = app.state::<Shell>().hotkeys.lock().get(&shortcut.id()).cloned();
    let Some(action) = action else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        if action == "openApp" {
            main_window::show(&app);
        } else {
            app.state::<Shell>().engine.action(&action);
        }
    });
}
