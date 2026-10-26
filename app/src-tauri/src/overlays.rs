//! Overlay windows: make reality match the engine's list of [`OverlaySpec`]s.
//!
//! Each overlay is a borderless, transparent, always-on-top window that never takes focus and
//! (normally) lets clicks through to the game. It is a normal window above the game — nothing is
//! injected — so it cannot appear over exclusive-fullscreen games.
//!
//! Deadlock rule: never hold the `overlays` lock while calling a window method. Window creation
//! waits for the UI thread, and UI-thread event handlers take that lock.

use crate::state::Shell;
use fpsframer_core::model::Anchor;
use fpsframer_engine::types::OverlaySpec;
use serde_json::json;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const PREFIX: &str = "overlay-";

pub fn label_of(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    format!("{PREFIX}{safe}")
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Raw native window handle, where the platform has one.
pub fn native_handle(w: &WebviewWindow) -> Option<isize> {
    #[cfg(windows)]
    {
        // `HWND` is a one-field wrapper; casting its content works whichever integer/pointer type it holds.
        w.hwnd().ok().map(|h| h.0 as isize)
    }
    #[cfg(not(windows))]
    {
        let _ = w;
        None
    }
}

// ───────────────────────────── reconcile ─────────────────────────────

/// Create, update and remove overlay windows so they match `specs`.
pub fn reconcile(app: &AppHandle, specs: &[OverlaySpec]) {
    // Startup and the event pump both call this. Run one at a time: otherwise one pass destroys the window the
    // other is just creating, leaving a duplicate or a hidden leftover window.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let shell = app.state::<Shell>();
    let wanted: HashSet<String> = specs.iter().map(|s| label_of(&s.id)).collect();

    // Remove windows the engine no longer wants.
    let stale: Vec<String> = {
        let mut b = shell.overlays.lock();
        let stale: Vec<String> = b.specs.keys().filter(|l| !wanted.contains(*l)).cloned().collect();
        for l in &stale {
            b.specs.remove(l);
            b.sizes.remove(l);
            b.shown.remove(l);
            b.ignore_moves_until.remove(l);
            b.pending_move.remove(l);
        }
        stale
    };
    for l in stale {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.destroy();
        }
    }

    for spec in specs {
        let label = label_of(&spec.id);
        let previous = {
            let mut b = shell.overlays.lock();
            b.specs.insert(label.clone(), spec.clone())
        };
        match app.get_webview_window(&label) {
            None => match create(app, spec) {
                Ok(w) => {
                    log::info!("Overlay window \"{label}\" created (layout {}, monitor \"{}\", anchor {:?}).", spec.layout_id, spec.monitor, spec.anchor);
                    shell.engine.set_ui_visible(true);
                    apply_style(&w, spec, false);
                    // For debugging on a developer machine: FPSFRAMER_DEVTOOLS=1 (debug builds) opens the web inspector.
                    #[cfg(debug_assertions)]
                    if std::env::var("FPSFRAMER_DEVTOOLS").is_ok_and(|v| v == "1") {
                        w.open_devtools();
                    }
                    // It is shown once its page reports a size (see `set_size`).
                }
                Err(e) => log::error!("Could not create the overlay window: {e}"),
            },
            Some(w) => {
                if previous.as_ref() != Some(spec) {
                    let shown = shell.overlays.lock().shown.contains(&label);
                    apply_style(&w, spec, shown);
                    place(app, &label);
                }
            }
        }
    }
}

fn create(app: &AppHandle, spec: &OverlaySpec) -> tauri::Result<WebviewWindow> {
    let url = WebviewUrl::App(format!("overlay.html?id={}", percent_encode(&spec.id)).into());
    WebviewWindowBuilder::new(app, label_of(&spec.id), url)
        .title("FPSFramer overlay")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(spec.always_on_top)
        .focused(false)
        .focusable(false)
        .visible(false)
        .inner_size(10.0, 10.0)
        .build()
}

/// Everything that depends on the spec but not on the size or position.
fn apply_style(w: &WebviewWindow, spec: &OverlaySpec, shown: bool) {
    // Some platforms can only change mouse pass-through once the window has been shown.
    if shown {
        let _ = w.set_ignore_cursor_events(spec.click_through);
    }
    let _ = w.set_always_on_top(spec.always_on_top);
    if let Some(h) = native_handle(w) {
        fpsframer_platform::overlay_window::make_unobtrusive(h);
        if let Err(e) = fpsframer_platform::overlay_window::exclude_from_capture(h, spec.exclude_from_capture) {
            if spec.exclude_from_capture {
                log::warn!("{e}");
            }
        }
    }
}

// ───────────────────────────── size and position ─────────────────────────────

/// An overlay page tells us how big its content is. The window is fitted to it and shown.
pub fn set_size(app: &AppHandle, id: &str, width: f64, height: f64) {
    if !(width.is_finite() && height.is_finite()) || width < 1.0 || height < 1.0 {
        return;
    }
    let label = label_of(id);
    let shell = app.state::<Shell>();
    {
        let mut b = shell.overlays.lock();
        if !b.specs.contains_key(&label) {
            return;
        }
        let w = width.min(4000.0);
        let h = height.min(4000.0);
        if b.sizes.get(&label) == Some(&(w, h)) {
            return;
        }
        if !b.sizes.contains_key(&label) {
            log::info!("Overlay page \"{label}\" reported its size: {w:.0}x{h:.0} px.");
        }
        b.sizes.insert(label.clone(), (w, h));
    }
    place(app, &label);
}

fn monitor_named<'a>(monitors: &'a [Monitor], name: &str) -> Option<&'a Monitor> {
    monitors.iter().find(|m| m.name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// Which column / row (0, 1, 2) an anchor means.
fn anchor_cell(a: Anchor) -> (u8, u8) {
    match a {
        Anchor::TopLeft => (0, 0),
        Anchor::TopCenter => (1, 0),
        Anchor::TopRight => (2, 0),
        Anchor::MiddleLeft => (0, 1),
        Anchor::Center => (1, 1),
        Anchor::MiddleRight => (2, 1),
        Anchor::BottomLeft => (0, 2),
        Anchor::BottomCenter => (1, 2),
        Anchor::BottomRight => (2, 2),
    }
}

fn anchor_from_cell(col: u8, row: u8) -> &'static str {
    match (col, row) {
        (0, 0) => "top-left",
        (1, 0) => "top-center",
        (2, 0) => "top-right",
        (0, 1) => "middle-left",
        (1, 1) => "center",
        (2, 1) => "middle-right",
        (0, 2) => "bottom-left",
        (1, 2) => "bottom-center",
        _ => "bottom-right",
    }
}

/// Physical position of a window of `size` for an anchor + offset (logical pixels) on a monitor.
fn position_for(spec: &OverlaySpec, mon: &Monitor, size: (f64, f64)) -> (i32, i32) {
    let sf = mon.scale_factor();
    let (mp, ms) = (mon.position(), mon.size());
    let (col, row) = anchor_cell(spec.anchor);
    let (ox, oy) = (spec.offset_x * sf, spec.offset_y * sf);
    let free_w = ms.width as f64 - size.0;
    let free_h = ms.height as f64 - size.1;
    let x = match col {
        0 => mp.x as f64 + ox,
        1 => mp.x as f64 + free_w / 2.0 + ox,
        _ => mp.x as f64 + free_w - ox,
    };
    let y = match row {
        0 => mp.y as f64 + oy,
        1 => mp.y as f64 + free_h / 2.0 + oy,
        _ => mp.y as f64 + free_h - oy,
    };
    (x.round() as i32, y.round() as i32)
}

/// Fit the window to its content and put it where the spec says. Shows it the first time.
pub fn place(app: &AppHandle, label: &str) {
    let shell = app.state::<Shell>();
    let (spec, size) = {
        let b = shell.overlays.lock();
        (b.specs.get(label).cloned(), b.sizes.get(label).copied())
    };
    let (Some(spec), Some((cw, ch))) = (spec, size) else { return };
    let Some(win) = app.get_webview_window(label) else { return };
    let monitors = win.available_monitors().unwrap_or_default();
    let primary = win.primary_monitor().ok().flatten();
    let named = monitor_named(&monitors, &spec.monitor).cloned();
    if named.is_none() && spec.monitor != "primary" {
        log::warn!(
            "Overlay \"{label}\": no screen is called \"{}\" (screens: {}); using the main screen.",
            spec.monitor,
            monitors.iter().map(|m| m.name().cloned().unwrap_or_default()).collect::<Vec<_>>().join(", ")
        );
    }
    let Some(mon) = named.or(primary).or_else(|| monitors.first().cloned()) else { return };

    let sf = mon.scale_factor();
    let (pw, ph) = ((cw * sf).ceil().max(1.0), (ch * sf).ceil().max(1.0));
    let (x, y) = position_for(&spec, &mon, (pw, ph));

    shell.overlays.lock().ignore_moves_until.insert(label.to_string(), Instant::now() + Duration::from_millis(700));
    // Move onto the monitor first so the window adopts that monitor's scaling, then size, then final position.
    let _ = win.set_position(PhysicalPosition::new(mon.position().x, mon.position().y));
    let _ = win.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = win.set_position(PhysicalPosition::new(x, y));

    let first = shell.overlays.lock().shown.insert(label.to_string());
    log::info!(
        "Overlay \"{label}\" placed on screen {:?} at ({x}, {y}), {pw:.0}x{ph:.0} px, scale {sf}{}.",
        mon.name().cloned().unwrap_or_default(),
        if first { ", shown for the first time" } else { "" }
    );
    if first {
        // Show through the toolkit, not with a raw `ShowWindow`: tao keeps its own visibility flag and re-applies
        // the window style whenever another flag changes (click-through, always-on-top), which would silently
        // clear `WS_VISIBLE` of a window it believes is hidden. The window is non-focusable, so this takes no focus.
        if let Err(e) = win.show() {
            log::warn!("Overlay \"{label}\" could not be shown: {e}");
        }
        if spec.always_on_top {
            let _ = win.set_always_on_top(true);
        }
        let _ = win.set_ignore_cursor_events(spec.click_through);
        // Showing and the flag changes above make tao rewrite the extended style; put our bits back.
        if let Some(h) = native_handle(&win) {
            fpsframer_platform::overlay_window::make_unobtrusive(h);
        }
    }
}

// ───────────────────────────── dragging (edit mode) ─────────────────────────────

/// The user moved an overlay window (called on the UI thread; must stay cheap).
pub fn on_moved(app: &AppHandle, label: &str) {
    let shell = app.state::<Shell>();
    let mut b = shell.overlays.lock();
    let editing = b.specs.get(label).is_some_and(|s| s.edit_mode);
    if !editing || b.ignore_moves_until.get(label).is_some_and(|t| Instant::now() < *t) {
        return;
    }
    b.pending_move.insert(label.to_string(), Instant::now());
}

/// Save where the window ended up as anchor + offset on the monitor it is on.
fn persist_position(app: &AppHandle, label: &str) {
    let shell = app.state::<Shell>();
    let spec = shell.overlays.lock().specs.get(label).cloned();
    let Some(spec) = spec else { return };
    let Some(win) = app.get_webview_window(label) else { return };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return };
    let monitors = win.available_monitors().unwrap_or_default();
    let (cx, cy) = (pos.x + size.width as i32 / 2, pos.y + size.height as i32 / 2);
    let contains = |m: &Monitor| {
        let (p, s) = (m.position(), m.size());
        cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32
    };
    let Some(mon) = monitors.iter().find(|m| contains(m)).cloned().or_else(|| win.primary_monitor().ok().flatten()) else { return };
    let (mp, ms, sf) = (mon.position(), mon.size(), mon.scale_factor());

    // The nearest of the nine anchors, so the overlay keeps its place if the resolution changes.
    let fx = (cx - mp.x) as f64 / ms.width.max(1) as f64;
    let fy = (cy - mp.y) as f64 / ms.height.max(1) as f64;
    let col = if fx < 1.0 / 3.0 { 0 } else if fx < 2.0 / 3.0 { 1 } else { 2 };
    let row = if fy < 1.0 / 3.0 { 0 } else if fy < 2.0 / 3.0 { 1 } else { 2 };
    let free_w = ms.width as i32 - size.width as i32;
    let free_h = ms.height as i32 - size.height as i32;
    let ox = match col {
        0 => pos.x - mp.x,
        1 => pos.x - (mp.x + free_w / 2),
        _ => (mp.x + free_w) - pos.x,
    };
    let oy = match row {
        0 => pos.y - mp.y,
        1 => pos.y - (mp.y + free_h / 2),
        _ => (mp.y + free_h) - pos.y,
    };
    let same_monitor = mon.name().is_some_and(|n| n.eq_ignore_ascii_case(&spec.monitor));
    let params = json!({
        "id": spec.id,
        "anchor": anchor_from_cell(col, row),
        "offsetX": (ox as f64 / sf).round(),
        "offsetY": (oy as f64 / sf).round(),
        "monitor": if same_monitor { None } else { mon.name().cloned() },
    });
    if let Err(e) = shell.engine.call("overlay.setPosition", params) {
        log::warn!("Could not save the overlay position: {}", e.message);
    }
}

// ───────────────────────────── background upkeep ─────────────────────────────

/// Two small loops: save positions after a drag settles, and keep overlays above the game.
pub fn start_upkeep(app: &AppHandle) {
    let a = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(250));
        let due: Vec<String> = {
            let shell = a.state::<Shell>();
            let mut b = shell.overlays.lock();
            let due: Vec<String> = b.pending_move.iter().filter(|(_, t)| t.elapsed() > Duration::from_millis(450)).map(|(l, _)| l.clone()).collect();
            for l in &due {
                b.pending_move.remove(l);
            }
            due
        };
        for l in due {
            persist_position(&a, &l);
        }
    });

    let a = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1500));
        let labels: Vec<String> = {
            let shell = a.state::<Shell>();
            let b = shell.overlays.lock();
            b.specs.iter().filter(|(l, s)| s.reassert_topmost && s.always_on_top && b.shown.contains(*l)).map(|(l, _)| l.clone()).collect()
        };
        for l in labels {
            if let Some(h) = a.get_webview_window(&l).and_then(|w| native_handle(&w)) {
                fpsframer_platform::overlay_window::assert_topmost(h);
            }
        }
    });
}

/// A window was destroyed (by us or otherwise).
pub fn forget(app: &AppHandle, label: &str) {
    let shell = app.state::<Shell>();
    let mut b = shell.overlays.lock();
    b.shown.remove(label);
    b.sizes.remove(label);
    b.pending_move.remove(label);
    b.ignore_moves_until.remove(label);
}
