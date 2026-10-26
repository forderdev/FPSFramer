//! What a fresh install contains, so the app is useful immediately.

use crate::model::*;
use crate::sensors::ids;
use crate::util::{gen_id, now_ms};

pub const DEFAULT_PROFILE_ID: &str = "default";

fn placement(layout_id: &str) -> Placement {
    Placement { id: gen_id("pl"), layout_id: layout_id.into(), ..Default::default() }
}

fn scene(name: &str, layout_id: &str) -> Scene {
    Scene { id: gen_id("sc"), name: name.into(), overlays: vec![placement(layout_id)] }
}

fn scene_ids(scenes: &[Scene], name: &str) -> String {
    scenes.iter().find(|s| s.name == name).map(|s| s.id.clone()).unwrap_or_default()
}

pub fn default_profiles() -> Vec<Profile> {
    let now = now_ms();
    let all_scenes = vec![
        scene("Minimal", "preset.minimal"),
        scene("Gaming", "preset.gaming"),
        scene("Detailed", "preset.detailed"),
        scene("Advanced", "preset.advanced"),
    ];
    let all = Profile {
        id: DEFAULT_PROFILE_ID.into(),
        name: "All games".into(),
        kind: ProfileKind::Default,
        executables: vec![],
        notes: "Used for any game that doesn't have its own profile.".into(),
        active_scene_id: scene_ids(&all_scenes, "Gaming"),
        scenes: all_scenes,
        target_fps: None,
        created_at: now,
        updated_at: now,
    };

    let comp_scenes = vec![scene("Minimal", "preset.minimal"), scene("Gaming", "preset.gaming")];
    let comp = Profile {
        id: "tpl-competitive".into(),
        name: "Competitive shooters".into(),
        kind: ProfileKind::Game,
        executables: [
            "cs2.exe",
            "csgo.exe",
            "r5apex.exe",
            "r5apex_dx12.exe",
            "valorant-win64-shipping.exe",
            "overwatch.exe",
            "rainbowsix.exe",
            "rainbowsix_vulkan.exe",
            "fortniteclient-win64-shipping.exe",
            "tslgame.exe",
            "dota2.exe",
            "rocketleague.exe",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        notes: "Small, unobtrusive overlay for fast games. Edit the list to match what you play.".into(),
        active_scene_id: scene_ids(&comp_scenes, "Minimal"),
        scenes: comp_scenes,
        target_fps: Some(144.0),
        created_at: now,
        updated_at: now,
    };

    let aaa_scenes = vec![
        scene("Detailed", "preset.detailed"),
        scene("Gaming", "preset.gaming"),
        scene("Advanced", "preset.advanced"),
    ];
    let aaa = Profile {
        id: "tpl-open-world".into(),
        name: "Open-world & cinematic games".into(),
        kind: ProfileKind::Game,
        executables: [
            "cyberpunk2077.exe",
            "eldenring.exe",
            "rdr2.exe",
            "gta5.exe",
            "witcher3.exe",
            "bg3.exe",
            "bg3_dx11.exe",
            "starfield.exe",
            "hogwartslegacy.exe",
            "forzahorizon5.exe",
            "horizonzerodawn.exe",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        notes: "Shows temperatures and clocks. Demanding games stress the hardware for hours.".into(),
        active_scene_id: scene_ids(&aaa_scenes, "Detailed"),
        scenes: aaa_scenes,
        target_fps: Some(60.0),
        created_at: now,
        updated_at: now,
    };
    vec![all, comp, aaa]
}

/// A desktop widget that is present (but switched off) on a fresh install.
pub fn default_widgets() -> Vec<Placement> {
    vec![Placement {
        id: gen_id("pl"),
        layout_id: "preset.widget".into(),
        kind: PlacementKind::Widget,
        enabled: false,
        monitor: "primary".into(),
        anchor: Anchor::TopRight,
        offset_x: 24.0,
        offset_y: 24.0,
        visibility: Visibility::Always,
        always_on_top: false,
        click_through: false,
        ..Default::default()
    }]
}

pub fn default_settings() -> Settings {
    Settings { widgets: default_widgets(), ..Default::default() }
}

fn rule(id: &str, name: &str, sensor: &str, cmp: Comparator, threshold: f64, for_s: u32, sev: Severity) -> AlertRule {
    AlertRule {
        id: id.into(),
        name: name.into(),
        sensor_id: sensor.into(),
        comparator: cmp,
        threshold,
        for_seconds: for_s,
        severity: sev,
        hysteresis: match sensor {
            s if s.contains("temp") => 4.0,
            s if s.contains("percent") || s.contains("load") => 3.0,
            _ => (threshold.abs() * 0.05).max(1.0),
        },
        ..Default::default()
    }
}

/// Alerts that exist (and are on) from the first launch. They stay silent until a real problem appears.
pub fn default_rules() -> Vec<AlertRule> {
    let mut r = vec![
        rule("rule.cpu-temp", "CPU running hot", ids::CPU_TEMP, Comparator::Above, 90.0, 8, Severity::Warning),
        rule("rule.gpu-temp", "Graphics card running hot", ids::GPU_TEMP, Comparator::Above, 85.0, 8, Severity::Warning),
        rule("rule.ram", "Memory almost full", ids::RAM_PERCENT, Comparator::Above, 92.0, 15, Severity::Warning),
        rule("rule.vram", "Video memory almost full", ids::GPU_VRAM_PERCENT, Comparator::Above, 95.0, 10, Severity::Warning),
        rule("rule.fps-low", "FPS very low", ids::FPS, Comparator::Below, 30.0, 15, Severity::Warning),
    ];
    r[4].game_only = true;
    r[4].cooldown_seconds = 300;
    r[4].hysteresis = 5.0;
    r
}

/// Ready-made rules offered in the "Add alert" dialog. They are not enabled until added.
pub fn rule_templates() -> Vec<AlertRule> {
    let mut v = vec![
        rule("tpl.cpu-temp-critical", "CPU dangerously hot", ids::CPU_TEMP, Comparator::Above, 95.0, 3, Severity::Critical),
        rule("tpl.gpu-temp-critical", "Graphics card dangerously hot", ids::GPU_TEMP, Comparator::Above, 90.0, 3, Severity::Critical),
        rule("tpl.cpu-load", "CPU maxed out", ids::CPU_LOAD, Comparator::Above, 98.0, 20, Severity::Info),
        rule("tpl.gpu-load", "GPU maxed out", ids::GPU_LOAD, Comparator::Above, 99.0, 30, Severity::Info),
        rule("tpl.frametime", "Slow frames (stutter)", ids::FRAMETIME_MAX, Comparator::Above, 60.0, 0, Severity::Info),
        rule("tpl.fps-target", "FPS below my target", ids::FPS, Comparator::Below, 60.0, 10, Severity::Warning),
        rule("tpl.gpu-power", "GPU power very high", ids::GPU_POWER, Comparator::Above, 350.0, 10, Severity::Info),
        rule("tpl.gpu-fan", "GPU fan near maximum", ids::GPU_FAN, Comparator::Above, 90.0, 15, Severity::Warning),
        rule("tpl.disk-latency", "Disk responding slowly", ids::DISK_LATENCY, Comparator::Above, 50.0, 10, Severity::Info),
        rule("tpl.battery", "Battery low", ids::BATTERY_PERCENT, Comparator::Below, 20.0, 5, Severity::Warning),
    ];
    for r in &mut v {
        if r.sensor_id == ids::FPS || r.sensor_id == ids::FRAMETIME_MAX {
            r.game_only = true;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::preset_by_id;

    #[test]
    fn default_profiles_reference_existing_presets_and_have_valid_active_scene() {
        let ps = default_profiles();
        assert_eq!(ps[0].kind, ProfileKind::Default);
        for p in ps {
            assert!(p.scenes.iter().any(|s| s.id == p.active_scene_id), "{} has no active scene", p.name);
            for s in &p.scenes {
                for o in &s.overlays {
                    assert!(preset_by_id(&o.layout_id).is_some(), "unknown preset {}", o.layout_id);
                }
            }
        }
    }

    #[test]
    fn default_rules_are_unique_and_target_real_sensors() {
        let rules = default_rules();
        let mut seen = std::collections::HashSet::new();
        for r in &rules {
            assert!(seen.insert(r.id.clone()));
            assert!(!r.sensor_id.is_empty() && r.cooldown_seconds >= 60);
        }
    }

    #[test]
    fn fresh_install_has_a_disabled_desktop_widget() {
        let s = default_settings();
        assert_eq!(s.widgets.len(), 1);
        assert!(!s.widgets[0].enabled && s.widgets[0].kind == PlacementKind::Widget);
    }
}
