//! The contract every data source implements.

use fpsframer_core::sensors::{Readings, SensorDesc};

#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    Ok,
    /// Still producing some data, but something is wrong (shown in diagnostics).
    Degraded(String),
    /// Producing nothing; the engine will recreate the provider after a back-off.
    Failed(String),
}

pub struct PollOutcome {
    /// The set of sensors (or their availability) changed; call `descriptors()` again.
    pub catalog_changed: bool,
    pub health: Health,
}

impl PollOutcome {
    pub fn ok() -> Self {
        Self { catalog_changed: false, health: Health::Ok }
    }
    pub fn failed(msg: impl Into<String>) -> Self {
        Self { catalog_changed: false, health: Health::Failed(msg.into()) }
    }
}

pub trait SensorProvider: Send {
    /// Stable machine id, e.g. `"windows-basic"`.
    fn id(&self) -> &'static str;
    /// Human name for diagnostics, e.g. `"Windows performance counters"`.
    fn label(&self) -> &'static str;
    /// The sensors this provider knows about, including ones that are unavailable (with the reason).
    fn descriptors(&self) -> Vec<SensorDesc>;
    /// Read current values. Sensors without a value must simply be left out.
    fn poll(&mut self, out: &mut Readings) -> PollOutcome;
    /// Do not poll more often than this (expensive sources such as WMI).
    fn min_interval_ms(&self) -> u64 {
        0
    }
}
