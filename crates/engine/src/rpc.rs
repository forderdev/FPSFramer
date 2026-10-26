//! The JSON RPC surface used by the UI, plus hotkey/tray actions.

use crate::diagnostics;
use crate::events::Event;
use crate::logging;
use crate::runtime::resolve_profile;
use crate::screenshots::Shots;
use crate::types::*;
use crate::{Core, Inner};
use fpsframer_core::defaults;
use fpsframer_core::layouts::{presets, Layout};
use fpsframer_core::model::*;
use fpsframer_core::session::to_csv;
use fpsframer_core::storage::{valid_id, BackupBundle};
use fpsframer_core::util::{gen_id, now_ms, stamp};
use fpsframer_platform::system;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub code: String,
    /// Plain-language message safe to show to the user.
    pub message: String,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new("badRequest", message)
    }
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new("failed", message)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new("notFound", format!("{what} was not found. It may have been deleted."))
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<serde_json::Error> for RpcError {
    fn from(e: serde_json::Error) -> Self {
        RpcError::bad(format!("The request was not understood: {e}"))
    }
}

type R = Result<Value, RpcError>;

fn ok<T: Serialize>(v: T) -> R {
    Ok(serde_json::to_value(v)?)
}

fn arg<T: DeserializeOwned>(p: &Value, key: &str) -> Result<T, RpcError> {
    let v = p.get(key).cloned().ok_or_else(|| RpcError::bad(format!("Missing '{key}'.")))?;
    Ok(serde_json::from_value(v)?)
}

fn arg_opt<T: DeserializeOwned>(p: &Value, key: &str) -> Result<Option<T>, RpcError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(Some(serde_json::from_value(v.clone())?)),
    }
}

fn toast(inner: &Inner, level: &str, message: impl Into<String>) {
    inner.bus.emit(Event::Toast(UiToast { level: level.into(), message: message.into(), action: None }));
}

/// RFC 7386-style merge: objects merge recursively, everything else (including arrays and null) replaces.
pub fn merge(dst: &mut Value, patch: &Value) {
    match (dst, patch) {
        (Value::Object(d), Value::Object(p)) => {
            for (k, v) in p {
                merge(d.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (d, p) => *d = p.clone(),
    }
}

/// A shortcut must contain a modifier (so it can't steal plain keys from a game) and a real key.
pub fn accelerator_ok(a: &str) -> bool {
    let parts: Vec<&str> = a.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    let is_mod = |p: &str| matches!(p.to_lowercase().as_str(), "ctrl" | "control" | "alt" | "shift" | "super" | "cmd" | "command" | "commandorcontrol" | "cmdorctrl" | "meta");
    parts[..parts.len() - 1].iter().all(|p| is_mod(p)) && !is_mod(parts[parts.len() - 1])
}

fn emit_all_data(inner: &Inner, core: &mut Core) {
    inner.bus.emit(Event::Settings(core.store.settings.clone()));
    inner.bus.emit(Event::Layouts(core.store.all_layouts()));
    inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
    inner.bus.emit(Event::Rules(core.store.rules.clone()));
    core.refresh_outputs(&inner.bus);
}

// ───────────────────────────── dispatch ─────────────────────────────

pub fn dispatch(inner: &Inner, method: &str, p: Value) -> R {
    match method {
        // ── app ──
        "app.getState" => app_state(inner),
        "app.completeSetup" => {
            let mut core = inner.core.lock();
            if let Some(patch) = p.get("settings") {
                let mut v = serde_json::to_value(&core.store.settings)?;
                merge(&mut v, patch);
                let s: Settings = serde_json::from_value(v)?;
                core.store.settings = s;
            }
            core.store.settings.first_run_completed = true;
            core.store.settings.sanitize();
            core.store.save_settings().map_err(|e| RpcError::failed(format!("Could not save settings: {e}")))?;
            core.first_run = false;
            inner.bus.emit(Event::Settings(core.store.settings.clone()));
            core.refresh_outputs(&inner.bus);
            ok(&core.store.settings)
        }
        // ── settings ──
        "settings.update" => settings_update(inner, &p),
        "settings.reset" => {
            let mut core = inner.core.lock();
            let _ = core.store.write_backup("before-reset", 5);
            let mut s = defaults::default_settings();
            s.first_run_completed = true;
            core.store.settings = s;
            core.store.save_settings().map_err(|e| RpcError::failed(e.to_string()))?;
            inner.rebuild_providers.store(true, Ordering::SeqCst);
            core.rt.catalog_dirty = true;
            emit_all_data(inner, &mut core);
            ok(&core.store.settings)
        }
        // ── layouts ──
        "layouts.list" => ok(inner.core.lock().store.all_layouts()),
        "layouts.save" => layouts_save(inner, &p),
        "layouts.delete" => layouts_delete(inner, &p),
        "layouts.duplicate" => {
            let id: String = arg(&p, "id")?;
            let name: Option<String> = arg_opt(&p, "name")?;
            let mut core = inner.core.lock();
            let src = core.store.layout(&id).ok_or_else(|| RpcError::not_found("The layout"))?;
            let mut l = src.duplicate(name);
            l.sanitize().map_err(RpcError::failed)?;
            core.store.layouts.push(l.clone());
            core.store.save_layout(&l.id).map_err(|e| RpcError::failed(e.to_string()))?;
            inner.bus.emit(Event::Layouts(core.store.all_layouts()));
            ok(l)
        }
        "layouts.export" => {
            let id: String = arg(&p, "id")?;
            let core = inner.core.lock();
            let l = core.store.layout(&id).ok_or_else(|| RpcError::not_found("The layout"))?;
            let json = serde_json::to_string_pretty(&json!({ "format": "fpsframer-layout", "version": 1, "layout": l }))?;
            ok(json!({ "filename": format!("{}.fpsframer-layout.json", fpsframer_core::util::slug(&l.name)), "json": json }))
        }
        "layouts.import" => {
            let text: String = arg(&p, "json")?;
            let v: Value = serde_json::from_str(&text).map_err(|_| RpcError::bad("That file is not valid JSON, so it can't be a layout."))?;
            if v.get("format").and_then(Value::as_str) != Some("fpsframer-layout") {
                return Err(RpcError::bad("This file is not an FPSFramer layout."));
            }
            let mut l: Layout = serde_json::from_value(v.get("layout").cloned().unwrap_or(Value::Null)).map_err(|_| RpcError::bad("The layout inside this file is damaged."))?;
            l.id = gen_id("lay");
            l.builtin = false;
            l.tags.retain(|t| t != "preset");
            l.created_at = now_ms();
            l.updated_at = l.created_at;
            l.sanitize().map_err(RpcError::bad)?;
            let mut core = inner.core.lock();
            core.store.layouts.push(l.clone());
            core.store.save_layout(&l.id).map_err(|e| RpcError::failed(e.to_string()))?;
            inner.bus.emit(Event::Layouts(core.store.all_layouts()));
            ok(l)
        }
        // ── profiles ──
        "profiles.list" => ok(inner.core.lock().store.profiles.clone()),
        "profiles.save" => profiles_save(inner, &p),
        "profiles.delete" => {
            let id: String = arg(&p, "id")?;
            if id == defaults::DEFAULT_PROFILE_ID {
                return Err(RpcError::bad("The \"All games\" profile can't be deleted. It is used for every game without its own profile."));
            }
            let mut core = inner.core.lock();
            let before = core.store.profiles.len();
            core.store.profiles.retain(|x| x.id != id);
            if core.store.profiles.len() == before {
                return Err(RpcError::not_found("The profile"));
            }
            core.store.remove_profile_file(&id);
            if core.store.settings.pinned_profile.as_deref() == Some(&id) {
                core.store.settings.pinned_profile = None;
                let _ = core.store.save_settings();
                inner.bus.emit(Event::Settings(core.store.settings.clone()));
            }
            inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
            core.refresh_outputs(&inner.bus);
            Ok(Value::Null)
        }
        "profiles.duplicate" => {
            let id: String = arg(&p, "id")?;
            let mut core = inner.core.lock();
            let src = core.store.profiles.iter().find(|x| x.id == id).cloned().ok_or_else(|| RpcError::not_found("The profile"))?;
            let copy = clone_profile(&src, Some(format!("{} (copy)", src.name)));
            core.store.profiles.push(copy.clone());
            core.store.save_profile(&copy.id).map_err(|e| RpcError::failed(e.to_string()))?;
            inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
            ok(copy)
        }
        "profiles.setScene" => {
            let (pid, sid): (String, String) = (arg(&p, "profileId")?, arg(&p, "sceneId")?);
            let mut core = inner.core.lock();
            set_scene(inner, &mut core, &pid, &sid)?;
            Ok(Value::Null)
        }
        "profiles.export" => {
            let id: String = arg(&p, "id")?;
            let core = inner.core.lock();
            let prof = core.store.profiles.iter().find(|x| x.id == id).ok_or_else(|| RpcError::not_found("The profile"))?;
            let mut layouts = Vec::new();
            for sc in &prof.scenes {
                for o in &sc.overlays {
                    if let Some(l) = core.store.layouts.iter().find(|l| l.id == o.layout_id) {
                        if !layouts.iter().any(|x: &Layout| x.id == l.id) {
                            layouts.push(l.clone());
                        }
                    }
                }
            }
            let json = serde_json::to_string_pretty(&json!({ "format": "fpsframer-profile", "version": 1, "profile": prof, "layouts": layouts }))?;
            ok(json!({ "filename": format!("{}.fpsframer-profile.json", fpsframer_core::util::slug(&prof.name)), "json": json }))
        }
        "profiles.import" => profiles_import(inner, &p),
        // ── overlay ──
        "overlay.setEdit" => {
            let on: bool = arg(&p, "on")?;
            let mut core = inner.core.lock();
            core.rt.edit_mode = on;
            core.refresh_outputs(&inner.bus);
            Ok(Value::Null)
        }
        "overlay.preview" => {
            let secs: u64 = arg_opt(&p, "seconds")?.unwrap_or(10);
            let mut core = inner.core.lock();
            core.rt.preview_until = Some(Instant::now() + Duration::from_secs(secs.clamp(2, 120)));
            core.refresh_outputs(&inner.bus);
            Ok(Value::Null)
        }
        "overlay.setPosition" => overlay_set_position(inner, &p),
        "overlay.setLayout" => overlay_set_layout(inner, &p),
        // ── alerts ──
        "rules.list" => ok(inner.core.lock().store.rules.clone()),
        "rules.templates" => ok(defaults::rule_templates()),
        "rules.save" => {
            let mut r: AlertRule = arg(&p, "rule")?;
            if !valid_id(&r.id) {
                r.id = gen_id("rule");
            }
            if r.sensor_id.trim().is_empty() {
                return Err(RpcError::bad("Choose which measurement the alert should watch."));
            }
            if r.name.trim().is_empty() {
                r.name = "Custom alert".into();
            }
            r.sanitize();
            let mut core = inner.core.lock();
            match core.store.rules.iter().position(|x| x.id == r.id) {
                Some(i) => core.store.rules[i] = r.clone(),
                None => core.store.rules.push(r.clone()),
            }
            core.store.save_rules().map_err(|e| RpcError::failed(e.to_string()))?;
            inner.bus.emit(Event::Rules(core.store.rules.clone()));
            ok(r)
        }
        "rules.delete" => {
            let id: String = arg(&p, "id")?;
            let mut core = inner.core.lock();
            core.store.rules.retain(|r| r.id != id);
            core.store.save_rules().map_err(|e| RpcError::failed(e.to_string()))?;
            inner.bus.emit(Event::Rules(core.store.rules.clone()));
            Ok(Value::Null)
        }
        "alerts.log" => ok(inner.core.lock().rt.alert_log.iter().rev().cloned().collect::<Vec<_>>()),
        // ── live data ──
        "live.snapshot" => ok(inner.core.lock().rt.last_snapshot.clone()),
        "history.get" => {
            let ids: Vec<String> = arg(&p, "ids")?;
            let secs: usize = arg_opt(&p, "seconds")?.unwrap_or(300);
            ok(inner.core.lock().rt.history.get(&ids, secs.min(crate::history::HISTORY_SECONDS)))
        }
        "stats.reset" => {
            inner.core.lock().reset_stats();
            Ok(Value::Null)
        }
        // ── sessions ──
        "sessions.list" => ok(inner.sessions.list()),
        "sessions.get" => {
            let id: String = arg(&p, "id")?;
            ok(inner.sessions.load(&id).map_err(|_| RpcError::not_found("The session"))?)
        }
        "sessions.delete" => {
            let id: String = arg(&p, "id")?;
            inner.sessions.delete(&id).map_err(|_| RpcError::not_found("The session"))?;
            inner.bus.emit(Event::Sessions);
            Ok(Value::Null)
        }
        "sessions.restore" => {
            let id: String = arg(&p, "id")?;
            inner.sessions.restore(&id).map_err(|_| RpcError::failed("The session could not be restored (it may have been permanently removed)."))?;
            inner.bus.emit(Event::Sessions);
            Ok(Value::Null)
        }
        "sessions.update" => {
            let id: String = arg(&p, "id")?;
            let name: Option<String> = arg_opt(&p, "name")?;
            let notes: Option<String> = arg_opt(&p, "notes")?;
            let starred: Option<bool> = arg_opt(&p, "starred")?;
            let meta = inner
                .sessions
                .update_meta(&id, |m| {
                    if let Some(n) = name {
                        m.name = n.chars().take(120).collect();
                    }
                    if let Some(n) = notes {
                        m.notes = n.chars().take(4000).collect();
                    }
                    if let Some(s) = starred {
                        m.starred = s;
                    }
                })
                .map_err(|_| RpcError::not_found("The session"))?;
            inner.bus.emit(Event::Sessions);
            ok(meta)
        }
        "sessions.export" => {
            let id: String = arg(&p, "id")?;
            let format: String = arg_opt(&p, "format")?.unwrap_or_else(|| "csv".into());
            let d = inner.sessions.load(&id).map_err(|_| RpcError::not_found("The session"))?;
            let base = format!("{}-{}", fpsframer_core::util::slug(d.meta.game_name.as_deref().unwrap_or("session")), stamp(d.meta.started_at));
            match format.as_str() {
                "csv" => ok(json!({ "filename": format!("{base}.csv"), "mime": "text/csv", "text": to_csv(&d) })),
                "json" => ok(json!({ "filename": format!("{base}.json"), "mime": "application/json", "text": serde_json::to_string_pretty(&d)? })),
                _ => Err(RpcError::bad("Unknown export format.")),
            }
        }
        "sessions.startRecording" => {
            let mut core = inner.core.lock();
            if core.rt.session.is_some() {
                return Err(RpcError::bad("A recording is already running."));
            }
            let hw = inner.hardware.lock().clone().unwrap_or_default();
            core.start_session(hw, true);
            core.refresh_outputs(&inner.bus);
            core.rt.catalog_dirty = true;
            Ok(Value::Null)
        }
        "sessions.stopRecording" => {
            let mut core = inner.core.lock();
            let kept = core.finish_session(inner);
            core.refresh_outputs(&inner.bus);
            core.rt.catalog_dirty = true;
            ok(json!({ "saved": kept }))
        }
        // ── diagnostics ──
        "diagnostics.run" => ok(diagnostics::run(inner)),
        "diagnostics.report" => ok(diagnostics::report(inner)),
        "logs.recent" => ok(logging::recent(arg_opt::<usize>(&p, "n")?.unwrap_or(200).min(600))),
        "capture.retry" => {
            inner.retry_capture.store(true, Ordering::SeqCst);
            Ok(Value::Null)
        }
        "capture.grantPermission" => match system::grant_capture_permission() {
            Ok(()) => ok(json!({ "granted": true, "note": "Done. Sign out of Windows and back in (or restart) for the change to take effect." })),
            Err(e) => Err(RpcError::failed(e)),
        },
        "capture.restartElevated" => match system::relaunch_elevated() {
            Ok(()) => {
                inner.bus.emit(Event::Quit);
                Ok(Value::Null)
            }
            Err(e) => Err(RpcError::failed(e)),
        },
        // ── screenshots ──
        "screenshots.take" => take_screenshot(inner).and_then(ok),
        "screenshots.list" => {
            let core = inner.core.lock();
            let shots = Shots::new(core.store.settings.screenshots.folder.as_deref(), &core.store.paths);
            ok(json!({ "folder": shots.root().to_string_lossy(), "items": shots.list(300) }))
        }
        "screenshots.image" => {
            let id: String = arg(&p, "id")?;
            let thumb: bool = arg_opt(&p, "thumb")?.unwrap_or(true);
            let core = inner.core.lock();
            let shots = Shots::new(core.store.settings.screenshots.folder.as_deref(), &core.store.paths);
            ok(shots.data_url(&id, thumb).ok_or_else(|| RpcError::not_found("The screenshot"))?)
        }
        "screenshots.delete" => {
            let id: String = arg(&p, "id")?;
            let core = inner.core.lock();
            let shots = Shots::new(core.store.settings.screenshots.folder.as_deref(), &core.store.paths);
            if !shots.delete(&id) {
                return Err(RpcError::not_found("The screenshot"));
            }
            inner.bus.emit(Event::Screenshots);
            Ok(Value::Null)
        }
        "screenshots.open" => {
            let id: Option<String> = arg_opt(&p, "id")?;
            let core = inner.core.lock();
            let shots = Shots::new(core.store.settings.screenshots.folder.as_deref(), &core.store.paths);
            let target = match id {
                Some(i) => shots.path_of(&i).ok_or_else(|| RpcError::not_found("The screenshot"))?,
                None => {
                    let _ = std::fs::create_dir_all(shots.root());
                    shots.root().to_path_buf()
                }
            };
            system::open_path(&target).map_err(RpcError::failed)?;
            Ok(Value::Null)
        }
        // ── backup ──
        "backup.list" => ok(inner.core.lock().store.list_backups()),
        "backup.create" => {
            let core = inner.core.lock();
            let path = core.store.write_backup("manual", 30).map_err(|e| RpcError::failed(format!("Could not create the backup: {e}")))?;
            ok(json!({ "name": path.file_name().map(|n| n.to_string_lossy().to_string()) }))
        }
        "backup.export" => {
            let core = inner.core.lock();
            let b = core.store.bundle();
            ok(json!({ "filename": format!("fpsframer-backup-{}.json", stamp(now_ms())), "json": serde_json::to_string_pretty(&b)? }))
        }
        "backup.import" => {
            let text: String = arg(&p, "json")?;
            restore_bundle(inner, &text)
        }
        "backup.restore" => {
            let name: String = arg(&p, "name")?;
            if !valid_id(&name.replace(".json", "")) {
                return Err(RpcError::bad("Invalid backup name."));
            }
            let path = inner.core.lock().store.paths.backups_dir().join(&name);
            let text = std::fs::read_to_string(&path).map_err(|_| RpcError::not_found("The backup"))?;
            restore_bundle(inner, &text)
        }
        // ── system ──
        "system.processes" => {
            // Programs that are running right now, for the "pick a running program" list.
            // Background helpers that can never be a game are left out.
            let core = inner.core.lock();
            let mut seen = std::collections::BTreeSet::new();
            let mut out = Vec::new();
            for pr in &core.rt.procs {
                if fpsframer_platform::proc::is_never_a_game(&pr.exe) || !seen.insert(pr.exe.clone()) {
                    continue;
                }
                let fps = core.rt.capture.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&pr.exe));
                out.push(json!({ "exe": pr.exe, "presenting": fps }));
            }
            ok(out)
        }
        "system.openUrl" => {
            let url: String = arg(&p, "url")?;
            system::open_url(&url).map_err(RpcError::failed)?;
            Ok(Value::Null)
        }
        "system.openFolder" => {
            let kind: String = arg(&p, "kind")?;
            let core = inner.core.lock();
            let path = match kind.as_str() {
                "logs" => core.store.paths.logs_dir(),
                "data" => core.store.paths.data.clone(),
                "config" => core.store.paths.config.clone(),
                "backups" => core.store.paths.backups_dir(),
                _ => return Err(RpcError::bad("Unknown folder.")),
            };
            let _ = std::fs::create_dir_all(&path);
            system::open_path(&path).map_err(RpcError::failed)?;
            Ok(Value::Null)
        }
        "demo.setGame" => {
            let on: bool = arg(&p, "on")?;
            match &inner.demo {
                Some(w) => {
                    w.set_running(on);
                    Ok(Value::Null)
                }
                None => Err(RpcError::bad("Demo mode is not turned on.")),
            }
        }
        _ => Err(RpcError::new("unknownMethod", format!("Unknown request '{method}'."))),
    }
}

// ───────────────────────────── implementations ─────────────────────────────

fn app_state(inner: &Inner) -> R {
    let core = inner.core.lock();
    let hw = inner.hardware.lock().clone();
    ok(json!({
        "version": inner.opts.version,
        "platform": std::env::consts::OS,
        "demo": inner.demo.is_some(),
        "demoGameRunning": inner.demo.as_ref().map(|w| w.is_running()),
        "firstRun": core.first_run || !core.store.settings.first_run_completed,
        "settings": core.store.settings,
        "profiles": core.store.profiles,
        "layouts": core.store.all_layouts(),
        "rules": core.store.rules,
        "catalog": core.rt.catalog.values().collect::<Vec<_>>(),
        "status": StatusInfo { capture: core.rt.capture.clone(), providers: core.rt.providers.clone(), elevated: core.rt.elevated, capture_events: core.rt.capture_events },
        "notices": core.notices,
        "hotkeys": core.store.settings.hotkeys.assigned().into_iter().map(|(a, k)| json!({ "action": a, "accelerator": k })).collect::<Vec<_>>(),
        "snapshot": core.rt.last_snapshot,
        "hardware": hw,
        "paths": { "config": core.store.paths.config, "data": core.store.paths.data },
        "presetIds": presets().iter().map(|l| l.id.clone()).collect::<Vec<_>>(),
        "tray": core.rt.tray,
        "overlays": core.rt.specs,
    }))
}

fn settings_update(inner: &Inner, p: &Value) -> R {
    let patch = p.get("patch").ok_or_else(|| RpcError::bad("Missing 'patch'."))?;
    let mut core = inner.core.lock();
    let old = core.store.settings.clone();
    let mut v = serde_json::to_value(&old)?;
    merge(&mut v, patch);
    let mut s: Settings = serde_json::from_value(v)?;
    s.sanitize();
    for (_, k) in s.hotkeys.assigned() {
        if !accelerator_ok(&k) {
            return Err(RpcError::bad(format!("The shortcut \"{k}\" needs at least one modifier key (Ctrl, Alt or Shift) plus another key, so it doesn't interfere with your game.")));
        }
    }
    let conflicts = s.hotkeys.conflicts();
    if !conflicts.is_empty() {
        return Err(RpcError::bad(format!("These shortcuts are assigned to more than one action: {}. Each shortcut can do only one thing.", conflicts.join(", "))));
    }
    if s.companions != old.companions || s.primary_gpu != old.primary_gpu {
        inner.rebuild_providers.store(true, Ordering::SeqCst);
    }
    if s.sensor_overrides != old.sensor_overrides {
        core.rt.catalog_dirty = true;
    }
    core.store.settings = s;
    core.store.save_settings().map_err(|e| RpcError::failed(format!("Your settings could not be saved: {e}")))?;
    inner.bus.emit(Event::Settings(core.store.settings.clone()));
    core.refresh_outputs(&inner.bus);
    ok(&core.store.settings)
}

fn layouts_save(inner: &Inner, p: &Value) -> R {
    let mut l: Layout = arg(p, "layout")?;
    if l.id.starts_with("preset.") {
        return Err(RpcError::bad("Built-in layouts can't be changed. Duplicate it first, then edit your copy."));
    }
    if !valid_id(&l.id) {
        l.id = gen_id("lay");
    }
    l.builtin = false;
    l.tags.retain(|t| t != "preset");
    l.sanitize().map_err(RpcError::bad)?;
    let mut core = inner.core.lock();
    let now = now_ms();
    match core.store.layouts.iter().position(|x| x.id == l.id) {
        Some(i) => {
            l.created_at = core.store.layouts[i].created_at;
            l.updated_at = now;
            core.store.layouts[i] = l.clone();
        }
        None => {
            l.created_at = now;
            l.updated_at = now;
            core.store.layouts.push(l.clone());
        }
    }
    core.store.save_layout(&l.id).map_err(|e| RpcError::failed(format!("The layout could not be saved: {e}")))?;
    inner.bus.emit(Event::Layouts(core.store.all_layouts()));
    ok(l)
}

fn replace_layout_refs(core: &mut Core, old: &str, new: &str) -> usize {
    let mut n = 0;
    let mut changed_profiles = Vec::new();
    for pr in &mut core.store.profiles {
        let mut hit = false;
        for sc in &mut pr.scenes {
            for o in &mut sc.overlays {
                if o.layout_id == old {
                    o.layout_id = new.into();
                    n += 1;
                    hit = true;
                }
            }
        }
        if hit {
            changed_profiles.push(pr.id.clone());
        }
    }
    for id in changed_profiles {
        let _ = core.store.save_profile(&id);
    }
    let mut widgets_hit = false;
    for w in &mut core.store.settings.widgets {
        if w.layout_id == old {
            w.layout_id = new.into();
            n += 1;
            widgets_hit = true;
        }
    }
    if widgets_hit {
        let _ = core.store.save_settings();
    }
    n
}

fn layouts_delete(inner: &Inner, p: &Value) -> R {
    let id: String = arg(p, "id")?;
    if id.starts_with("preset.") {
        return Err(RpcError::bad("Built-in layouts can't be deleted."));
    }
    let mut core = inner.core.lock();
    let before = core.store.layouts.len();
    core.store.layouts.retain(|l| l.id != id);
    if core.store.layouts.len() == before {
        return Err(RpcError::not_found("The layout"));
    }
    core.store.remove_layout_file(&id);
    let replaced = replace_layout_refs(&mut core, &id, "preset.gaming");
    inner.bus.emit(Event::Layouts(core.store.all_layouts()));
    if replaced > 0 {
        inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
        inner.bus.emit(Event::Settings(core.store.settings.clone()));
    }
    core.refresh_outputs(&inner.bus);
    ok(json!({ "replacedReferences": replaced }))
}

fn fresh_ids(p: &mut Profile) {
    let now = now_ms();
    p.id = gen_id("prof");
    p.created_at = now;
    p.updated_at = now;
    let mut active = None;
    for sc in &mut p.scenes {
        let old = std::mem::replace(&mut sc.id, gen_id("sc"));
        if old == p.active_scene_id {
            active = Some(sc.id.clone());
        }
        for o in &mut sc.overlays {
            o.id = gen_id("pl");
        }
    }
    if let Some(a) = active {
        p.active_scene_id = a;
    }
}

fn clone_profile(src: &Profile, name: Option<String>) -> Profile {
    let mut c = src.clone();
    fresh_ids(&mut c);
    c.kind = ProfileKind::Game;
    c.executables.clear(); // a duplicate must not compete for the same game
    if let Some(n) = name {
        c.name = n;
    }
    c
}

fn exe_conflict(core: &Core, p: &Profile) -> Option<String> {
    for e in &p.executables {
        if let Some(other) = core.store.profiles.iter().find(|o| o.id != p.id && o.kind == ProfileKind::Game && o.matches_exe(e)) {
            return Some(format!("{e} already belongs to the profile \"{}\". A program can only have one profile.", other.name));
        }
    }
    None
}

fn profiles_save(inner: &Inner, p: &Value) -> R {
    let mut prof: Profile = arg(p, "profile")?;
    let mut core = inner.core.lock();
    let existing = core.store.profiles.iter().find(|x| x.id == prof.id).cloned();
    if !valid_id(&prof.id) || (existing.is_none() && prof.id == defaults::DEFAULT_PROFILE_ID) {
        prof.id = gen_id("prof");
    }
    prof.kind = existing.as_ref().map_or(ProfileKind::Game, |e| e.kind);
    if prof.kind == ProfileKind::Default {
        prof.executables.clear();
    }
    for sc in &mut prof.scenes {
        if !valid_id(&sc.id) {
            sc.id = gen_id("sc");
        }
        for o in &mut sc.overlays {
            if !valid_id(&o.id) {
                o.id = gen_id("pl");
            }
            if o.layout_id.is_empty() {
                o.layout_id = "preset.gaming".into();
            }
        }
    }
    if prof.scenes.is_empty() {
        let id = gen_id("sc");
        prof.active_scene_id = id.clone();
        prof.scenes.push(Scene { id, name: "Main".into(), overlays: vec![] });
    }
    prof.sanitize();
    if let Some(msg) = exe_conflict(&core, &prof) {
        return Err(RpcError::bad(msg));
    }
    let now = now_ms();
    prof.updated_at = now;
    match core.store.profiles.iter().position(|x| x.id == prof.id) {
        Some(i) => {
            prof.created_at = core.store.profiles[i].created_at;
            core.store.profiles[i] = prof.clone();
        }
        None => {
            prof.created_at = now;
            core.store.profiles.push(prof.clone());
        }
    }
    core.store.save_profile(&prof.id).map_err(|e| RpcError::failed(format!("The profile could not be saved: {e}")))?;
    inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
    core.refresh_outputs(&inner.bus);
    ok(prof)
}

fn profiles_import(inner: &Inner, p: &Value) -> R {
    let text: String = arg(p, "json")?;
    let v: Value = serde_json::from_str(&text).map_err(|_| RpcError::bad("That file is not valid JSON, so it can't be a profile."))?;
    if v.get("format").and_then(Value::as_str) != Some("fpsframer-profile") {
        return Err(RpcError::bad("This file is not an FPSFramer profile."));
    }
    let mut prof: Profile = serde_json::from_value(v.get("profile").cloned().unwrap_or(Value::Null)).map_err(|_| RpcError::bad("The profile inside this file is damaged."))?;
    let layouts: Vec<Layout> = serde_json::from_value(v.get("layouts").cloned().unwrap_or(json!([]))).unwrap_or_default();
    let mut core = inner.core.lock();
    let mut warnings = Vec::new();
    // Bring the layouts along under fresh ids and re-point the profile at them.
    for mut l in layouts {
        let old = l.id.clone();
        l.id = gen_id("lay");
        l.builtin = false;
        l.tags.retain(|t| t != "preset");
        if l.sanitize().is_err() {
            warnings.push(format!("The layout \"{}\" was skipped because it is invalid.", l.name));
            continue;
        }
        let new_id = l.id.clone();
        for sc in &mut prof.scenes {
            for o in &mut sc.overlays {
                if o.layout_id == old {
                    o.layout_id = new_id.clone();
                }
            }
        }
        core.store.layouts.push(l.clone());
        let _ = core.store.save_layout(&new_id);
    }
    fresh_ids(&mut prof);
    prof.kind = ProfileKind::Game;
    prof.sanitize();
    // Executables already owned by another profile are dropped rather than creating a conflict.
    let mut kept = Vec::new();
    for e in std::mem::take(&mut prof.executables) {
        match core.store.profiles.iter().find(|o| o.kind == ProfileKind::Game && o.matches_exe(&e)) {
            Some(o) => warnings.push(format!("{e} was left out because it already belongs to \"{}\".", o.name)),
            None => kept.push(e),
        }
    }
    prof.executables = kept;
    for sc in &mut prof.scenes {
        for o in &mut sc.overlays {
            if core.store.layout(&o.layout_id).is_none() {
                o.layout_id = "preset.gaming".into();
            }
        }
    }
    core.store.profiles.push(prof.clone());
    core.store.save_profile(&prof.id).map_err(|e| RpcError::failed(e.to_string()))?;
    inner.bus.emit(Event::Layouts(core.store.all_layouts()));
    inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
    ok(json!({ "profile": prof, "warnings": warnings }))
}

fn set_scene(inner: &Inner, core: &mut Core, profile_id: &str, scene_id: &str) -> Result<(), RpcError> {
    let prof = core.store.profiles.iter_mut().find(|x| x.id == profile_id).ok_or_else(|| RpcError::not_found("The profile"))?;
    if !prof.scenes.iter().any(|s| s.id == scene_id) {
        return Err(RpcError::not_found("The scene"));
    }
    prof.active_scene_id = scene_id.into();
    core.store.save_profile(profile_id).map_err(|e| RpcError::failed(e.to_string()))?;
    inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
    core.refresh_outputs(&inner.bus);
    Ok(())
}

fn find_placement<'a>(core: &'a mut Core, id: &str) -> Option<(&'a mut Placement, Option<String>)> {
    for pr in core.store.profiles.iter_mut() {
        let pid = pr.id.clone();
        for sc in pr.scenes.iter_mut() {
            if let Some(o) = sc.overlays.iter_mut().find(|o| o.id == id) {
                return Some((o, Some(pid)));
            }
        }
    }
    core.store.settings.widgets.iter_mut().find(|w| w.id == id).map(|w| (w, None))
}

fn persist_placement_owner(inner: &Inner, core: &mut Core, owner: Option<String>) {
    match owner {
        Some(pid) => {
            let _ = core.store.save_profile(&pid);
            inner.bus.emit(Event::Profiles(core.store.profiles.clone()));
        }
        None => {
            let _ = core.store.save_settings();
            inner.bus.emit(Event::Settings(core.store.settings.clone()));
        }
    }
    core.refresh_outputs(&inner.bus);
}

fn overlay_set_position(inner: &Inner, p: &Value) -> R {
    let id: String = arg(p, "id")?;
    let anchor: Anchor = arg(p, "anchor")?;
    let (ox, oy): (f64, f64) = (arg(p, "offsetX")?, arg(p, "offsetY")?);
    let monitor: Option<String> = arg_opt(p, "monitor")?;
    let mut core = inner.core.lock();
    let owner = {
        let (pl, owner) = find_placement(&mut core, &id).ok_or_else(|| RpcError::not_found("The overlay"))?;
        pl.anchor = anchor;
        pl.offset_x = ox;
        pl.offset_y = oy;
        if let Some(m) = monitor {
            pl.monitor = m;
        }
        pl.sanitize();
        owner
    };
    persist_placement_owner(inner, &mut core, owner);
    Ok(Value::Null)
}

/// Point a placement (or the first overlay of the active scene) at a layout.
fn overlay_set_layout(inner: &Inner, p: &Value) -> R {
    let layout_id: String = arg(p, "layoutId")?;
    let placement_id: Option<String> = arg_opt(p, "placementId")?;
    let mut core = inner.core.lock();
    if core.store.layout(&layout_id).is_none() {
        return Err(RpcError::not_found("The layout"));
    }
    let owner = if let Some(pid) = placement_id {
        let (pl, owner) = find_placement(&mut core, &pid).ok_or_else(|| RpcError::not_found("The overlay"))?;
        pl.layout_id = layout_id;
        owner
    } else {
        let prof_id = resolve_profile(&core.store, core.rt.game.as_ref().map(|g| g.exe.as_str())).id.clone();
        let prof = core.store.profiles.iter_mut().find(|x| x.id == prof_id).ok_or_else(|| RpcError::not_found("The profile"))?;
        let sid = prof.active_scene_id.clone();
        let sc = prof.scenes.iter_mut().find(|s| s.id == sid).ok_or_else(|| RpcError::not_found("The scene"))?;
        match sc.overlays.iter_mut().find(|o| o.kind == PlacementKind::Overlay) {
            Some(o) => o.layout_id = layout_id,
            None => sc.overlays.push(Placement { id: gen_id("pl"), layout_id, ..Default::default() }),
        }
        Some(prof_id)
    };
    persist_placement_owner(inner, &mut core, owner);
    Ok(Value::Null)
}

fn restore_bundle(inner: &Inner, text: &str) -> R {
    let mut bundle: BackupBundle = serde_json::from_str(text).map_err(|_| RpcError::bad("That file is not a valid FPSFramer backup."))?;
    bundle.validate().map_err(RpcError::bad)?;
    let mut core = inner.core.lock();
    let _ = core.store.write_backup("before-restore", 5); // so a restore can itself be undone
    let counts = json!({ "layouts": bundle.layouts.len(), "profiles": bundle.profiles.len(), "rules": bundle.alert_rules.len() });
    core.store.apply_bundle(bundle).map_err(RpcError::failed)?;
    inner.rebuild_providers.store(true, Ordering::SeqCst);
    core.rt.catalog_dirty = true;
    emit_all_data(inner, &mut core);
    ok(counts)
}

fn take_screenshot(inner: &Inner) -> Result<Value, RpcError> {
    let (monitor, game, folder, paths) = {
        let core = inner.core.lock();
        let monitor = if core.rt.game.is_some() { core.rt.fg.as_ref().and_then(|f| f.monitor.clone()) } else { None };
        let game = core.rt.game.as_ref().map(|g| g.name.clone()).unwrap_or_else(|| "Desktop".into());
        (monitor, game, core.store.settings.screenshots.folder.clone(), core.store.paths.clone())
    };
    let shot = match fpsframer_platform::screenshot::capture_monitor(monitor.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            toast(inner, "error", e.to_string());
            return Err(RpcError::failed(e.to_string()));
        }
    };
    let thumb = fpsframer_platform::screenshot::thumbnail(&shot, 480);
    let png = fpsframer_platform::screenshot::encode_png(&shot).map_err(RpcError::failed)?;
    let thumb_png = fpsframer_platform::screenshot::encode_png(&thumb).map_err(RpcError::failed)?;
    let shots = Shots::new(folder.as_deref(), &paths);
    let id = shots.save(&game, &png, &thumb_png, now_ms()).map_err(|e| RpcError::failed(format!("The screenshot could not be saved: {e}")))?;
    inner.bus.emit(Event::Screenshots);
    inner.bus.emit(Event::Toast(UiToast { level: "success".into(), message: format!("Screenshot saved ({game})"), action: Some(ToastAction { label: "View".into(), route: "/captures".into() }) }));
    Ok(json!({ "id": id }))
}

// ───────────────────────────── actions (hotkeys, tray) ─────────────────────────────

pub fn run_action(inner: &Inner, name: &str) {
    match name {
        "toggleOverlay" => {
            let mut core = inner.core.lock();
            core.store.settings.overlay_enabled = !core.store.settings.overlay_enabled;
            let _ = core.store.save_settings();
            inner.bus.emit(Event::Settings(core.store.settings.clone()));
            core.refresh_outputs(&inner.bus);
            let on = core.store.settings.overlay_enabled;
            drop(core);
            toast(inner, "info", if on { "Overlay on" } else { "Overlay off" });
        }
        "nextScene" | "prevScene" => {
            let mut core = inner.core.lock();
            let prof = resolve_profile(&core.store, core.rt.game.as_ref().map(|g| g.exe.as_str())).clone();
            if prof.scenes.len() < 2 {
                return;
            }
            let i = prof.scenes.iter().position(|s| s.id == prof.active_scene_id).unwrap_or(0);
            let n = prof.scenes.len();
            let j = if name == "nextScene" { (i + 1) % n } else { (i + n - 1) % n };
            let (sid, sname) = (prof.scenes[j].id.clone(), prof.scenes[j].name.clone());
            if set_scene(inner, &mut core, &prof.id, &sid).is_ok() {
                drop(core);
                toast(inner, "info", format!("Scene: {sname}"));
            }
        }
        "toggleEdit" => {
            let mut core = inner.core.lock();
            core.rt.edit_mode = !core.rt.edit_mode;
            core.refresh_outputs(&inner.bus);
            let on = core.rt.edit_mode;
            drop(core);
            toast(inner, "info", if on { "Move mode: drag the overlay, then press the shortcut again to lock it" } else { "Overlay locked" });
        }
        "toggleRecording" => {
            let mut core = inner.core.lock();
            if core.rt.session.is_some() {
                let kept = core.finish_session(inner);
                core.refresh_outputs(&inner.bus);
                drop(core);
                if !kept {
                    toast(inner, "info", "Recording stopped (too short to keep)");
                }
            } else {
                let hw = inner.hardware.lock().clone().unwrap_or_default();
                core.start_session(hw, true);
                core.refresh_outputs(&inner.bus);
                drop(core);
                toast(inner, "info", "Recording started");
            }
        }
        "screenshot" => {
            let _ = take_screenshot(inner);
        }
        "resetStats" => {
            inner.core.lock().reset_stats();
            toast(inner, "info", "Statistics reset");
        }
        _ => {}
    }
}
