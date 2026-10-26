//! Commands the web pages can call.

use crate::overlays;
use crate::state::Shell;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize)]
pub struct CommandError {
    code: String,
    message: String,
}

impl CommandError {
    fn failed(message: impl Into<String>) -> Self {
        Self { code: "failed".into(), message: message.into() }
    }
}

/// The engine's JSON-RPC. Runs off the UI thread because some calls touch the disk.
#[tauri::command]
pub async fn rpc(state: State<'_, Shell>, method: String, params: Option<Value>) -> Result<Value, CommandError> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.call(&method, params.unwrap_or(Value::Null)))
        .await
        .map_err(|e| CommandError::failed(format!("The engine stopped unexpectedly ({e}).")))?
        .map_err(|e| CommandError { code: e.code, message: e.message })
}

/// An overlay page reports how big its content is so the window can fit it exactly.
#[tauri::command]
pub async fn overlay_size(app: AppHandle, id: String, width: f64, height: f64) {
    overlays::set_size(&app, &id, width, height);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    name: String,
    label: String,
    primary: bool,
    width: u32,
    height: u32,
}

#[tauri::command]
pub async fn list_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, CommandError> {
    let monitors = app.available_monitors().map_err(|e| CommandError::failed(e.to_string()))?;
    let primary = app.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    let mut out = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let name = m.name().cloned().unwrap_or_default();
        let is_primary = primary.as_deref() == Some(name.as_str()) || (primary.is_none() && i == 0);
        let size = m.size();
        out.push(MonitorInfo {
            label: if is_primary { format!("Main screen ({}×{})", size.width, size.height) } else { format!("Screen {} ({}×{})", i + 1, size.width, size.height) },
            name,
            primary: is_primary,
            width: size.width,
            height: size.height,
        });
    }
    Ok(out)
}

/// "Save as…" for exports. Returns false when the user cancelled.
#[tauri::command]
pub async fn save_text_file(app: AppHandle, filename: String, text: String) -> Result<bool, CommandError> {
    let ext = std::path::Path::new(&filename).extension().and_then(|e| e.to_str()).unwrap_or("json").to_string();
    let handle = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || handle.dialog().file().set_file_name(&filename).add_filter("File", &[ext.as_str()]).blocking_save_file())
        .await
        .map_err(|e| CommandError::failed(e.to_string()))?;
    let Some(picked) = picked else { return Ok(false) };
    let path = picked.into_path().map_err(|e| CommandError::failed(e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| CommandError::failed(format!("The file could not be saved: {e}")))?;
    Ok(true)
}
