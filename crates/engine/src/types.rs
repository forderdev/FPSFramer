//! Data shapes exchanged with the UI and the native shell (serialised as camelCase JSON).

use fpsframer_core::alerts::ActiveAlert;
use fpsframer_core::model::{Anchor, PlacementKind, Severity};
use fpsframer_core::sensors::Readings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInfo {
    /// `searching` | `tracking` | `stalled` | `unsupportedApi` | `needsPermission` | `unsupported` | `error` | `stopped`
    pub state: String,
    pub message: Option<String>,
    pub pid: Option<u32>,
    pub exe: Option<String>,
    pub name: Option<String>,
    pub api: Option<String>,
}

impl CaptureInfo {
    pub fn searching() -> Self {
        Self { state: "searching".into(), message: None, pid: None, exe: None, name: None, api: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionLive {
    pub id: String,
    pub elapsed_s: f64,
    pub manual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLive {
    pub id: String,
    pub name: String,
    pub scene_id: String,
    pub scene_name: String,
    pub target_fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub seq: u64,
    /// Unix milliseconds.
    pub t: u64,
    /// Everything in this snapshot is simulated.
    pub demo: bool,
    pub values: Readings,
    /// `[min, max, average]` since the statistics were last reset.
    pub stats: BTreeMap<String, [f64; 3]>,
    /// Frame times (ms) since the previous snapshot, newest last.
    pub frames: Vec<f32>,
    pub capture: CaptureInfo,
    pub alerts: Vec<ActiveAlert>,
    pub session: Option<SessionLive>,
    pub profile: ProfileLive,
    pub overlay_enabled: bool,
    pub edit_mode: bool,
    /// A game (or program with its own profile) is currently being measured.
    pub game_active: bool,
    /// Windows reports an exclusive full-screen Direct3D program, which no overlay window can be drawn over.
    pub exclusive_fullscreen: bool,
}

/// One overlay window the shell must show.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySpec {
    pub id: String,
    pub layout_id: String,
    pub kind: PlacementKind,
    /// Monitor name, `"game"` or `"primary"`.
    pub monitor: String,
    pub anchor: Anchor,
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale: f64,
    pub opacity: f64,
    /// Mouse clicks pass through to the game (off while repositioning).
    pub click_through: bool,
    pub always_on_top: bool,
    pub edit_mode: bool,
    pub exclude_from_capture: bool,
    pub reassert_topmost: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub level: Severity,
    pub sound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiToast {
    /// `info` | `success` | `warning` | `error`
    pub level: String,
    pub message: String,
    /// Optional action button, e.g. `{ "label": "View", "route": "/sessions/abc" }`.
    pub action: Option<ToastAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastAction {
    pub label: String,
    pub route: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertLogItem {
    pub t: u64,
    pub rule_id: String,
    pub name: String,
    pub severity: Severity,
    pub message: String,
    /// `fired` | `cleared` | `suppressed`
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyBinding {
    pub action: String,
    pub accelerator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TraySceneItem {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrayState {
    pub overlay_enabled: bool,
    pub recording: bool,
    pub profile_id: String,
    pub profile_name: String,
    pub scenes: Vec<TraySceneItem>,
    pub game: Option<String>,
    pub alert_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub label: String,
    /// `ok` | `degraded` | `failed`
    pub health: String,
    pub message: Option<String>,
    pub sensors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub capture: CaptureInfo,
    pub providers: Vec<ProviderStatus>,
    pub elevated: bool,
    pub capture_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagFix {
    pub label: String,
    /// Built-in action id the UI can run: `grantCapture`, `restartElevated`, `openUrl`, `openRoute`.
    pub action: String,
    pub arg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagCheck {
    pub id: String,
    pub title: String,
    /// `ok` | `info` | `warn` | `fail`
    pub status: String,
    pub summary: String,
    pub details: Vec<String>,
    pub fix: Option<DiagFix>,
}
