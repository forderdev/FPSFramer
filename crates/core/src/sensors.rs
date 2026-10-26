//! Sensor model shared by every provider, the engine and the UI.
//!
//! The central rule of this module: **a missing reading is never a zero.**
//! A sensor either has a value in the [`Readings`] map or it does not; when it
//! does not, its [`SensorDesc::availability`] explains why in plain language.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Well-known sensor ids. Providers and the engine agree on these names.
pub mod ids {
    pub const FPS: &str = "fps";
    pub const FPS_AVG: &str = "fps.avg";
    pub const FPS_MIN: &str = "fps.min";
    pub const FPS_MAX: &str = "fps.max";
    pub const FPS_LOW1: &str = "fps.low1";
    pub const FPS_LOW01: &str = "fps.low01";
    pub const FRAMETIME: &str = "frametime";
    pub const FRAMETIME_MAX: &str = "frametime.max";
    pub const GAME_NAME: &str = "game.name";
    pub const GAME_API: &str = "game.api";
    pub const SESSION_TIME: &str = "session.time";

    pub const CPU_LOAD: &str = "cpu.load";
    pub const CPU_CORE_MAX: &str = "cpu.core.max";
    pub const CPU_CLOCK: &str = "cpu.clock";
    pub const CPU_TEMP: &str = "cpu.temp";
    pub const CPU_POWER: &str = "cpu.power";
    pub const CPU_NAME: &str = "cpu.name";

    pub const GPU_LOAD: &str = "gpu.load";
    pub const GPU_TEMP: &str = "gpu.temp";
    pub const GPU_CLOCK: &str = "gpu.clock";
    pub const GPU_MEMCLOCK: &str = "gpu.memclock";
    pub const GPU_POWER: &str = "gpu.power";
    pub const GPU_FAN: &str = "gpu.fan";
    pub const GPU_VRAM_USED: &str = "gpu.vram.used";
    pub const GPU_VRAM_TOTAL: &str = "gpu.vram.total";
    pub const GPU_VRAM_PERCENT: &str = "gpu.vram.percent";
    pub const GPU_THROTTLE: &str = "gpu.throttle";
    pub const GPU_NAME: &str = "gpu.name";

    pub const RAM_USED: &str = "ram.used";
    pub const RAM_TOTAL: &str = "ram.total";
    pub const RAM_AVAILABLE: &str = "ram.available";
    pub const RAM_PERCENT: &str = "ram.percent";

    pub const DISK_READ: &str = "disk.read";
    pub const DISK_WRITE: &str = "disk.write";
    pub const DISK_ACTIVE: &str = "disk.active";
    pub const DISK_LATENCY: &str = "disk.latency";

    pub const NET_DOWN: &str = "net.down";
    pub const NET_UP: &str = "net.up";

    pub const BATTERY_PERCENT: &str = "battery.percent";
    pub const POWER_SOURCE: &str = "power.source";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Fps,
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    Power,
    Thermal,
    Fan,
    Voltage,
    System,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    Percent,
    Celsius,
    Mhz,
    Megabytes,
    BytesPerSec,
    Watt,
    Volt,
    Rpm,
    Ms,
    Fps,
    Seconds,
    Count,
    Text,
}

impl Unit {
    /// Decimal places that make sense when the value is stored/serialised.
    pub fn precision(self) -> i32 {
        match self {
            Unit::Volt => 3,
            Unit::Fps | Unit::Ms | Unit::Percent | Unit::Celsius | Unit::Watt => 1,
            _ => 0,
        }
    }
}

/// A single reading. Text readings exist for things like the game name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reading {
    Num(f64),
    Text(String),
}

impl Reading {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Reading::Num(v) => Some(*v),
            Reading::Text(_) => None,
        }
    }
}

/// Map of sensor id → reading. Absent key = unavailable (never "0").
pub type Readings = BTreeMap<String, Reading>;

/// Convenience helpers for building reading maps.
pub trait ReadingsExt {
    fn put(&mut self, id: &str, value: f64, unit: Unit);
    fn put_text(&mut self, id: &str, value: impl Into<String>);
    fn num(&self, id: &str) -> Option<f64>;
}

impl ReadingsExt for Readings {
    fn put(&mut self, id: &str, value: f64, unit: Unit) {
        if !value.is_finite() {
            return; // NaN / inf is "no reading", never a number to show.
        }
        let p = 10f64.powi(unit.precision());
        self.insert(id.to_string(), Reading::Num((value * p).round() / p));
    }
    fn put_text(&mut self, id: &str, value: impl Into<String>) {
        self.insert(id.to_string(), Reading::Text(value.into()));
    }
    fn num(&self, id: &str) -> Option<f64> {
        self.get(id).and_then(Reading::as_f64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    HigherIsWorse,
    LowerIsWorse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Normal,
    Elevated,
    Critical,
}

/// Defines when a reading is "elevated" or "critical" (colour / label in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusRule {
    pub direction: Direction,
    pub elevated: f64,
    pub critical: f64,
}

impl StatusRule {
    pub const fn higher(elevated: f64, critical: f64) -> Self {
        Self { direction: Direction::HigherIsWorse, elevated, critical }
    }
    pub const fn lower(elevated: f64, critical: f64) -> Self {
        Self { direction: Direction::LowerIsWorse, elevated, critical }
    }
    pub fn evaluate(&self, v: f64) -> Status {
        match self.direction {
            Direction::HigherIsWorse => {
                if v >= self.critical {
                    Status::Critical
                } else if v >= self.elevated {
                    Status::Elevated
                } else {
                    Status::Normal
                }
            }
            Direction::LowerIsWorse => {
                if v <= self.critical {
                    Status::Critical
                } else if v <= self.elevated {
                    Status::Elevated
                } else {
                    Status::Normal
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnavailableCode {
    /// The machine has no such hardware (e.g. no discrete GPU, no battery).
    NoHardware,
    /// Windows cannot expose this without a hardware driver; a companion tool can.
    NeedsCompanion,
    /// Needs elevated rights or a one-time permission.
    NeedsPermission,
    /// Only exists while a game is being measured.
    NoActiveGame,
    /// This hardware/driver doesn't report it.
    NotSupported,
    /// Needs a second sample before a value can be computed.
    Warming,
    /// The data source failed; it will be retried.
    ProviderError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unavailable {
    pub code: UnavailableCode,
    /// Plain-language explanation shown in tooltips.
    pub message: String,
    /// Optional "how to fix it" hint.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub available: bool,
    pub reason: Option<Unavailable>,
}

impl Availability {
    pub fn yes() -> Self {
        Self { available: true, reason: None }
    }
    pub fn no(code: UnavailableCode, message: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            available: false,
            reason: Some(Unavailable { code, message: message.into(), fix: fix.map(str::to_string) }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorDesc {
    pub id: String,
    /// Full, plain-language name: "CPU temperature".
    pub name: String,
    /// Short label for compact displays: "CPU".
    pub short: String,
    /// What this measures, in words a beginner understands.
    pub description: String,
    pub category: Category,
    pub unit: Unit,
    /// Sub-group within a category: "Core 3", "GPU 2 · Intel UHD Graphics".
    pub group: Option<String>,
    /// Provider that supplies it (shown as a source badge).
    pub source: String,
    pub status_rule: Option<StatusRule>,
    pub availability: Availability,
    /// Whether session recordings include this sensor by default.
    pub recordable: bool,
    /// Natural range for bars and gauges.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub keywords: Vec<String>,
}

impl SensorDesc {
    pub fn new(id: impl Into<String>, name: impl Into<String>, category: Category, unit: Unit) -> Self {
        let name = name.into();
        Self {
            id: id.into(),
            short: name.clone(),
            name,
            description: String::new(),
            category,
            unit,
            group: None,
            source: String::new(),
            status_rule: None,
            availability: Availability::yes(),
            recordable: false,
            min: None,
            max: None,
            keywords: Vec::new(),
        }
    }
    pub fn short(mut self, s: impl Into<String>) -> Self {
        self.short = s.into();
        self
    }
    pub fn desc(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }
    pub fn group(mut self, s: impl Into<String>) -> Self {
        self.group = Some(s.into());
        self
    }
    pub fn source(mut self, s: impl Into<String>) -> Self {
        self.source = s.into();
        self
    }
    pub fn rule(mut self, r: StatusRule) -> Self {
        self.status_rule = Some(r);
        self
    }
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
    pub fn rec(mut self) -> Self {
        self.recordable = true;
        self
    }
    pub fn kw(mut self, words: &[&str]) -> Self {
        self.keywords = words.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn unavailable(mut self, code: UnavailableCode, message: impl Into<String>, fix: Option<&str>) -> Self {
        self.availability = Availability::no(code, message, fix);
        self
    }
    pub fn is_numeric(&self) -> bool {
        self.unit != Unit::Text
    }
}

/// Convert celsius to the user's preferred unit for text (used by alert messages).
pub fn format_temp(c: f64, fahrenheit: bool) -> String {
    if fahrenheit {
        format!("{:.0}°F", c * 9.0 / 5.0 + 32.0)
    } else {
        format!("{:.0}°C", c)
    }
}

/// Running min / max / average for one sensor since the stats were reset.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunningStat {
    pub min: f64,
    pub max: f64,
    sum: f64,
    pub count: u64,
}

impl RunningStat {
    pub fn push(&mut self, v: f64) {
        if self.count == 0 {
            self.min = v;
            self.max = v;
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.sum += v;
        self.count += 1;
    }
    pub fn avg(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum / self.count as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_rule_higher_and_lower() {
        let r = StatusRule::higher(80.0, 90.0);
        assert_eq!(r.evaluate(79.9), Status::Normal);
        assert_eq!(r.evaluate(80.0), Status::Elevated);
        assert_eq!(r.evaluate(95.0), Status::Critical);
        let l = StatusRule::lower(30.0, 10.0);
        assert_eq!(l.evaluate(50.0), Status::Normal);
        assert_eq!(l.evaluate(30.0), Status::Elevated);
        assert_eq!(l.evaluate(5.0), Status::Critical);
    }

    #[test]
    fn put_rounds_and_rejects_non_finite() {
        let mut r = Readings::new();
        r.put("a", 12.3456, Unit::Percent);
        r.put("b", f64::NAN, Unit::Percent);
        r.put("c", f64::INFINITY, Unit::Percent);
        r.put("d", 1.23456, Unit::Volt);
        assert_eq!(r.num("a"), Some(12.3));
        assert_eq!(r.num("b"), None, "NaN must stay unavailable, not become 0");
        assert_eq!(r.num("c"), None);
        assert_eq!(r.num("d"), Some(1.235));
    }

    #[test]
    fn zero_is_a_real_reading() {
        let mut r = Readings::new();
        r.put("gpu.load", 0.0, Unit::Percent);
        assert_eq!(r.num("gpu.load"), Some(0.0));
        assert_eq!(r.num("gpu.temp"), None);
    }

    #[test]
    fn running_stat() {
        let mut s = RunningStat::default();
        assert_eq!(s.avg(), None);
        for v in [3.0, 1.0, 2.0] {
            s.push(v);
        }
        assert_eq!((s.min, s.max, s.avg()), (1.0, 3.0, Some(2.0)));
    }
}
