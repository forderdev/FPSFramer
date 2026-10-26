//! The sampler thread and the live (non-persistent) state.

use crate::events::Event;
use crate::format::format_value;
use crate::history::{History, HISTORY_SECONDS};
use crate::overlay::{compute_specs, SpecInput};
use crate::router::{Decision, Router, SelectCtx};
use crate::types::*;
use crate::{Core, Inner, SaveJob};
use crossbeam_channel::Receiver;
use fpsframer_core::alerts::{AlertContext, AlertEngine, AlertEvent, AlertFormat};
use fpsframer_core::frames::{decimate_max, FrameTracker, TrackerConfig};
use fpsframer_core::model::{CaptureMode, Profile, ProfileKind, Settings, Severity};
use fpsframer_core::sensors::{ids, Reading, Readings, ReadingsExt, RunningStat, SensorDesc, Unavailable, UnavailableCode, Unit};
use fpsframer_core::session::{AlertLogEntry, HardwareInfo, RecorderConfig, SessionKind, SessionMeta, SessionRecorder};
use fpsframer_core::storage::Store;
use fpsframer_core::util::{gen_id, now_ms};
use fpsframer_platform::capture::{CaptureState, FrameEvent, FrameSource};
use fpsframer_platform::clock::now_ns;
use fpsframer_platform::demo::{DemoFrames, DemoProcs, DemoProvider};
use fpsframer_platform::proc::{self, ForegroundInfo, ProcInfo, ProcessSource};
use fpsframer_platform::provider::{Health, SensorProvider};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct ActiveGame {
    pub exe: String,
    pub name: String,
    pub pid: u32,
    /// Frames are being measured (as opposed to merely recognising a program that has a profile).
    pub tracked: bool,
    pub api: Option<String>,
}

pub struct LiveSession {
    pub rec: SessionRecorder,
    pub manual: bool,
}

pub struct Runtime {
    pub catalog: BTreeMap<String, SensorDesc>,
    pub catalog_dirty: bool,
    pub providers: Vec<ProviderStatus>,
    pub capture: CaptureInfo,
    pub capture_class: String,
    pub capture_events: u64,
    pub router: Router,
    pub tracker: FrameTracker,
    pub tracker_cfg: Option<TrackerConfig>,
    pub run_stats: HashMap<String, RunningStat>,
    pub history: History,
    pub history_last_s: u64,
    pub alerts: AlertEngine,
    pub alert_log: VecDeque<AlertLogItem>,
    pub session: Option<LiveSession>,
    pub edit_mode: bool,
    pub preview_until: Option<Instant>,
    pub seq: u64,
    pub game: Option<ActiveGame>,
    pub game_monitor: Option<String>,
    pub specs: Vec<OverlaySpec>,
    pub tray: Option<TrayState>,
    pub hotkeys_sig: String,
    pub last_snapshot: Option<Snapshot>,
    pub procs: Vec<ProcInfo>,
    pub fg: Option<ForegroundInfo>,
    pub exclusive_fullscreen: Option<bool>,
    pub unsupported_api: Option<(String, String)>,
    pub last_readings: Readings,
    pub elevated: bool,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            catalog: BTreeMap::new(),
            catalog_dirty: true,
            providers: Vec::new(),
            capture: CaptureInfo::searching(),
            capture_class: String::new(),
            capture_events: 0,
            router: Router::default(),
            tracker: FrameTracker::new(TrackerConfig::default()),
            tracker_cfg: None,
            run_stats: HashMap::new(),
            history: History::new(HISTORY_SECONDS),
            history_last_s: 0,
            alerts: AlertEngine::new(),
            alert_log: VecDeque::new(),
            session: None,
            edit_mode: false,
            preview_until: None,
            seq: 0,
            game: None,
            game_monitor: None,
            specs: Vec::new(),
            tray: None,
            hotkeys_sig: String::new(),
            last_snapshot: None,
            procs: Vec::new(),
            fg: None,
            exclusive_fullscreen: None,
            unsupported_api: None,
            last_readings: Readings::new(),
            elevated: fpsframer_platform::system::is_elevated(),
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── profile resolution ─────────────────────────────

/// The profile that applies to `exe` (pinned > exact match > "All games").
pub fn resolve_profile<'a>(store: &'a Store, exe: Option<&str>) -> &'a Profile {
    if let Some(p) = store.settings.pinned_profile.as_ref().and_then(|id| store.profiles.iter().find(|p| &p.id == id)) {
        return p;
    }
    if let Some(e) = exe {
        if let Some(p) = store.profiles.iter().find(|p| p.kind == ProfileKind::Game && p.matches_exe(e)) {
            return p;
        }
    }
    store.profiles.iter().find(|p| p.kind == ProfileKind::Default).or_else(|| store.profiles.first()).expect("default profile always exists")
}

fn pretty_name(exe: &str) -> String {
    exe.trim_end_matches(".exe").to_string()
}

// ───────────────────────────── Core helpers ─────────────────────────────

impl Core {
    pub fn current_exe(&self) -> Option<&str> {
        self.rt.game.as_ref().map(|g| g.exe.as_str())
    }

    pub fn current_profile(&self) -> &Profile {
        resolve_profile(&self.store, self.current_exe())
    }

    fn make_specs(&self) -> Vec<OverlaySpec> {
        let preview = self.rt.preview_until.is_some_and(|t| Instant::now() < t);
        let exists = |id: &str| self.store.layout(id).is_some();
        compute_specs(&SpecInput {
            settings: &self.store.settings,
            profile: self.current_profile(),
            game_active: self.rt.game.is_some(),
            edit_mode: self.rt.edit_mode,
            preview,
            game_monitor: self.rt.game_monitor.as_deref(),
            layout_exists: &exists,
        })
    }

    fn make_tray(&self) -> TrayState {
        let p = self.current_profile();
        TrayState {
            overlay_enabled: self.store.settings.overlay_enabled,
            recording: self.rt.session.is_some(),
            profile_id: p.id.clone(),
            profile_name: p.name.clone(),
            scenes: p.scenes.iter().map(|s| TraySceneItem { id: s.id.clone(), name: s.name.clone(), active: s.id == p.active_scene_id }).collect(),
            game: self.rt.game.as_ref().map(|g| g.name.clone()),
            alert_count: self.rt.alerts.active().len(),
        }
    }

    fn make_hotkeys(&self) -> Vec<HotkeyBinding> {
        self.store.settings.hotkeys.assigned().into_iter().map(|(action, accelerator)| HotkeyBinding { action: action.into(), accelerator }).collect()
    }

    /// Recompute everything the shell mirrors (overlay windows, tray menu, hotkeys) and publish changes.
    pub fn refresh_outputs(&mut self, bus: &crate::EventBus) {
        let specs = self.make_specs();
        if specs != self.rt.specs {
            self.rt.specs = specs.clone();
            bus.emit(Event::Overlays(specs));
        }
        let tray = self.make_tray();
        if self.rt.tray.as_ref() != Some(&tray) {
            self.rt.tray = Some(tray.clone());
            bus.emit(Event::Tray(tray));
        }
        let hk = self.make_hotkeys();
        let sig = serde_json::to_string(&hk).unwrap_or_default();
        if sig != self.rt.hotkeys_sig {
            self.rt.hotkeys_sig = sig;
            bus.emit(Event::Hotkeys(hk));
        }
    }

    pub fn reset_stats(&mut self) {
        self.rt.run_stats.clear();
        self.rt.tracker.reset();
    }

    fn tracker_config(&self) -> TrackerConfig {
        let s = &self.store.settings;
        TrackerConfig { fps_window_ms: s.fps_window_ms, stats_window_s: s.stats_window.seconds(), lows_method: s.lows_method }
    }

    /// Begin recording. `hardware` is described in the session file.
    pub fn start_session(&mut self, hardware: HardwareInfo, manual: bool) {
        if self.rt.session.is_some() {
            return;
        }
        let profile = self.current_profile();
        let (profile_id, profile_name, target) = (profile.id.clone(), profile.name.clone(), profile.target_fps);
        let g = self.rt.game.as_ref();
        let meta = SessionMeta {
            id: gen_id("ses"),
            kind: if manual { SessionKind::Manual } else { SessionKind::Game },
            started_at: now_ms(),
            game_name: g.map(|g| g.name.clone()),
            exe: g.map(|g| g.exe.clone()),
            api: g.and_then(|g| g.api.clone()),
            profile_id: Some(profile_id),
            profile_name: Some(profile_name),
            demo: false,
            hardware,
            ..Default::default()
        };
        let mut meta = meta;
        meta.demo = self.rt.last_snapshot.as_ref().is_some_and(|s| s.demo);
        let cfg = RecorderConfig {
            record_all_sensors: self.store.settings.recording.record_all_sensors,
            target_fps: target,
            lows_method: self.store.settings.lows_method,
        };
        self.rt.session = Some(LiveSession { rec: SessionRecorder::new(meta, now_ns(), cfg), manual });
        log::info!("Recording started ({})", if manual { "manual" } else { "automatic" });
    }

    /// Stop recording and hand the result to the saver. Returns whether it was kept.
    pub fn finish_session(&mut self, inner: &Inner) -> bool {
        let Some(live) = self.rt.session.take() else { return false };
        let data = live.rec.finish(now_ns(), now_ms());
        let min = self.store.settings.recording.min_duration_s as f64;
        let keep = if live.manual { data.meta.duration_s >= 3.0 } else { data.meta.duration_s >= min };
        if keep {
            let notify = self.store.settings.notifications.session_summary;
            let _ = inner.save_tx.send(SaveJob { data, notify });
        } else {
            log::info!("Discarded a {:.0} s recording (shorter than the {} s minimum)", data.meta.duration_s, min);
        }
        keep
    }
}

// ───────────────────────────── sampler ─────────────────────────────

struct Slot {
    p: Box<dyn SensorProvider>,
    last: Readings,
    last_poll: Option<Instant>,
    health: Health,
    retry_at: Option<Instant>,
}

struct Sampler {
    inner: Arc<Inner>,
    providers: Vec<Slot>,
    frames: Box<dyn FrameSource>,
    procs: Box<dyn ProcessSource>,
    frame_buf: Vec<FrameEvent>,
    last_proc_check: Option<Instant>,
    pid_exe: HashMap<u32, String>,
    gfx_cache: HashMap<u32, Vec<String>>,
    fg_no_frames_since: Option<(u32, Instant)>,
    last_provider_sig: String,
}

fn build_providers(inner: &Inner, settings: &Settings) -> Vec<Slot> {
    let list: Vec<Box<dyn SensorProvider>> = match &inner.demo {
        Some(w) => vec![Box::new(DemoProvider::new(w.clone()))],
        None => fpsframer_platform::default_providers(settings.companions.clone(), settings.primary_gpu),
    };
    list.into_iter().map(|p| Slot { p, last: Readings::new(), last_poll: None, health: Health::Ok, retry_at: None }).collect()
}

pub fn spawn_sampler(inner: Arc<Inner>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("fpsframer-sampler".into())
        .spawn(move || {
            let settings = inner.core.lock().store.settings.clone();
            let providers = build_providers(&inner, &settings);
            let (frames, procs): (Box<dyn FrameSource>, Box<dyn ProcessSource>) = match &inner.demo {
                Some(w) => (Box::new(DemoFrames::new(w.clone())), Box::new(DemoProcs::new(w.clone(), proc::system()))),
                None => (fpsframer_platform::capture::system_source(), proc::system()),
            };
            let mut s = Sampler {
                inner,
                providers,
                frames,
                procs,
                frame_buf: Vec::with_capacity(4096),
                last_proc_check: None,
                pid_exe: HashMap::new(),
                gfx_cache: HashMap::new(),
                fg_no_frames_since: None,
                last_provider_sig: String::new(),
            };
            s.run();
        })
        .expect("spawn sampler thread")
}

impl Sampler {
    fn run(&mut self) {
        let st = self.frames.start();
        log::info!("Frame capture ({}): {:?}", self.frames.name(), st);
        // Hardware description for session files, collected once off the hot path.
        if self.inner.hardware.lock().is_none() {
            let hw = fpsframer_platform::hardware::collect(&[]);
            *self.inner.hardware.lock() = Some(hw);
        }
        let mut next = Instant::now();
        while !self.inner.shutdown.load(Ordering::SeqCst) {
            let started = Instant::now();
            if self.inner.rebuild_providers.swap(false, Ordering::SeqCst) {
                let settings = self.inner.core.lock().store.settings.clone();
                self.providers = build_providers(&self.inner, &settings);
                self.inner.core.lock().rt.catalog_dirty = true;
            }
            if self.inner.retry_capture.swap(false, Ordering::SeqCst) {
                self.frames.stop();
                let st = self.frames.start();
                log::info!("Frame capture restarted: {:?}", st);
            }
            let (interval, active) = self.tick();
            let period = if active { Duration::from_millis(interval as u64) } else { Duration::from_millis(2000.max(interval as u64)) };
            next = (next + period).max(started + Duration::from_millis(50));
            // Sleep in short steps so shutdown and wake-up flags are honoured quickly.
            while Instant::now() < next && !self.inner.shutdown.load(Ordering::SeqCst) {
                if self.inner.rebuild_providers.load(Ordering::SeqCst) || self.inner.retry_capture.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if Instant::now() > next + Duration::from_secs(2) {
                next = Instant::now(); // resumed from sleep: don't try to catch up
            }
        }
        // Wind down.
        let mut core = self.inner.core.lock();
        core.finish_session(&self.inner);
        drop(core);
        self.frames.stop();
        log::info!("Sampler stopped");
    }

    fn poll_providers(&mut self) -> (Readings, bool) {
        let mut readings = Readings::new();
        let mut catalog_changed = false;
        for slot in &mut self.providers {
            let due = slot.last_poll.is_none_or(|t| t.elapsed().as_millis() as u64 >= slot.p.min_interval_ms())
                && slot.retry_at.is_none_or(|t| Instant::now() >= t);
            if due {
                let mut fresh = Readings::new();
                let res = catch_unwind(AssertUnwindSafe(|| slot.p.poll(&mut fresh)));
                slot.last_poll = Some(Instant::now());
                match res {
                    Ok(o) => {
                        catalog_changed |= o.catalog_changed;
                        if let Health::Failed(m) = &o.health {
                            if !matches!(slot.health, Health::Failed(_)) {
                                log::warn!("Sensor source '{}' failed: {m}. Retrying in 30 s.", slot.p.label());
                            }
                            slot.retry_at = Some(Instant::now() + Duration::from_secs(30));
                        } else {
                            slot.retry_at = None;
                        }
                        if o.health != slot.health {
                            catalog_changed = true;
                        }
                        slot.health = o.health;
                        slot.last = fresh;
                    }
                    Err(_) => {
                        log::error!("Sensor source '{}' crashed internally; it will be retried in 60 s.", slot.p.label());
                        slot.health = Health::Failed("An internal error occurred.".into());
                        slot.retry_at = Some(Instant::now() + Duration::from_secs(60));
                        slot.last.clear();
                        catalog_changed = true;
                    }
                }
            }
            for (k, v) in &slot.last {
                readings.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // Derived: busiest core.
        if !readings.contains_key(ids::CPU_CORE_MAX) {
            let m = readings
                .iter()
                .filter(|(k, _)| k.starts_with("cpu.core.") && k.ends_with(".load"))
                .filter_map(|(_, v)| v.as_f64())
                .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
            if let Some(m) = m {
                readings.put(ids::CPU_CORE_MAX, m, Unit::Percent);
            }
        }
        (readings, catalog_changed)
    }

    fn refresh_processes(&mut self) {
        let list_due = self.last_proc_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(2));
        let list = if list_due {
            self.last_proc_check = Some(Instant::now());
            let l = self.procs.list();
            self.pid_exe = l.iter().map(|p| (p.pid, p.exe.clone())).collect();
            let alive: HashSet<u32> = self.pid_exe.keys().cloned().collect();
            self.gfx_cache.retain(|pid, _| alive.contains(pid));
            Some(l)
        } else {
            None
        };
        // The foreground window changes quickly, so it is checked on every pass.
        let fg = self.procs.foreground();
        let excl = if list_due { self.procs.exclusive_fullscreen() } else { None };
        let mut core = self.inner.core.lock();
        if let Some(l) = list {
            core.rt.procs = l;
            core.rt.exclusive_fullscreen = excl;
        }
        core.rt.fg = fg;
    }

    fn modules_for(&mut self, pid: u32) -> Vec<String> {
        if let Some(m) = self.gfx_cache.get(&pid) {
            return m.clone();
        }
        let m = self.procs.graphics_modules(pid);
        self.gfx_cache.insert(pid, m.clone());
        m
    }

    /// One sampling pass. Returns `(configured interval ms, whether anything is watching)`.
    fn tick(&mut self) -> (u32, bool) {
        let t_ns = now_ns();
        let t_ms = now_ms();
        let (mut readings, mut catalog_changed) = self.poll_providers();
        self.refresh_processes();
        self.frame_buf.clear();
        self.frames.drain(&mut self.frame_buf);
        let capture_state = self.frames.state();
        let capture_events = self.frames.events_seen();

        // Provider status (diagnostics).
        let statuses: Vec<ProviderStatus> = self
            .providers
            .iter()
            .map(|s| {
                let (health, message) = match &s.health {
                    Health::Ok => ("ok", None),
                    Health::Degraded(m) => ("degraded", Some(m.clone())),
                    Health::Failed(m) => ("failed", Some(m.clone())),
                };
                ProviderStatus { id: s.p.id().into(), label: s.p.label().into(), health: health.into(), message, sensors: s.p.descriptors().len() }
            })
            .collect();
        let sig = serde_json::to_string(&statuses).unwrap_or_default();
        let providers_changed = sig != self.last_provider_sig;
        self.last_provider_sig = sig;

        // Everything below needs the shared state.
        let inner = self.inner.clone();
        let mut guard = inner.core.lock();
        let core = &mut *guard;
        let settings = core.store.settings.clone();

        // ── frame tracker configuration ──
        let want_cfg = core.tracker_config_pub();
        if core.rt.tracker_cfg.is_none_or(|c| c.fps_window_ms != want_cfg.fps_window_ms || c.stats_window_s != want_cfg.stats_window_s || c.lows_method != want_cfg.lows_method) {
            core.rt.tracker.set_config(want_cfg);
            core.rt.tracker_cfg = Some(want_cfg);
        }

        // ── frames → tracker / recorder ──
        let mut unknown_pid = false;
        // Trace events can reach us slightly out of order; frame times are differences of timestamps.
        self.frame_buf.sort_by_key(|e| e.t_ns);
        let mut batch_started = false;
        for ev in &self.frame_buf {
            if !self.pid_exe.contains_key(&ev.pid) {
                unknown_pid = true;
            }
            if let Some((t, ft)) = core.rt.router.ingest(ev) {
                if !batch_started {
                    core.rt.tracker.batch_arrived(t_ns);
                    batch_started = true;
                }
                core.rt.tracker.push(t, ft);
                if let Some(s) = core.rt.session.as_mut() {
                    s.rec.on_frame(t, ft);
                }
            }
        }
        if unknown_pid {
            self.last_proc_check = None; // look at the process list again next tick
        }
        core.rt.router.prune(t_ns);

        // ── which program is "the game"? ──
        let fg_pid = core.rt.fg.as_ref().map(|f| f.pid);
        let ignored: HashSet<String> = settings.capture.ignored.iter().cloned().collect();
        let manual = (settings.capture.mode == CaptureMode::Manual).then(|| settings.capture.manual_process.clone()).flatten();
        let known_exes: HashSet<String> = core.store.profiles.iter().filter(|p| p.kind == ProfileKind::Game).flat_map(|p| p.executables.iter().cloned()).collect();
        let pid_exe = self.pid_exe.clone();
        let exe_of = |pid: u32| pid_exe.get(&pid).cloned();
        let excluded = |e: &str| proc::is_never_a_game(e) || ignored.contains(&e.to_lowercase());
        let known = |e: &str| known_exes.contains(&e.to_lowercase());
        let decision = core.rt.router.select(&SelectCtx { now_ns: t_ns, foreground_pid: fg_pid, exe_of: &exe_of, excluded: &excluded, manual: manual.as_deref(), known: &known });
        match decision {
            Decision::Switch(key) => {
                let exe = self.pid_exe.get(&key.0).cloned().unwrap_or_default();
                let modules = self.modules_for(key.0);
                let api = core.rt.router.api_of(key).map(|a| proc::describe_api(a, &modules));
                self.on_game_start(core, key.0, exe, api, t_ns);
            }
            Decision::Drop => self.on_game_end(core),
            Decision::Keep => {}
        }
        // A program that has its own profile counts as "game running" even if we can't measure FPS yet.
        if core.rt.game.as_ref().is_none_or(|g| !g.tracked) {
            let known_running = pid_exe
                .iter()
                .filter(|(_, e)| known(e))
                .map(|(pid, e)| (*pid, e.clone()))
                .min_by_key(|(pid, _)| if Some(*pid) == fg_pid { 0 } else { 1 });
            match known_running {
                Some((pid, exe)) => {
                    if core.rt.game.as_ref().is_none_or(|g| g.exe != exe) {
                        let name = core.store.profiles.iter().find(|p| p.matches_exe(&exe)).map(|p| p.name.clone()).unwrap_or_else(|| pretty_name(&exe));
                        core.rt.game = Some(ActiveGame { exe, name, pid, tracked: false, api: None });
                    }
                }
                None => core.rt.game = None,
            }
        }
        // Monitor the game window is on.
        core.rt.game_monitor = match (&core.rt.fg, &core.rt.game) {
            (Some(f), Some(g)) if f.exe == g.exe => f.monitor.clone(),
            _ => core.rt.game_monitor.take().filter(|_| core.rt.game.is_some()),
        };

        // ── frame statistics → readings ──
        let tracked = core.rt.game.as_ref().is_some_and(|g| g.tracked);
        let mut last_frame_age_ms: Option<f64> = None;
        if tracked {
            let st = core.rt.tracker.stats(t_ns);
            if let Some(last) = core.rt.tracker.last_frame_ns() {
                last_frame_age_ms = Some(t_ns.saturating_sub(last) as f64 / 1e6);
            }
            let mut put = |id: &str, v: Option<f64>, u: Unit| {
                if let Some(v) = v {
                    readings.put(id, v, u);
                }
            };
            put(ids::FPS, st.fps, Unit::Fps);
            put(ids::FRAMETIME, st.frametime, Unit::Ms);
            put(ids::FRAMETIME_MAX, st.frametime_max, Unit::Ms);
            put(ids::FPS_AVG, st.fps_avg, Unit::Fps);
            put(ids::FPS_MIN, st.fps_min, Unit::Fps);
            put(ids::FPS_MAX, st.fps_max, Unit::Fps);
            put(ids::FPS_LOW1, st.low1, Unit::Fps);
            put(ids::FPS_LOW01, st.low01, Unit::Fps);
        }
        if let Some(g) = &core.rt.game {
            readings.put_text(ids::GAME_NAME, g.name.clone());
            if let Some(a) = &g.api {
                readings.put_text(ids::GAME_API, a.clone());
            }
        }
        if let Some(s) = &core.rt.session {
            readings.put(ids::SESSION_TIME, s.rec.elapsed_s(t_ns), Unit::Seconds);
        }

        // ── capture status for the UI ──
        let info = self.capture_info(core, &capture_state, last_frame_age_ms, t_ns);
        let class = info.state.clone();
        let class_changed = class != core.rt.capture_class;
        core.rt.capture_class = class;
        let status_changed = info != core.rt.capture || capture_events / 200 != core.rt.capture_events / 200 || providers_changed;
        core.rt.capture = info;
        core.rt.capture_events = capture_events;
        core.rt.providers = statuses.clone();

        // ── catalog ──
        if catalog_changed || class_changed || core.rt.catalog_dirty || core.rt.catalog.is_empty() {
            let merged = self.merge_catalog(core);
            if merged.values().cloned().collect::<Vec<_>>() != core.rt.catalog.values().cloned().collect::<Vec<_>>() {
                core.rt.catalog = merged;
                inner.bus.emit(Event::Catalog(core.rt.catalog.values().cloned().collect()));
            }
            core.rt.catalog_dirty = false;
            catalog_changed = false;
        }
        let _ = catalog_changed;

        // ── running stats + history ──
        for (id, r) in &readings {
            if let Reading::Num(v) = r {
                if id.starts_with("cpu.core.") || id.starts_with("ext.") {
                    continue;
                }
                core.rt.run_stats.entry(id.clone()).or_default().push(*v);
            }
        }
        if t_ms / 1000 != core.rt.history_last_s {
            core.rt.history_last_s = t_ms / 1000;
            core.rt.history.push_second(t_ms, &readings);
        }

        // ── alerts ──
        let alert_events = {
            let catalog = &core.rt.catalog;
            let name_of = |id: &str| catalog.get(id).map(|d| d.name.clone()).unwrap_or_else(|| id.to_string());
            let fmt_val = |id: &str, v: f64| format_value(catalog.get(id).map_or(Unit::Count, |d| d.unit), v, &settings);
            let profile_id = resolve_profile(&core.store, core.rt.game.as_ref().map(|g| g.exe.as_str())).id.clone();
            let game_active = core.rt.game.is_some();
            core.rt.alerts.evaluate(
                t_ms,
                &core.store.rules,
                &readings,
                &AlertContext { game_active, profile_id: Some(&profile_id) },
                &AlertFormat { sensor_name: &name_of, value: &fmt_val },
            )
        };
        for ev in alert_events {
            self.handle_alert(core, ev, t_ns, t_ms, &settings);
        }

        // ── recorder ──
        if let Some(s) = core.rt.session.as_mut() {
            let catalog = &core.rt.catalog;
            s.rec.on_sample(t_ns, &readings, &|id| catalog.get(id).is_some_and(|d| d.recordable));
        }

        // ── publish ──
        core.rt.seq += 1;
        let frames = decimate_max(&core.rt.tracker.take_pending(), 300);
        let profile = core.current_profile();
        let scene = profile.active_scene();
        let profile_live = ProfileLive {
            id: profile.id.clone(),
            name: profile.name.clone(),
            scene_id: scene.map(|s| s.id.clone()).unwrap_or_default(),
            scene_name: scene.map(|s| s.name.clone()).unwrap_or_default(),
            target_fps: profile.target_fps,
        };
        let stats: BTreeMap<String, [f64; 3]> = core.rt.run_stats.iter().filter_map(|(k, s)| Some((k.clone(), [round2(s.min), round2(s.max), round2(s.avg()?)]))).collect();
        let snap = Snapshot {
            seq: core.rt.seq,
            t: t_ms,
            demo: inner.demo.is_some(),
            values: readings.clone(),
            stats,
            frames,
            capture: core.rt.capture.clone(),
            alerts: core.rt.alerts.active(),
            session: core.rt.session.as_ref().map(|s| SessionLive { id: s.rec.meta().id.clone(), elapsed_s: s.rec.elapsed_s(t_ns), manual: s.manual }),
            profile: profile_live,
            overlay_enabled: settings.overlay_enabled,
            edit_mode: core.rt.edit_mode,
            game_active: core.rt.game.is_some(),
            exclusive_fullscreen: core.rt.exclusive_fullscreen == Some(true),
        };
        core.rt.last_readings = readings;
        core.rt.last_snapshot = Some(snap.clone());
        inner.bus.emit(Event::Snapshot(snap));
        if status_changed {
            inner.bus.emit(Event::Status(StatusInfo { capture: core.rt.capture.clone(), providers: statuses, elevated: core.rt.elevated, capture_events }));
        }
        if core.rt.preview_until.is_some_and(|t| Instant::now() >= t) {
            core.rt.preview_until = None;
        }
        core.refresh_outputs(&inner.bus);

        let watching = inner.ui_visible.load(Ordering::SeqCst) > 0 || !core.rt.specs.is_empty() || core.rt.game.is_some() || core.rt.session.is_some() || core.rt.edit_mode;
        (settings.update_interval_ms, watching)
    }

    fn on_game_start(&mut self, core: &mut Core, pid: u32, exe: String, api: Option<String>, t_ns: u64) {
        let profile_name = resolve_profile(&core.store, Some(&exe));
        let name = if profile_name.kind == ProfileKind::Game && profile_name.matches_exe(&exe) { profile_name.name.clone() } else { pretty_name(&exe) };
        log::info!("Measuring {exe} (pid {pid}) via {}", api.as_deref().unwrap_or("unknown API"));
        core.rt.run_stats.clear();
        core.rt.tracker.reset();
        core.rt.alerts.reset();
        core.rt.game = Some(ActiveGame { exe: exe.clone(), name: name.clone(), pid, tracked: true, api: api.clone() });
        self.unsupported_clear(core);
        let hw = self.inner.hardware.lock().clone().unwrap_or_default();
        if let Some(s) = core.rt.session.as_mut() {
            let m = s.rec.meta_mut();
            m.game_name = Some(name.clone());
            m.exe = Some(exe.clone());
            m.api = api;
        } else if core.store.settings.recording.auto_record {
            core.start_session(hw, false);
        }
        let _ = t_ns;
        self.inner.bus.emit(Event::Toast(UiToast { level: "info".into(), message: format!("Now measuring {name}"), action: None }));
        core.refresh_outputs(&self.inner.bus);
    }

    fn on_game_end(&mut self, core: &mut Core) {
        if let Some(g) = &core.rt.game {
            log::info!("{} stopped presenting frames", g.exe);
        }
        let auto = core.rt.session.as_ref().is_some_and(|s| !s.manual);
        if auto {
            core.finish_session(&self.inner);
        }
        core.rt.game = None;
        core.rt.tracker.reset();
        core.rt.router.clear_tracked();
        core.rt.alerts.reset();
        core.refresh_outputs(&self.inner.bus);
    }

    fn unsupported_clear(&mut self, core: &mut Core) {
        core.rt.unsupported_api = None;
        self.fg_no_frames_since = None;
    }

    fn capture_info(&mut self, core: &mut Core, state: &CaptureState, last_frame_age_ms: Option<f64>, _t_ns: u64) -> CaptureInfo {
        let (pid, exe, name, api, tracked) = match core.rt.game.as_ref() {
            Some(g) => (Some(g.pid), Some(g.exe.clone()), Some(g.name.clone()), g.api.clone(), g.tracked),
            None => (None, None, None, None, false),
        };
        let base = move |s: &str, msg: Option<String>| CaptureInfo { state: s.into(), message: msg, pid, exe: exe.clone(), name: name.clone(), api: api.clone() };
        match state {
            CaptureState::NeedsPermission => base(
                "needsPermission",
                Some("Windows needs to give FPSFramer permission before it can measure FPS. This is a one-time step.".into()),
            ),
            CaptureState::Unsupported(m) => base("unsupported", Some(m.clone())),
            CaptureState::Error(m) => base("error", Some(m.clone())),
            CaptureState::Stopped => base("stopped", None),
            CaptureState::Running => {
                if tracked {
                    if last_frame_age_ms.is_some_and(|a| a > 1500.0) {
                        return base("stalled", Some("The game has stopped drawing frames (paused, minimised or frozen).".into()));
                    }
                    return base("tracking", None);
                }
                // Not tracking: is the foreground program a Vulkan/OpenGL game we can't measure?
                match self.detect_unsupported_api(core) {
                    Some((exe, api)) => {
                        let mut i = base("unsupportedApi", Some(format!("{exe} uses {api}. FPSFramer can't measure FPS for {api} games yet, but hardware readings still work.")));
                        i.name = Some(pretty_name(&exe));
                        i.exe = Some(exe);
                        i.api = Some(api);
                        i
                    }
                    None => base("searching", None),
                }
            }
        }
    }

    fn detect_unsupported_api(&mut self, core: &mut Core) -> Option<(String, String)> {
        let fg = core.rt.fg.clone()?;
        if !fg.fullscreen || proc::is_never_a_game(&fg.exe) {
            self.fg_no_frames_since = None;
            core.rt.unsupported_api = None;
            return None;
        }
        let since = match self.fg_no_frames_since {
            Some((pid, t)) if pid == fg.pid => t,
            _ => {
                self.fg_no_frames_since = Some((fg.pid, Instant::now()));
                core.rt.unsupported_api = None;
                return None;
            }
        };
        if since.elapsed() < Duration::from_secs(6) {
            return None;
        }
        let modules = self.modules_for(fg.pid);
        let api = proc::unsupported_api(&modules)?;
        core.rt.unsupported_api = Some((fg.exe.clone(), api.to_string()));
        Some((fg.exe, api.to_string()))
    }

    fn merge_catalog(&self, core: &Core) -> BTreeMap<String, SensorDesc> {
        let mut map: BTreeMap<String, SensorDesc> = BTreeMap::new();
        for slot in &self.providers {
            for d in slot.p.descriptors() {
                match map.get(&d.id) {
                    None => {
                        map.insert(d.id.clone(), d);
                    }
                    Some(existing) if !existing.availability.available && d.availability.available => {
                        map.insert(d.id.clone(), d);
                    }
                    _ => {}
                }
            }
        }
        let unavail = fps_unavailable(&core.rt.capture);
        for d in fpsframer_platform::catalog::fps_family(unavail.as_ref()).into_iter().chain(fpsframer_platform::catalog::game_family(unavail.as_ref())) {
            map.insert(d.id.clone(), d);
        }
        // Session time is available only while recording.
        if let Some(d) = map.get_mut(ids::SESSION_TIME) {
            if core.rt.session.is_none() {
                d.availability = fpsframer_core::sensors::Availability::no(UnavailableCode::NoActiveGame, "Shown while a recording is running.", Some("Recording starts automatically with a game, or press the record hotkey."));
            } else {
                d.availability = fpsframer_core::sensors::Availability::yes();
            }
        }
        for (id, o) in &core.store.settings.sensor_overrides {
            if let Some(d) = map.get_mut(id) {
                if let Some(r) = d.status_rule.as_mut() {
                    if let Some(e) = o.elevated {
                        r.elevated = e;
                    }
                    if let Some(c) = o.critical {
                        r.critical = c;
                    }
                }
            }
        }
        map
    }

    fn handle_alert(&mut self, core: &mut Core, ev: AlertEvent, t_ns: u64, t_ms: u64, settings: &Settings) {
        match ev {
            AlertEvent::Fired { alert, toast, sound, suppressed } => {
                let item = AlertLogItem {
                    t: t_ms,
                    rule_id: alert.rule_id.clone(),
                    name: alert.name.clone(),
                    severity: alert.severity,
                    message: alert.message.clone(),
                    kind: if suppressed { "suppressed".into() } else { "fired".into() },
                };
                push_log(&mut core.rt.alert_log, item.clone());
                self.inner.bus.emit(Event::AlertLog(item));
                if let Some(s) = core.rt.session.as_mut() {
                    s.rec.on_alert(
                        t_ns,
                        AlertLogEntry { t_s: 0.0, rule_id: alert.rule_id.clone(), name: alert.name.clone(), severity: alert.severity, message: alert.message.clone(), kind: "fired".into() },
                    );
                }
                if toast && settings.notifications.toast {
                    self.inner.bus.emit(Event::Notify(Notification { title: alert.name.clone(), body: alert.message.clone(), level: alert.severity, sound: sound && settings.notifications.sound }));
                } else if sound && settings.notifications.sound {
                    self.inner.bus.emit(Event::Notify(Notification { title: String::new(), body: String::new(), level: Severity::Info, sound: true }));
                }
            }
            AlertEvent::Cleared { rule_id, name, duration_ms, peak: _ } => {
                let item = AlertLogItem {
                    t: t_ms,
                    rule_id: rule_id.clone(),
                    name: name.clone(),
                    severity: Severity::Info,
                    message: format!("Back to normal after {} s.", duration_ms / 1000),
                    kind: "cleared".into(),
                };
                push_log(&mut core.rt.alert_log, item.clone());
                self.inner.bus.emit(Event::AlertLog(item));
                if let Some(s) = core.rt.session.as_mut() {
                    s.rec.on_alert(t_ns, AlertLogEntry { t_s: 0.0, rule_id, name, severity: Severity::Info, message: "cleared".into(), kind: "cleared".into() });
                }
            }
        }
    }
}

fn push_log(log: &mut VecDeque<AlertLogItem>, item: AlertLogItem) {
    if log.len() >= 200 {
        log.pop_front();
    }
    log.push_back(item);
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Why FPS sensors cannot currently provide values (None = they can).
pub fn fps_unavailable(c: &CaptureInfo) -> Option<Unavailable> {
    match c.state.as_str() {
        "tracking" | "stalled" => None,
        "needsPermission" => Some(Unavailable {
            code: UnavailableCode::NeedsPermission,
            message: "FPS measurement needs a one-time permission from Windows.".into(),
            fix: Some("Open Help → FPS permission and choose how to grant it.".into()),
        }),
        "unsupported" => Some(Unavailable { code: UnavailableCode::NotSupported, message: c.message.clone().unwrap_or_else(|| "FPS measurement is not available on this system.".into()), fix: None }),
        "error" => Some(Unavailable { code: UnavailableCode::ProviderError, message: c.message.clone().unwrap_or_else(|| "FPS measurement failed to start.".into()), fix: Some("Open Help → Diagnostics for details.".into()) }),
        "unsupportedApi" => Some(Unavailable { code: UnavailableCode::NotSupported, message: c.message.clone().unwrap_or_default(), fix: None }),
        _ => Some(fpsframer_platform::catalog::no_active_game()),
    }
}

// ───────────────────────────── saver ─────────────────────────────

pub fn spawn_saver(inner: Arc<Inner>, rx: Receiver<SaveJob>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("fpsframer-saver".into())
        .spawn(move || loop {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(job) => save_one(&inner, job),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if inner.shutdown.load(Ordering::SeqCst) && rx.is_empty() {
                        break;
                    }
                }
                Err(_) => break,
            }
        })
        .expect("spawn saver thread")
}

fn save_one(inner: &Inner, job: SaveJob) {
    let d = &job.data;
    let name = d.meta.game_name.clone().unwrap_or_else(|| "Desktop".into());
    match inner.sessions.save(d) {
        Ok(()) => {
            let max = inner.core.lock().store.settings.recording.max_sessions;
            inner.sessions.housekeeping(7, max);
            inner.bus.emit(Event::Sessions);
            inner.bus.emit(Event::SessionFinished { id: d.meta.id.clone(), name: name.clone() });
            let s = &d.summary;
            let dur = fpsframer_core::analysis::fmt_clock(d.meta.duration_s);
            let body = match (s.fps_avg, s.low1) {
                (Some(a), Some(l)) => format!("{name} · {a:.0} FPS average · 1% low {l:.0} · {dur}"),
                (Some(a), None) => format!("{name} · {a:.0} FPS average · {dur}"),
                _ => format!("{name} · {dur}"),
            };
            inner.bus.emit(Event::Toast(UiToast {
                level: "success".into(),
                message: format!("Session saved: {body}"),
                action: Some(ToastAction { label: "View".into(), route: format!("/sessions/{}", d.meta.id) }),
            }));
            if job.notify {
                inner.bus.emit(Event::Notify(Notification { title: "Session saved".into(), body, level: Severity::Info, sound: false }));
            }
        }
        Err(e) => {
            log::error!("Could not save session {}: {e}", d.meta.id);
            inner.bus.emit(Event::Toast(UiToast { level: "error".into(), message: format!("The session recording could not be saved: {e}"), action: None }));
        }
    }
}

impl Core {
    pub fn tracker_config_pub(&self) -> TrackerConfig {
        self.tracker_config()
    }
}
