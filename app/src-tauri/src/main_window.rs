//! The main application window. It is created on demand and destroyed when closed to the tray,
//! so a hidden FPSFramer doesn't keep a web view (and its memory) alive.

use crate::state::Shell;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "main";

/// Bring the window to the front, creating it if it doesn't exist. Safe to call from any thread.
pub fn show(app: &AppHandle) {
    let app = app.clone();
    // Creating a window blocks until the UI thread has built it, so never do it on the UI thread.
    std::thread::spawn(move || {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        } else if let Err(e) = create(&app) {
            log::error!("Could not open the main window: {e}");
        }
    });
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("FPSFramer")
        .inner_size(1240.0, 800.0)
        .min_inner_size(960.0, 620.0)
        .center()
        .build()?;
    app.state::<Shell>().engine.set_ui_visible(true);
    Ok(())
}
