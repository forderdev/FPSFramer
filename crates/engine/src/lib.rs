//! FPSFramer engine: the runtime behind both the desktop app and the test server.
//!
//! * a **sampler thread** reads sensors, routes frames, recognises games, records
//!   sessions, evaluates alerts and publishes [`Event`]s;
//! * a **saver thread** writes finished sessions to disk;
//! * [`Engine::call`] is the JSON RPC surface the UI uses.
//!
//! The engine owns *what* should happen (which overlays exist, which hotkeys,
//! which notifications). The native shell owns *how* (windows, tray, shortcuts).

mod diagnostics;
pub mod events;
mod format;
mod history;
pub mod logging;
mod overlay;
mod router;
mod rpc;
mod runtime;
mod screenshots;
pub mod types;

pub use events::{Event, EventBus};
pub use rpc::RpcError;

use crossbeam_channel::{unbounded, Receiver, Sender};
use fpsframer_core::session::{HardwareInfo, SessionData};
use fpsframer_core::storage::{Opened, Paths, SessionStore, Store};
use fpsframer_platform::demo::DemoWorld;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct EngineOptions {
    pub paths: Paths,
    /// Replace all hardware data with a clearly-labelled simulation.
    pub demo: bool,
    pub version: String,
}

pub(crate) struct Core {
    pub store: Store,
    pub rt: runtime::Runtime,
    pub notices: Vec<String>,
    pub first_run: bool,
}

pub(crate) struct SaveJob {
    pub data: SessionData,
    pub notify: bool,
}

pub(crate) struct Inner {
    pub core: Mutex<Core>,
    pub bus: EventBus,
    pub sessions: SessionStore,
    pub save_tx: Sender<SaveJob>,
    pub shutdown: AtomicBool,
    pub ui_visible: AtomicUsize,
    pub rebuild_providers: AtomicBool,
    pub retry_capture: AtomicBool,
    pub demo: Option<Arc<DemoWorld>>,
    pub opts: EngineOptions,
    pub hardware: Mutex<Option<HardwareInfo>>,
    pub threads: Mutex<Vec<JoinHandle<()>>>,
    pub save_rx: Mutex<Option<Receiver<SaveJob>>>,
}

#[derive(Clone)]
pub struct Engine {
    pub(crate) inner: Arc<Inner>,
}

impl Engine {
    /// Open (or create) the user's data and prepare the engine. Call [`start`](Self::start) to begin sampling.
    pub fn new(opts: EngineOptions) -> Engine {
        logging::init(opts.paths.logs_dir());
        let Opened { store, notices, first_run } = Store::open(opts.paths.clone());
        for n in &notices {
            log::warn!("{n}");
        }
        let sessions = SessionStore::new(opts.paths.sessions_dir());
        sessions.housekeeping(7, store.settings.recording.max_sessions);
        if store.settings.auto_backup {
            let recent = store.list_backups().iter().any(|b| b.name.contains("-auto-") && fpsframer_core::util::now_ms().saturating_sub(b.modified) < 7 * 86_400_000);
            if !recent && !first_run {
                if let Err(e) = store.write_backup("auto", 5) {
                    log::warn!("Automatic backup failed: {e}");
                }
            }
        }
        let demo = opts.demo.then(|| DemoWorld::new(std::env::var("FPSFRAMER_DEMO_GAME").is_ok_and(|v| v == "1")));
        let (save_tx, save_rx) = unbounded();
        let rt = runtime::Runtime::new();
        let inner = Inner {
            core: Mutex::new(Core { store, rt, notices, first_run }),
            bus: EventBus::default(),
            sessions,
            save_tx,
            shutdown: AtomicBool::new(false),
            ui_visible: AtomicUsize::new(0),
            rebuild_providers: AtomicBool::new(false),
            retry_capture: AtomicBool::new(false),
            demo,
            opts,
            // Cheap (registry, DXGI, display modes), and the first screen of the app wants it immediately.
            hardware: Mutex::new(Some(fpsframer_platform::hardware::collect(&[]))),
            threads: Mutex::new(Vec::new()),
            save_rx: Mutex::new(Some(save_rx)),
        };
        log::info!("FPSFramer engine {} starting{}", inner.opts.version, if inner.demo.is_some() { " (DEMO MODE)" } else { "" });
        Engine { inner: Arc::new(inner) }
    }

    /// Start the sampler and saver threads.
    pub fn start(&self) {
        let mut threads = self.inner.threads.lock();
        if !threads.is_empty() {
            return;
        }
        threads.push(runtime::spawn_sampler(self.inner.clone()));
        if let Some(rx) = self.inner.save_rx.lock().take() {
            threads.push(runtime::spawn_saver(self.inner.clone(), rx));
        }
    }

    /// Finish any recording, stop the threads and release capture resources.
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        let handles: Vec<_> = self.inner.threads.lock().drain(..).collect();
        for h in handles {
            let _ = h.join();
        }
    }

    pub fn subscribe(&self) -> Receiver<Arc<Event>> {
        self.inner.bus.subscribe()
    }

    /// Windows that need the live UI call this with `true` when shown and `false` when hidden,
    /// so sampling can slow down while nothing is looking and no game is running.
    pub fn set_ui_visible(&self, visible: bool) {
        if visible {
            self.inner.ui_visible.fetch_add(1, Ordering::SeqCst);
        } else {
            // Decrement without going below zero (a plain compare-and-swap loop works on every Rust version).
            let mut cur = self.inner.ui_visible.load(Ordering::SeqCst);
            while cur > 0 {
                match self.inner.ui_visible.compare_exchange_weak(cur, cur - 1, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => break,
                    Err(seen) => cur = seen,
                }
            }
        }
    }

    /// JSON RPC used by the UI. Errors carry plain-language messages.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        rpc::dispatch(&self.inner, method, params)
    }

    /// Hotkey / tray actions: `toggleOverlay`, `nextScene`, `prevScene`, `toggleEdit`,
    /// `toggleRecording`, `screenshot`, `resetStats`.
    pub fn action(&self, name: &str) {
        rpc::run_action(&self.inner, name);
    }

    pub fn demo_enabled(&self) -> bool {
        self.inner.demo.is_some()
    }
}

#[cfg(test)]
mod tests;
