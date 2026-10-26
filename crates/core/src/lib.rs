//! FPSFramer core: everything that does not depend on the operating system.
//!
//! * [`sensors`] – sensor descriptors, readings and status rules
//! * [`frames`] – frame-time statistics (FPS, 1% / 0.1% lows)
//! * [`analysis`] – spikes, drops, bottleneck estimation, findings
//! * [`session`] – session recording and CSV export
//! * [`alerts`] – alert evaluation with anti-noise behaviour
//! * [`layouts`] – overlay layout format, validation and presets
//! * [`model`] / [`defaults`] – settings, profiles, scenes, defaults
//! * [`storage`] – atomic persistence, backups, session archive

pub mod alerts;
pub mod analysis;
pub mod defaults;
pub mod frames;
pub mod layouts;
pub mod model;
pub mod sensors;
pub mod session;
pub mod storage;
pub mod util;
