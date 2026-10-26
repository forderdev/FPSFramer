//! Engine integration tests: run the real sampler against the demo world and the RPC surface.

use super::*;
use serde_json::json;
use std::time::{Duration, Instant};

fn engine(demo: bool) -> (Engine, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("fpsf-engine-{}-{}", std::process::id(), fpsframer_core::util::gen_id("e")));
    let _ = std::fs::remove_dir_all(&dir);
    let e = Engine::new(EngineOptions { paths: Paths::in_dir(&dir), demo, version: "test".into() });
    (e, dir)
}

fn wait_for<T>(mut f: impl FnMut() -> Option<T>, secs: u64) -> Option<T> {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn fresh_state_has_presets_profiles_and_rules() {
    let (e, _d) = engine(false);
    let st = e.call("app.getState", json!({})).unwrap();
    assert_eq!(st["firstRun"], true);
    assert!(st["layouts"].as_array().unwrap().iter().any(|l| l["id"] == "preset.minimal"));
    assert!(st["profiles"].as_array().unwrap().iter().any(|p| p["id"] == "default"));
    assert!(!st["rules"].as_array().unwrap().is_empty());
}

#[test]
fn settings_update_validates_and_persists() {
    let (e, dir) = engine(false);
    let s = e.call("settings.update", json!({ "patch": { "theme": "light", "updateIntervalMs": 1 } })).unwrap();
    assert_eq!(s["theme"], "light");
    assert_eq!(s["updateIntervalMs"], 250, "out-of-range values are clamped");
    let err = e.call("settings.update", json!({ "patch": { "hotkeys": { "toggleOverlay": "F9" } } })).unwrap_err();
    assert!(err.message.contains("modifier"), "{}", err.message);
    let err = e.call("settings.update", json!({ "patch": { "hotkeys": { "nextScene": "Ctrl+Shift+F9" } } })).unwrap_err();
    assert!(err.message.contains("more than one"), "{}", err.message);
    // A second engine on the same folder sees the saved theme.
    let e2 = Engine::new(EngineOptions { paths: Paths::in_dir(&dir), demo: false, version: "t".into() });
    assert_eq!(e2.call("app.getState", json!({})).unwrap()["settings"]["theme"], "light");
}

#[test]
fn layout_lifecycle_and_builtin_protection() {
    let (e, _d) = engine(false);
    let err = e.call("layouts.save", json!({ "layout": { "id": "preset.gaming", "name": "x", "elements": [] } })).unwrap_err();
    assert!(err.message.contains("Duplicate"));
    let copy = e.call("layouts.duplicate", json!({ "id": "preset.gaming", "name": "Mine" })).unwrap();
    let id = copy["id"].as_str().unwrap().to_string();
    assert_eq!(copy["builtin"], false);
    // Use it in the active scene, then delete it: references fall back to a preset.
    e.call("overlay.setLayout", json!({ "layoutId": id })).unwrap();
    let del = e.call("layouts.delete", json!({ "id": id })).unwrap();
    assert_eq!(del["replacedReferences"], 1);
    let profiles = e.call("profiles.list", json!({})).unwrap();
    let json = profiles.to_string();
    assert!(!json.contains(&id), "no dangling layout reference");
    assert!(e.call("layouts.delete", json!({ "id": "preset.minimal" })).is_err());
}

#[test]
fn layout_export_import_roundtrip_and_hostile_import() {
    let (e, _d) = engine(false);
    let ex = e.call("layouts.export", json!({ "id": "preset.detailed" })).unwrap();
    let imported = e.call("layouts.import", json!({ "json": ex["json"] })).unwrap();
    assert_ne!(imported["id"], "preset.detailed");
    assert_eq!(imported["builtin"], false);
    assert!(e.call("layouts.import", json!({ "json": "{\"format\":\"nope\"}" })).is_err());
    assert!(e.call("layouts.import", json!({ "json": "not json" })).is_err());
}

#[test]
fn profile_rules_executable_conflicts_and_default_protection() {
    let (e, _d) = engine(false);
    assert!(e.call("profiles.delete", json!({ "id": "default" })).is_err());
    let p = e.call("profiles.save", json!({ "profile": { "id": "", "name": "CS", "executables": ["cs2.exe"], "scenes": [] } }));
    let err = p.unwrap_err();
    assert!(err.message.contains("already belongs"), "cs2.exe is in the template profile: {}", err.message);
    let p = e.call("profiles.save", json!({ "profile": { "id": "", "name": "My game", "executables": ["mygame.exe"], "scenes": [] } })).unwrap();
    assert_eq!(p["kind"], "game");
    assert_eq!(p["scenes"].as_array().unwrap().len(), 1, "a profile always has at least one scene");
    let exp = e.call("profiles.export", json!({ "id": p["id"] })).unwrap();
    let imp = e.call("profiles.import", json!({ "json": exp["json"] })).unwrap();
    assert_eq!(imp["profile"]["executables"].as_array().unwrap().len(), 0, "duplicate exe is dropped");
    assert!(!imp["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn alert_rules_crud() {
    let (e, _d) = engine(false);
    let r = e.call("rules.save", json!({ "rule": { "name": "", "sensorId": "gpu.temp", "threshold": 80 } })).unwrap();
    assert!(r["id"].as_str().unwrap().starts_with("rule_"));
    assert!(e.call("rules.save", json!({ "rule": { "sensorId": "" } })).is_err());
    e.call("rules.delete", json!({ "id": r["id"] })).unwrap();
    assert!(!e.call("rules.list", json!({})).unwrap().to_string().contains(r["id"].as_str().unwrap()));
}

#[test]
fn unknown_method_and_bad_params_are_clean_errors() {
    let (e, _d) = engine(false);
    assert_eq!(e.call("nope", json!({})).unwrap_err().code, "unknownMethod");
    assert_eq!(e.call("layouts.duplicate", json!({})).unwrap_err().code, "badRequest");
    assert!(e.call("sessions.get", json!({ "id": "../../etc/passwd" })).is_err());
    assert!(e.call("system.openUrl", json!({ "url": "file:///etc/passwd" })).is_err());
}

#[test]
fn backup_restore_roundtrip() {
    let (e, _d) = engine(false);
    e.call("settings.update", json!({ "patch": { "theme": "light" } })).unwrap();
    let b = e.call("backup.export", json!({})).unwrap();
    e.call("settings.update", json!({ "patch": { "theme": "dark" } })).unwrap();
    e.call("backup.import", json!({ "json": b["json"] })).unwrap();
    assert_eq!(e.call("app.getState", json!({})).unwrap()["settings"]["theme"], "light");
    assert!(e.call("backup.import", json!({ "json": "{}" })).is_err());
    let list = e.call("backup.list", json!({})).unwrap();
    assert!(list.to_string().contains("before-restore"), "a restore makes its own undo point");
}

#[test]
fn demo_game_end_to_end() {
    // The full pipeline: sensors + frames → game detection → snapshot → session → analysis → disk.
    let (e, _d) = engine(true);
    let rx = e.subscribe();
    e.start();
    // Complete setup and shorten the minimum so a short test session is kept.
    e.call("settings.update", json!({ "patch": { "updateIntervalMs": 250, "recording": { "minDurationS": 0 } } })).unwrap();

    // Before the game: hardware is live but FPS is unavailable, with a reason.
    let idle = wait_for(|| e.call("live.snapshot", json!({})).ok().filter(|v| !v.is_null()), 5).expect("a snapshot arrives");
    assert_eq!(idle["demo"], true);
    assert!(idle["values"].get("fps").is_none(), "no game ⇒ no FPS value (not 0)");
    assert!(idle["values"].get("cpu.load").is_some());
    let cat = e.call("app.getState", json!({})).unwrap();
    let fps = cat["catalog"].as_array().unwrap().iter().find(|d| d["id"] == "fps").unwrap();
    assert_eq!(fps["availability"]["available"], false);
    assert_eq!(fps["availability"]["reason"]["code"], "noActiveGame");

    // Start the simulated game.
    e.call("demo.setGame", json!({ "on": true })).unwrap();
    let live = wait_for(
        || {
            let s = e.call("live.snapshot", json!({})).ok()?;
            (s["capture"]["state"] == "tracking" && s["values"].get("fps").is_some() && s["values"].get("fps.low1").is_some()).then_some(s)
        },
        25,
    )
    .expect("the demo game should be detected and measured");
    let fps = live["values"]["fps"].as_f64().unwrap();
    assert!((40.0..140.0).contains(&fps), "plausible FPS, got {fps}");
    assert_eq!(live["capture"]["exe"], "demogame.exe");
    assert_eq!(live["gameActive"], true);
    assert!(live["session"].is_object(), "a session records automatically");
    assert!(!live["frames"].as_array().unwrap().is_empty() || live["values"]["frametime"].is_number());

    // Overlay becomes visible for the game.
    let mut saw_overlay = false;
    let mut saw_tray = false;
    while let Ok(ev) = rx.try_recv() {
        match &*ev {
            Event::Overlays(v) if !v.is_empty() => saw_overlay = true,
            Event::Tray(t) if t.game.is_some() => saw_tray = true,
            _ => {}
        }
    }
    assert!(saw_overlay && saw_tray);

    // Let it run, stop the game, and expect a saved, analysed session.
    std::thread::sleep(Duration::from_millis(4500));
    e.call("demo.setGame", json!({ "on": false })).unwrap();
    let list = wait_for(
        || {
            let l = e.call("sessions.list", json!({})).ok()?;
            (!l.as_array()?.is_empty()).then_some(l)
        },
        20,
    )
    .expect("the session is saved after the game ends");
    let item = &list[0];
    assert_eq!(item["meta"]["demo"], true, "demo data is always marked");
    assert_eq!(item["meta"]["gameName"], "demogame");
    let full = e.call("sessions.get", json!({ "id": item["meta"]["id"] })).unwrap();
    let avg = full["summary"]["fpsAvg"].as_f64().unwrap();
    assert!((40.0..140.0).contains(&avg), "avg {avg}");
    assert!(full["columns"]["fps"].as_array().unwrap().len() >= 3);
    assert!(full["columns"].get("gpu.load").is_some());
    let csv = e.call("sessions.export", json!({ "id": item["meta"]["id"], "format": "csv" })).unwrap();
    assert!(csv["text"].as_str().unwrap().starts_with("time_s,fps,"));

    e.shutdown();
}

#[test]
fn manual_recording_works_without_a_game() {
    let (e, _d) = engine(true);
    e.start();
    wait_for(|| e.call("live.snapshot", json!({})).ok().filter(|v| !v.is_null()), 5).unwrap();
    e.call("sessions.startRecording", json!({})).unwrap();
    assert!(e.call("sessions.startRecording", json!({})).is_err(), "can't start twice");
    std::thread::sleep(Duration::from_millis(3600));
    let r = e.call("sessions.stopRecording", json!({})).unwrap();
    assert_eq!(r["saved"], true);
    let l = wait_for(|| e.call("sessions.list", json!({})).ok().filter(|l| !l.as_array().unwrap().is_empty()), 10).unwrap();
    assert_eq!(l[0]["meta"]["kind"], "manual");
    assert!(l[0]["fpsAvg"].is_null(), "no game ⇒ no FPS figures");
    e.shutdown();
}

#[test]
fn alerts_fire_from_real_pipeline_values() {
    let (e, _d) = engine(true);
    let rx = e.subscribe();
    e.start();
    e.call("settings.update", json!({ "patch": { "updateIntervalMs": 250 } })).unwrap();
    // RAM is ~36 % in the demo at idle; alert at 10 % must fire quickly.
    e.call("rules.save", json!({ "rule": { "id": "t1", "name": "RAM test", "sensorId": "ram.percent", "comparator": "above", "threshold": 10, "forSeconds": 0, "cooldownSeconds": 60 } })).unwrap();
    let fired = wait_for(
        || {
            while let Ok(ev) = rx.try_recv() {
                if let Event::AlertLog(a) = &*ev {
                    if a.rule_id == "t1" && a.kind == "fired" {
                        return Some(a.message.clone());
                    }
                }
            }
            None
        },
        8,
    )
    .expect("alert should fire");
    assert!(fired.contains('%'), "message is human readable: {fired}");
    let snap = e.call("live.snapshot", json!({})).unwrap();
    assert!(snap["alerts"].as_array().unwrap().iter().any(|a| a["ruleId"] == "t1"));
    e.shutdown();
}

#[test]
fn real_linux_sensors_flow_through_the_engine() {
    if !std::path::Path::new("/proc/stat").exists() {
        return;
    }
    let (e, _d) = engine(false);
    e.start();
    e.call("settings.update", json!({ "patch": { "updateIntervalMs": 250 } })).unwrap();
    let snap = wait_for(
        || {
            let s = e.call("live.snapshot", json!({})).ok()?;
            s["values"].get("cpu.load").is_some().then_some(s)
        },
        8,
    )
    .expect("real CPU load reading");
    assert_eq!(snap["demo"], false);
    assert!(snap["values"]["ram.total"].as_f64().unwrap() > 100.0);
    assert!(snap["values"].get("fps").is_none(), "no FPS on a machine with no game: unavailable, not zero");
    let st = e.call("app.getState", json!({})).unwrap();
    let cpu_temp = st["catalog"].as_array().unwrap().iter().find(|d| d["id"] == "cpu.temp").unwrap();
    // Either a real sensor exists, or it is clearly marked unavailable with a reason.
    if cpu_temp["availability"]["available"] == false {
        assert!(cpu_temp["availability"]["reason"]["message"].as_str().unwrap().len() > 10);
        assert!(snap["values"].get("cpu.temp").is_none());
    }
    let diag = e.call("diagnostics.run", json!({})).unwrap();
    assert!(diag.as_array().unwrap().iter().any(|c| c["id"] == "capture"));
    let report = e.call("diagnostics.report", json!({})).unwrap();
    assert!(report.as_str().unwrap().contains("FPSFramer diagnostic report"));
    e.shutdown();
}
