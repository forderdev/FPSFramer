//! "Is everything working, and if not, why?" – the compatibility checks behind Help → Diagnostics.

use crate::logging;
use crate::types::*;
use crate::{Core, Inner};
use fpsframer_core::sensors::{ids, UnavailableCode};
use fpsframer_platform::system;

fn chk(id: &str, title: &str, status: &str, summary: impl Into<String>) -> DiagCheck {
    DiagCheck { id: id.into(), title: title.into(), status: status.into(), summary: summary.into(), details: vec![], fix: None }
}

fn fix(label: &str, action: &str, arg: Option<&str>) -> Option<DiagFix> {
    Some(DiagFix { label: label.into(), action: action.into(), arg: arg.map(str::to_string) })
}

fn dir_writable(p: &std::path::Path) -> bool {
    if std::fs::create_dir_all(p).is_err() {
        return false;
    }
    let probe = p.join(".write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn run(inner: &Inner) -> Vec<DiagCheck> {
    let core = inner.core.lock();
    let mut out = vec![capture_check(&core, inner), sensor_check(&core), provider_check(&core), companion_check(&core), overlay_check(&core)];
    if let Some(c) = layout_check(&core) {
        out.push(c);
    }
    out.push(anticheat_check(&core));
    out.push(storage_check(&core, inner));
    out.push(log_check());
    if inner.demo.is_some() {
        out.push(chk("demo", "Demo mode", "warn", "Demo mode is on: every value you see is simulated. Turn it off to see your real hardware."));
    }
    out
}

fn capture_check(core: &Core, inner: &Inner) -> DiagCheck {
    let c = &core.rt.capture;
    let mut d = match c.state.as_str() {
        "tracking" => chk("capture", "FPS measurement", "ok", format!("Measuring {} ({}).", c.name.clone().unwrap_or_default(), c.api.clone().unwrap_or_else(|| "DirectX".into()))),
        "stalled" => chk("capture", "FPS measurement", "info", "The game is open but not drawing frames right now (paused, minimised or frozen)."),
        "needsPermission" => {
            let mut x = chk("capture", "FPS measurement", "fail", "Windows has not given FPSFramer permission to measure FPS yet.");
            x.details = vec![
                "Measuring frame rate uses Windows' built-in performance tracing, which Windows only allows for administrators and members of the \"Performance Log Users\" group.".into(),
                "Recommended: grant permission once. You will see a Windows prompt, then sign out and back in.".into(),
                "Quick alternative: restart FPSFramer as administrator (needed every time).".into(),
            ];
            x.fix = fix("Grant permission", "grantCapture", None);
            x
        }
        "unsupportedApi" => {
            let mut x = chk("capture", "FPS measurement", "warn", c.message.clone().unwrap_or_default());
            x.details = vec!["FPSFramer measures DirectX 9, 10, 11 and 12 games. Vulkan and OpenGL games are not supported yet. If the game offers a DirectX mode, choose it.".into()];
            x
        }
        "unsupported" => chk("capture", "FPS measurement", "fail", c.message.clone().unwrap_or_else(|| "Not available on this system.".into())),
        "error" => {
            let mut x = chk("capture", "FPS measurement", "fail", c.message.clone().unwrap_or_else(|| "FPS measurement could not start.".into()));
            x.fix = fix("Try again", "retryCapture", None);
            x
        }
        _ => {
            let mut x = chk("capture", "FPS measurement", "info", "Ready. Start a game and FPSFramer will find it automatically.");
            x.details = vec![
                "Works with DirectX 9/10/11/12 games in windowed, borderless or \"fullscreen optimised\" mode.".into(),
                "If nothing is detected while a DirectX game is in the foreground, make sure the game is in borderless or windowed mode.".into(),
            ];
            if core.rt.capture_events == 0 && inner.demo.is_none() {
                x.details.push("No graphics events have been received since FPSFramer started.".into());
            }
            x
        }
    };
    d.details.push(format!("Graphics events received so far: {}", core.rt.capture_events));
    d
}

fn sensor_check(core: &Core) -> DiagCheck {
    let key: [(&str, &str); 10] = [
        (ids::CPU_LOAD, "CPU usage"),
        (ids::CPU_TEMP, "CPU temperature"),
        (ids::GPU_LOAD, "GPU usage"),
        (ids::GPU_TEMP, "GPU temperature"),
        (ids::GPU_VRAM_USED, "Video memory"),
        (ids::GPU_POWER, "GPU power"),
        (ids::RAM_PERCENT, "Memory usage"),
        (ids::DISK_READ, "Disk speed"),
        (ids::NET_DOWN, "Network speed"),
        (ids::GPU_FAN, "GPU fan speed"),
    ];
    let mut details = Vec::new();
    let mut missing = 0;
    let mut needs_companion = false;
    for (id, label) in key {
        match core.rt.catalog.get(id) {
            Some(d) if d.availability.available => details.push(format!("✓ {label}")),
            Some(d) => {
                missing += 1;
                let reason = d.availability.reason.as_ref();
                if reason.is_some_and(|r| r.code == UnavailableCode::NeedsCompanion) {
                    needs_companion = true;
                }
                details.push(format!("✗ {label}: {}", reason.map(|r| r.message.clone()).unwrap_or_else(|| "not available".into())));
            }
            None => {
                missing += 1;
                details.push(format!("✗ {label}: not available on this PC"));
            }
        }
    }
    let total = core.rt.catalog.values().filter(|d| d.availability.available).count();
    let mut c = chk(
        "sensors",
        "Hardware sensors",
        if missing == 0 { "ok" } else if missing <= 3 { "info" } else { "warn" },
        if missing == 0 { format!("All key sensors work ({total} sensors available).") } else { format!("{missing} of 10 key sensors are unavailable ({total} others work). This is normal for some hardware.") },
    );
    c.details = details;
    if needs_companion {
        c.fix = fix("How to add the missing sensors", "openRoute", Some("/help/sensors"));
    }
    c
}

fn provider_check(core: &Core) -> DiagCheck {
    let bad: Vec<&ProviderStatus> = core.rt.providers.iter().filter(|p| p.health != "ok").collect();
    let mut c = if bad.is_empty() {
        chk("providers", "Sensor sources", "ok", format!("{} sensor source{} running normally.", core.rt.providers.len(), if core.rt.providers.len() == 1 { "" } else { "s" }))
    } else {
        chk("providers", "Sensor sources", "warn", format!("{} sensor source{} had problems. FPSFramer keeps retrying automatically.", bad.len(), if bad.len() == 1 { "" } else { "s" }))
    };
    c.details = core
        .rt
        .providers
        .iter()
        .map(|p| format!("{} {}: {}{}", if p.health == "ok" { "✓" } else { "✗" }, p.label, p.health, p.message.as_ref().map(|m| format!(" ({m})")).unwrap_or_default()))
        .collect();
    c
}

fn companion_check(core: &Core) -> DiagCheck {
    let has = |src: &str| core.rt.catalog.values().any(|d| d.source == src && d.availability.available);
    let (hw, lhm) = (has("HWiNFO"), has("LibreHardwareMonitor"));
    let enabled = (core.store.settings.companions.hwinfo, core.store.settings.companions.libre_hardware_monitor);
    let mut c = if hw || lhm {
        chk("companions", "Companion sensor tools", "ok", format!("Reading extra sensors from {}.", [(hw, "HWiNFO"), (lhm, "LibreHardwareMonitor")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join(" and ")))
    } else {
        chk("companions", "Companion sensor tools", "info", "No companion tool detected. CPU temperature, fan speeds and voltages need one on Windows (optional).")
    };
    c.details = vec![
        format!("HWiNFO import: {}", if enabled.0 { if hw { "running" } else { "enabled, HWiNFO not detected" } } else { "turned off in Settings" }),
        format!("LibreHardwareMonitor import: {}", if enabled.1 { if lhm { "running" } else { "enabled, not detected" } } else { "turned off in Settings" }),
        "In HWiNFO open Settings → turn on \"Shared Memory Support\". LibreHardwareMonitor works automatically while it is running.".into(),
    ];
    if !(hw || lhm) {
        c.fix = fix("Show me how", "openRoute", Some("/help/sensors"));
    }
    c
}

fn overlay_check(core: &Core) -> DiagCheck {
    let s = &core.store.settings;
    let mut details = vec![
        format!("Overlay switched on: {}", if s.overlay_enabled { "yes" } else { "no" }),
        format!("Overlay windows requested right now: {}", core.rt.specs.len()),
    ];
    let mut status = "ok";
    let mut summary = String::from("The overlay works over games in windowed and borderless mode.");
    if let Some(fg) = &core.rt.fg {
        details.push(format!("Foreground program: {}{}", fg.exe, if fg.fullscreen { " (fills the screen)" } else { "" }));
    }
    if core.rt.exclusive_fullscreen == Some(true) {
        status = "warn";
        summary = "Windows reports an exclusive full-screen game. Overlays cannot be drawn on top of those.".into();
        details.push("Switch the game's display mode to \"Borderless\" or \"Windowed\" to see the overlay.".into());
    }
    details.push("FPSFramer shows the overlay as a transparent window above the game instead of injecting code into it. That keeps it compatible with anti-cheat software, but it cannot appear over true exclusive full-screen.".into());
    let mut c = chk("overlay", "Overlay", status, summary);
    c.details = details;
    c
}

fn layout_check(core: &Core) -> Option<DiagCheck> {
    let mut missing = Vec::new();
    for p in &core.store.profiles {
        for sc in &p.scenes {
            for o in &sc.overlays {
                if core.store.layout(&o.layout_id).is_none() {
                    missing.push(format!("{} → {}: layout no longer exists", p.name, sc.name));
                }
            }
        }
    }
    if missing.is_empty() {
        return None;
    }
    let mut c = chk("layouts", "Overlay layouts", "warn", "Some scenes point at a layout that was deleted, so nothing is shown for them.");
    c.details = missing;
    c.fix = fix("Open profiles", "openRoute", Some("/profiles"));
    Some(c)
}

fn anticheat_check(core: &Core) -> DiagCheck {
    let names: Vec<String> = core.rt.procs.iter().map(|p| p.exe.clone()).collect();
    let found = system::detect_anticheat(&names);
    let mut c = if found.is_empty() {
        chk("anticheat", "Anti-cheat software", "ok", "No anti-cheat service is running right now.")
    } else {
        chk("anticheat", "Anti-cheat software", "info", format!("Detected: {}.", found.join(", ")))
    };
    c.details = vec![
        "FPSFramer never injects code into games, never reads game memory and never tries to get around anti-cheat protection.".into(),
        "It only reads Windows' own performance data and draws a separate transparent window. Even so, some games' rules may restrict overlays. If a game objects, turn the overlay off for that profile.".into(),
    ];
    c
}

fn storage_check(core: &Core, inner: &Inner) -> DiagCheck {
    let p = &core.store.paths;
    let (cw, dw) = (dir_writable(&p.config), dir_writable(&p.data));
    let sessions = inner.sessions.list();
    let mut c = if cw && dw {
        chk("storage", "Saved data", "ok", format!("Settings and recordings are saved correctly ({} sessions stored).", sessions.len()))
    } else {
        chk("storage", "Saved data", "fail", "FPSFramer cannot write to its data folder, so settings and recordings will not be saved.")
    };
    c.details = vec![format!("Settings folder: {}", logging::redact(&p.config.to_string_lossy())), format!("Data folder: {}", logging::redact(&p.data.to_string_lossy()))];
    for n in &core.notices {
        c.details.push(logging::redact(n));
    }
    if !core.notices.is_empty() && cw && dw {
        c.status = "info".into();
        c.summary.push_str(" Some files were repaired when FPSFramer started.");
    }
    c
}

fn log_check() -> DiagCheck {
    let n = logging::recent_problem_count();
    if n == 0 {
        chk("logs", "Recent errors", "ok", "No warnings or errors since FPSFramer started.")
    } else {
        let mut c = chk("logs", "Recent errors", "info", format!("{n} warning or error message{} were logged since FPSFramer started.", if n == 1 { "" } else { "s" }));
        c.details = logging::recent(60)
            .into_iter()
            .filter(|l| l.level == "ERROR" || l.level == "WARN")
            .rev()
            .take(8)
            .map(|l| format!("{} {}", l.level, logging::redact(&l.message)))
            .collect();
        c
    }
}

/// Plain-text report users can paste into a support request. Personal paths are redacted.
pub fn report(inner: &Inner) -> String {
    let checks = run(inner);
    let hw = inner.hardware.lock().clone().unwrap_or_default();
    let core = inner.core.lock();
    let mut s = String::new();
    s.push_str(&format!("FPSFramer diagnostic report\nVersion: {}\nPlatform: {}\nAdministrator: {}\n", inner.opts.version, std::env::consts::OS, core.rt.elevated));
    s.push_str(&format!("CPU: {}\nGPU: {}\nRAM: {:.1} GB\nOS: {}\nDisplays: {}\n\n", hw.cpu, hw.gpus.join(", "), hw.ram_gb, hw.os, hw.displays.join("; ")));
    for c in &checks {
        s.push_str(&format!("[{}] {}: {}\n", c.status.to_uppercase(), c.title, c.summary));
        for d in &c.details {
            s.push_str(&format!("      {d}\n"));
        }
    }
    s.push_str("\nUnavailable sensors:\n");
    for d in core.rt.catalog.values().filter(|d| !d.availability.available) {
        s.push_str(&format!("  {} ({}): {}\n", d.name, d.id, d.availability.reason.as_ref().map(|r| r.message.as_str()).unwrap_or("")));
    }
    s.push_str("\nRecent log:\n");
    for l in logging::recent(120) {
        s.push_str(&format!("  {} {}\n", l.level, logging::redact(&l.message)));
    }
    s
}
