//! Deciding which overlay windows should exist right now.
//!
//! The engine never touches windows; it publishes a declarative list
//! ([`OverlaySpec`]) and the native shell makes reality match it.

use crate::types::*;
use fpsframer_core::model::{Placement, PlacementKind, Profile, Settings, Visibility};

pub struct SpecInput<'a> {
    pub settings: &'a Settings,
    pub profile: &'a Profile,
    /// A game (or a program with its own profile) is running.
    pub game_active: bool,
    pub edit_mode: bool,
    /// The user asked to preview the overlay on the desktop.
    pub preview: bool,
    /// Monitor the game window is on, if known.
    pub game_monitor: Option<&'a str>,
    pub layout_exists: &'a dyn Fn(&str) -> bool,
}

fn to_spec(p: &Placement, i: &SpecInput, widget: bool) -> OverlaySpec {
    let monitor = if p.monitor == "game" {
        i.game_monitor.map(str::to_string).unwrap_or_else(|| "primary".into())
    } else {
        p.monitor.clone()
    };
    OverlaySpec {
        id: p.id.clone(),
        layout_id: p.layout_id.clone(),
        kind: p.kind,
        monitor,
        anchor: p.anchor,
        offset_x: p.offset_x,
        offset_y: p.offset_y,
        scale: p.scale,
        opacity: p.opacity,
        click_through: p.click_through && !i.edit_mode,
        always_on_top: if widget { p.always_on_top } else { true },
        edit_mode: i.edit_mode,
        exclude_from_capture: !widget && i.settings.overlay.exclude_from_capture,
        reassert_topmost: !widget && i.settings.overlay.reassert_topmost,
    }
}

pub fn compute_specs(i: &SpecInput) -> Vec<OverlaySpec> {
    let mut out = Vec::new();
    if let Some(scene) = i.profile.active_scene() {
        for p in &scene.overlays {
            if p.kind != PlacementKind::Overlay || !p.enabled || !(i.layout_exists)(&p.layout_id) {
                continue;
            }
            let shown = i.settings.overlay_enabled
                && (i.edit_mode || i.preview || i.game_active || p.visibility == Visibility::Always);
            if shown {
                out.push(to_spec(p, i, false));
            }
        }
    }
    for p in &i.settings.widgets {
        if p.enabled && (i.layout_exists)(&p.layout_id) {
            out.push(to_spec(p, i, true));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpsframer_core::defaults::{default_profiles, default_settings};

    fn exists(_: &str) -> bool {
        true
    }

    fn input<'a>(s: &'a Settings, p: &'a Profile, game: bool, edit: bool, preview: bool) -> SpecInput<'a> {
        SpecInput { settings: s, profile: p, game_active: game, edit_mode: edit, preview, game_monitor: Some("\\\\.\\DISPLAY2"), layout_exists: &exists }
    }

    #[test]
    fn overlay_is_hidden_at_the_desktop_and_shown_in_game() {
        let s = default_settings();
        let p = default_profiles().remove(0);
        assert!(compute_specs(&input(&s, &p, false, false, false)).is_empty());
        let specs = compute_specs(&input(&s, &p, true, false, false));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].layout_id, "preset.gaming");
        assert!(specs[0].click_through, "overlay must never block clicks by default");
        assert_eq!(specs[0].monitor, "\\\\.\\DISPLAY2", "'game' resolves to the game's monitor");
    }

    #[test]
    fn master_switch_hides_everything_but_widgets() {
        let mut s = default_settings();
        s.overlay_enabled = false;
        s.widgets[0].enabled = true;
        let p = default_profiles().remove(0);
        let specs = compute_specs(&input(&s, &p, true, false, false));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].kind, PlacementKind::Widget);
    }

    #[test]
    fn edit_mode_and_preview_show_the_overlay_and_make_it_grabbable() {
        let s = default_settings();
        let p = default_profiles().remove(0);
        let prev = compute_specs(&input(&s, &p, false, false, true));
        assert_eq!(prev.len(), 1);
        assert!(prev[0].click_through);
        let edit = compute_specs(&input(&s, &p, false, true, false));
        assert_eq!(edit.len(), 1);
        assert!(!edit[0].click_through, "edit mode must accept mouse input so the overlay can be dragged");
    }

    #[test]
    fn missing_layouts_are_skipped_not_crashed_on() {
        let s = default_settings();
        let p = default_profiles().remove(0);
        let none = |_: &str| false;
        let i = SpecInput { layout_exists: &none, ..input(&s, &p, true, false, false) };
        assert!(compute_specs(&i).is_empty());
    }

    #[test]
    fn disabled_placements_stay_hidden() {
        let s = default_settings();
        let mut p = default_profiles().remove(0);
        let sc = p.scenes.iter().position(|x| x.id == p.active_scene_id).unwrap();
        p.scenes[sc].overlays[0].enabled = false;
        assert!(compute_specs(&input(&s, &p, true, true, true)).is_empty());
    }
}
