//! Persistent user-facing data: settings, profiles, scenes, overlay placements and alert rules.
//!
//! Every struct uses `#[serde(default)]` so files written by older or newer
//! versions still load; unknown fields are ignored.

use crate::frames::LowsMethod;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

// ───────────────────────────── settings ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiMode {
    #[default]
    Simple,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
    HighContrast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetUnit {
    /// Megabits per second (Mbps) – what internet plans are sold in.
    #[default]
    Megabits,
    /// Megabytes per second (MB/s) – what download managers show.
    Megabytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatsWindow {
    #[default]
    Session,
    Last30s,
    Last60s,
    Last5m,
}

impl StatsWindow {
    pub fn seconds(self) -> Option<u32> {
        match self {
            StatsWindow::Session => None,
            StatsWindow::Last30s => Some(30),
            StatsWindow::Last60s => Some(60),
            StatsWindow::Last5m => Some(300),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Units {
    pub temperature: TempUnit,
    pub network: NetUnit,
}
impl Default for Units {
    fn default() -> Self {
        Self { temperature: TempUnit::Celsius, network: NetUnit::Megabits }
    }
}

/// Hotkeys as accelerator strings, e.g. `"Ctrl+Shift+F9"`. `None` = unassigned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub toggle_overlay: Option<String>,
    pub next_scene: Option<String>,
    pub prev_scene: Option<String>,
    pub toggle_edit: Option<String>,
    pub toggle_recording: Option<String>,
    pub screenshot: Option<String>,
    pub reset_stats: Option<String>,
    pub open_app: Option<String>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        let s = |v: &str| Some(v.to_string());
        Self {
            toggle_overlay: s("Ctrl+Shift+F9"),
            next_scene: s("Ctrl+Shift+F10"),
            prev_scene: s("Ctrl+Shift+F8"),
            toggle_edit: s("Ctrl+Shift+F7"),
            toggle_recording: s("Ctrl+Shift+F6"),
            screenshot: s("Ctrl+Shift+F12"),
            reset_stats: s("Ctrl+Shift+F5"),
            open_app: None,
        }
    }
}

impl Hotkeys {
    /// `(action id, accelerator)` for every assigned hotkey.
    pub fn assigned(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        let mut add = |id: &'static str, k: &Option<String>| {
            if let Some(k) = k.as_ref().filter(|k| !k.trim().is_empty()) {
                v.push((id, k.clone()));
            }
        };
        add("toggleOverlay", &self.toggle_overlay);
        add("nextScene", &self.next_scene);
        add("prevScene", &self.prev_scene);
        add("toggleEdit", &self.toggle_edit);
        add("toggleRecording", &self.toggle_recording);
        add("screenshot", &self.screenshot);
        add("resetStats", &self.reset_stats);
        add("openApp", &self.open_app);
        v
    }

    /// Accelerators bound to more than one action (compared case-insensitively).
    pub fn conflicts(&self) -> Vec<String> {
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        for (_, k) in self.assigned() {
            *seen.entry(k.to_lowercase().replace(' ', "")).or_default() += 1;
        }
        seen.into_iter().filter(|(_, n)| *n > 1).map(|(k, _)| k).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayGlobal {
    /// Periodically re-assert "always on top" so games that fight for it don't hide the overlay.
    pub reassert_topmost: bool,
    /// Ask Windows to leave overlay windows out of screenshots / recordings / streams.
    pub exclude_from_capture: bool,
}
impl Default for OverlayGlobal {
    fn default() -> Self {
        Self { reassert_topmost: true, exclude_from_capture: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecordingSettings {
    /// Automatically record a session whenever a game is being measured.
    pub auto_record: bool,
    /// Sessions shorter than this are discarded (accidental launches).
    pub min_duration_s: u32,
    /// Oldest sessions are removed beyond this count (0 = keep all).
    pub max_sessions: u32,
    /// Also record sensors from companion tools (larger files).
    pub record_all_sensors: bool,
}
impl Default for RecordingSettings {
    fn default() -> Self {
        Self { auto_record: true, min_duration_s: 30, max_sessions: 200, record_all_sensors: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationSettings {
    pub toast: bool,
    pub sound: bool,
    /// Post a short summary notification when a recorded session ends.
    pub session_summary: bool,
}
impl Default for NotificationSettings {
    fn default() -> Self {
        Self { toast: true, sound: false, session_summary: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    /// Pick the game automatically.
    #[default]
    Auto,
    /// Only measure the process named in `manual_process`.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptureSettings {
    pub mode: CaptureMode,
    pub manual_process: Option<String>,
    /// Executable names that are never treated as games.
    pub ignored: Vec<String>,
}
impl Default for CaptureSettings {
    fn default() -> Self {
        Self { mode: CaptureMode::Auto, manual_process: None, ignored: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Companions {
    pub hwinfo: bool,
    pub libre_hardware_monitor: bool,
}
impl Default for Companions {
    fn default() -> Self {
        Self { hwinfo: true, libre_hardware_monitor: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ScreenshotSettings {
    /// Folder for screenshots; `None` = Pictures\FPSFramer.
    pub folder: Option<String>,
}

/// User-chosen thresholds for one sensor (overrides the built-in defaults).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorOverride {
    pub elevated: Option<f64>,
    pub critical: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub first_run_completed: bool,
    pub ui_mode: UiMode,
    pub theme: Theme,
    /// Accent colour for the app interface (hex).
    pub accent: String,
    /// 0.85 – 1.6; scales all interface text and controls.
    pub ui_scale: f64,
    pub reduce_motion: bool,
    pub units: Units,
    /// How often sensors are read and pushed to overlays.
    pub update_interval_ms: u32,
    pub fps_window_ms: u32,
    pub stats_window: StatsWindow,
    pub lows_method: LowsMethod,
    /// Master switch: when off, no overlay is shown anywhere.
    pub overlay_enabled: bool,
    pub overlay: OverlayGlobal,
    /// Always-on desktop widgets (independent of games and profiles).
    pub widgets: Vec<Placement>,
    pub hotkeys: Hotkeys,
    pub launch_at_startup: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub recording: RecordingSettings,
    pub notifications: NotificationSettings,
    pub capture: CaptureSettings,
    pub companions: Companions,
    pub screenshots: ScreenshotSettings,
    /// Index of the GPU shown as "GPU" (None = automatic: the most capable one).
    pub primary_gpu: Option<usize>,
    pub sensor_overrides: BTreeMap<String, SensorOverride>,
    /// Always use this profile instead of auto-detecting.
    pub pinned_profile: Option<String>,
    pub auto_backup: bool,
    /// Whether the user dismissed the hint about overlay/exclusive fullscreen.
    pub dismissed_tips: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            first_run_completed: false,
            ui_mode: UiMode::Simple,
            theme: Theme::Dark,
            accent: "#3ddcff".into(),
            ui_scale: 1.0,
            reduce_motion: false,
            units: Units::default(),
            update_interval_ms: 500,
            fps_window_ms: 1000,
            stats_window: StatsWindow::Session,
            lows_method: LowsMethod::Average,
            overlay_enabled: true,
            overlay: OverlayGlobal::default(),
            widgets: Vec::new(),
            hotkeys: Hotkeys::default(),
            launch_at_startup: false,
            start_minimized: false,
            close_to_tray: true,
            recording: RecordingSettings::default(),
            notifications: NotificationSettings::default(),
            capture: CaptureSettings::default(),
            companions: Companions::default(),
            screenshots: ScreenshotSettings::default(),
            primary_gpu: None,
            sensor_overrides: BTreeMap::new(),
            pinned_profile: None,
            auto_backup: true,
            dismissed_tips: Vec::new(),
        }
    }
}

impl Settings {
    /// Clamp every numeric value into a range the rest of the app can rely on.
    pub fn sanitize(&mut self) {
        self.update_interval_ms = self.update_interval_ms.clamp(250, 5000);
        self.fps_window_ms = self.fps_window_ms.clamp(250, 5000);
        self.ui_scale = if self.ui_scale.is_finite() { self.ui_scale.clamp(0.85, 1.6) } else { 1.0 };
        self.recording.min_duration_s = self.recording.min_duration_s.min(3600);
        self.recording.max_sessions = self.recording.max_sessions.min(5000);
        if !is_hex_color(&self.accent) {
            self.accent = "#3ddcff".into();
        }
        self.schema_version = SCHEMA_VERSION;
        // An override with neither level set is just "use the default" – drop it.
        self.sensor_overrides.retain(|_, o| o.elevated.is_some() || o.critical.is_some());
        for w in &mut self.widgets {
            w.sanitize();
        }
        self.capture.ignored.iter_mut().for_each(|s| *s = s.trim().to_lowercase());
        self.capture.ignored.retain(|s| !s.is_empty());
        self.capture.ignored.dedup();
    }
}

pub fn is_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    b.first() == Some(&b'#') && matches!(b.len(), 4 | 7 | 9) && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

// ─────────────────────── placements, scenes, profiles ───────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    /// Horizontal / vertical fraction of the free space (0 = left/top, 1 = right/bottom).
    pub fn fractions(self) -> (f64, f64) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::TopCenter => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::MiddleLeft => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::MiddleRight => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::BottomCenter => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlacementKind {
    /// Floats above games; click-through.
    #[default]
    Overlay,
    /// Desktop widget: a normal small window that stays on the desktop.
    Widget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Only while a game is being measured.
    #[default]
    Game,
    Always,
}

/// Where and how one overlay layout is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Placement {
    pub id: String,
    pub layout_id: String,
    pub kind: PlacementKind,
    pub enabled: bool,
    /// `"game"` (monitor the game is on), `"primary"`, or a monitor name.
    pub monitor: String,
    pub anchor: Anchor,
    pub offset_x: f64,
    pub offset_y: f64,
    /// Extra zoom on top of the layout (1.0 = as designed).
    pub scale: f64,
    pub opacity: f64,
    pub click_through: bool,
    pub visibility: Visibility,
    pub always_on_top: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            id: String::new(),
            layout_id: String::new(),
            kind: PlacementKind::Overlay,
            enabled: true,
            monitor: "game".into(),
            anchor: Anchor::TopLeft,
            offset_x: 16.0,
            offset_y: 16.0,
            scale: 1.0,
            opacity: 1.0,
            click_through: true,
            visibility: Visibility::Game,
            always_on_top: true,
        }
    }
}

impl Placement {
    pub fn sanitize(&mut self) {
        self.scale = if self.scale.is_finite() { self.scale.clamp(0.4, 4.0) } else { 1.0 };
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.1, 1.0) } else { 1.0 };
        self.offset_x = if self.offset_x.is_finite() { self.offset_x.clamp(-8000.0, 8000.0) } else { 0.0 };
        self.offset_y = if self.offset_y.is_finite() { self.offset_y.clamp(-8000.0, 8000.0) } else { 0.0 };
        if self.monitor.trim().is_empty() {
            self.monitor = "game".into();
        }
        if self.kind == PlacementKind::Widget {
            self.always_on_top = false;
        }
    }
}

/// A named set of overlays – "Minimal", "Detailed", "Streaming".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub overlays: Vec<Placement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProfileKind {
    /// "All games": used when a game is running but has no profile of its own.
    Default,
    #[default]
    Game,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kind: ProfileKind,
    /// Lower-case executable names this profile applies to, e.g. `"cs2.exe"`.
    pub executables: Vec<String>,
    pub notes: String,
    pub scenes: Vec<Scene>,
    pub active_scene_id: String,
    /// FPS the player is aiming for; drives colours, alerts and summaries.
    pub target_fps: Option<f64>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Profile {
    pub fn active_scene(&self) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == self.active_scene_id).or_else(|| self.scenes.first())
    }

    pub fn matches_exe(&self, exe: &str) -> bool {
        let e = exe.trim().to_lowercase();
        self.executables.iter().any(|x| x.trim().to_lowercase() == e)
    }

    pub fn sanitize(&mut self) {
        self.executables = self
            .executables
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        self.executables.dedup();
        if self.name.trim().is_empty() {
            self.name = "Untitled profile".into();
        }
        self.name = self.name.chars().take(80).collect();
        self.notes = self.notes.chars().take(2000).collect();
        if let Some(t) = self.target_fps {
            self.target_fps = if t.is_finite() && t >= 1.0 { Some(t.min(1000.0)) } else { None };
        }
        for s in &mut self.scenes {
            for p in &mut s.overlays {
                p.sanitize();
            }
        }
        if !self.scenes.iter().any(|s| s.id == self.active_scene_id) {
            self.active_scene_id = self.scenes.first().map(|s| s.id.clone()).unwrap_or_default();
        }
    }
}

// ───────────────────────────── alerts ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Comparator {
    #[default]
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    #[default]
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertNotify {
    /// Show the alert in the overlay.
    pub overlay: bool,
    /// Show a Windows notification when it starts.
    pub toast: bool,
    pub sound: bool,
}
impl Default for AlertNotify {
    fn default() -> Self {
        Self { overlay: true, toast: true, sound: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub sensor_id: String,
    pub comparator: Comparator,
    pub threshold: f64,
    /// The condition must hold this long before the alert fires (avoids blips).
    pub for_seconds: u32,
    /// After firing, don't notify again for this long.
    pub cooldown_seconds: u32,
    /// How far back past the threshold the value must move to clear the alert.
    pub hysteresis: f64,
    pub severity: Severity,
    pub notify: AlertNotify,
    /// Empty = every profile.
    pub profile_ids: Vec<String>,
    /// Only evaluate while a game is being measured.
    pub game_only: bool,
    /// Optional custom text; `{value}`, `{threshold}` and `{sensor}` are replaced.
    pub message: String,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            sensor_id: String::new(),
            comparator: Comparator::Above,
            threshold: 0.0,
            for_seconds: 5,
            cooldown_seconds: 120,
            hysteresis: 3.0,
            severity: Severity::Warning,
            notify: AlertNotify::default(),
            profile_ids: Vec::new(),
            game_only: false,
            message: String::new(),
        }
    }
}

impl AlertRule {
    pub fn sanitize(&mut self) {
        self.name = self.name.chars().take(80).collect();
        self.message = self.message.chars().take(240).collect();
        self.for_seconds = self.for_seconds.min(3600);
        self.cooldown_seconds = self.cooldown_seconds.min(86_400);
        if !self.threshold.is_finite() {
            self.threshold = 0.0;
        }
        self.hysteresis = if self.hysteresis.is_finite() { self.hysteresis.abs() } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_tolerate_missing_fields() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        // An old/partial file still loads, with defaults filled in.
        let partial: Settings = serde_json::from_str(r#"{"theme":"light","unknownFutureField":1}"#).unwrap();
        assert_eq!(partial.theme, Theme::Light);
        assert_eq!(partial.update_interval_ms, 500);
    }

    #[test]
    fn sanitize_clamps_hostile_values() {
        let mut s = Settings { update_interval_ms: 1, ui_scale: f64::NAN, accent: "javascript:alert(1)".into(), ..Settings::default() };
        s.sanitize();
        assert_eq!(s.update_interval_ms, 250);
        assert_eq!(s.ui_scale, 1.0);
        assert_eq!(s.accent, "#3ddcff");
    }

    #[test]
    fn hotkey_conflicts_detected() {
        let mut h = Hotkeys::default();
        assert!(h.conflicts().is_empty());
        h.next_scene = h.toggle_overlay.clone();
        assert_eq!(h.conflicts().len(), 1);
    }

    #[test]
    fn profile_matching_is_case_insensitive() {
        let mut p = Profile { executables: vec!["CS2.exe".into()], ..Default::default() };
        p.sanitize();
        assert!(p.matches_exe("cs2.EXE"));
        assert!(!p.matches_exe("csgo.exe"));
    }

    #[test]
    fn profile_sanitize_repairs_active_scene() {
        let mut p = Profile {
            scenes: vec![Scene { id: "a".into(), name: "A".into(), overlays: vec![] }],
            active_scene_id: "missing".into(),
            ..Default::default()
        };
        p.sanitize();
        assert_eq!(p.active_scene_id, "a");
    }

    #[test]
    fn hex_colour_validation() {
        assert!(is_hex_color("#fff"));
        assert!(is_hex_color("#12ab34"));
        assert!(is_hex_color("#12ab34cc"));
        assert!(!is_hex_color("red"));
        assert!(!is_hex_color("#12ab3"));
    }
}
