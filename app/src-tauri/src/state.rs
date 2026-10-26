//! Shared state of the native shell.

use fpsframer_core::model::Settings;
use fpsframer_engine::types::{OverlaySpec, TrayState};
use fpsframer_engine::Engine;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tauri::tray::TrayIcon;

/// The few settings the shell itself needs to know about.
#[derive(Default, Clone, Copy)]
pub struct Prefs {
    pub close_to_tray: bool,
}

impl Prefs {
    pub fn from(s: &Settings) -> Self {
        Self { close_to_tray: s.close_to_tray }
    }
}

/// What the shell knows about the overlay windows it manages. Keys are window labels.
#[derive(Default)]
pub struct OverlayBook {
    /// The last description received from the engine.
    pub specs: HashMap<String, OverlaySpec>,
    /// Content size in CSS pixels, reported by each overlay page.
    pub sizes: HashMap<String, (f64, f64)>,
    /// Windows that have been made visible at least once.
    pub shown: HashSet<String>,
    /// Moves before this instant were made by us, not by the user dragging.
    pub ignore_moves_until: HashMap<String, Instant>,
    /// When the user last moved each window (to save the position once the drag ends).
    pub pending_move: HashMap<String, Instant>,
}

pub struct Shell {
    pub engine: Engine,
    pub prefs: Mutex<Prefs>,
    pub overlays: Mutex<OverlayBook>,
    /// Registered shortcut id -> engine action.
    pub hotkeys: Mutex<HashMap<u32, String>>,
    /// The `(action, accelerator)` pairs most recently applied, so the same list is never registered twice.
    pub applied_hotkeys: Mutex<Vec<(String, String)>>,
    /// Registering shortcuts is a multi-step operation; only one may run at a time.
    pub hotkey_lock: Mutex<()>,
    pub tray: Mutex<Option<TrayIcon>>,
    pub last_tray: Mutex<Option<TrayState>>,
    pub demo: bool,
}

impl Shell {
    pub fn new(engine: Engine, demo: bool) -> Self {
        Self {
            engine,
            prefs: Mutex::new(Prefs { close_to_tray: true }),
            overlays: Mutex::new(OverlayBook::default()),
            hotkeys: Mutex::new(HashMap::new()),
            applied_hotkeys: Mutex::new(Vec::new()),
            hotkey_lock: Mutex::new(()),
            tray: Mutex::new(None),
            last_tray: Mutex::new(None),
            demo,
        }
    }
}
