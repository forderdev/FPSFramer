//! FPSFramer platform layer: everything that talks to the operating system.
//!
//! * [`provider`] – the `SensorProvider` contract and [`catalog`] – sensor wording
//! * [`linux`], [`gpu`], [`nvml`] – hardware sensor providers
//! * [`companion`] – HWiNFO / LibreHardwareMonitor bridge (parsing + mapping)
//! * [`capture`] – frame sources (ETW on Windows)
//! * [`proc`] – processes and the foreground window
//! * [`demo`] – clearly-labelled simulated data
//! * [`hardware`], [`screenshot`], [`system`] – hardware info, screen capture, OS helpers

pub mod capture;
pub mod catalog;
pub mod clock;
pub mod companion;
pub mod counters;
pub mod demo;
pub mod etw_parse;
pub mod gpu;
pub mod hardware;
pub mod linux;
pub mod nvml;
pub mod overlay_window;
pub mod proc;
pub mod provider;
pub mod screenshot;
pub mod system;

#[cfg(windows)]
pub mod windows;

/// Providers that make sense on this platform, in priority order
/// (when two provide the same sensor id, the earlier one wins).
pub fn default_providers(companions: fpsframer_core::model::Companions, primary_gpu: Option<usize>) -> Vec<Box<dyn provider::SensorProvider>> {
    #[cfg(windows)]
    {
        windows::default_providers(companions, primary_gpu)
    }
    #[cfg(not(windows))]
    {
        let _ = (companions, primary_gpu);
        let mut v: Vec<Box<dyn provider::SensorProvider>> = vec![Box::new(linux::LinuxBasic::new())];
        let nv = gpu::NvmlProvider::new();
        if nv.available() {
            v.push(Box::new(nv));
        }
        v
    }
}

/// A smoke test against the *real* machine it runs on (Linux `/proc` here, Windows performance
/// counters on a Windows runner). It checks the contract rather than particular values: numbers
/// are finite and in range, missing sensors are absent rather than zero, and ids don't collide.
#[cfg(test)]
mod real_machine_smoke {
    use super::*;
    use fpsframer_core::sensors::{Readings, ReadingsExt};
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    #[test]
    fn real_providers_report_sane_readings_and_never_invent_fps() {
        let mut providers = default_providers(Default::default(), None);
        assert!(!providers.is_empty(), "this platform has no sensor provider at all");

        // Two providers may describe the same sensor (the earlier one wins), but one provider must not repeat itself.
        for p in &providers {
            let mut seen = HashSet::new();
            for d in p.descriptors() {
                assert!(seen.insert(d.id.clone()), "provider {} describes sensor {} twice", p.id(), d.id);
                if !d.availability.available {
                    assert!(d.availability.reason.is_some(), "{} is unavailable without a reason", d.id);
                }
            }
        }

        let mut readings = Readings::new();
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline && readings.num("cpu.load").is_none() {
            for p in providers.iter_mut() {
                p.poll(&mut readings);
            }
            std::thread::sleep(Duration::from_millis(600));
        }

        let cpu = readings.num("cpu.load").expect("no CPU load reading after 12 s");
        assert!((0.0..=100.5).contains(&cpu), "CPU load out of range: {cpu}");
        if let Some(total) = readings.num("ram.total") {
            assert!(total > 100.0, "implausible RAM total: {total} MB");
        }
        if let Some(pct) = readings.num("ram.percent") {
            assert!((0.0..=100.0).contains(&pct), "RAM percent out of range: {pct}");
        }
        for (id, v) in &readings {
            if let Some(n) = v.as_f64() {
                assert!(n.is_finite(), "{id} is not a finite number");
            }
        }
        assert!(readings.num("fps").is_none(), "FPS must not come from a hardware provider: without a game it is unavailable, not zero");
        if let Some(t) = readings.num("cpu.temp") {
            assert!(t > 0.0 && t < 130.0, "implausible CPU temperature: {t}");
        }
    }
}
