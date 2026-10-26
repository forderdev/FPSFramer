//! CPU, memory, disk, network and battery from Windows performance counters and APIs.

use super::pdh::Pdh;
use crate::counters::{is_physical_nic, parse_logical_cpu};
use crate::catalog;
use crate::provider::{PollOutcome, SensorProvider};
use fpsframer_core::sensors::{ids, Readings, ReadingsExt, SensorDesc, Unit};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

const SRC: &str = "windows";

pub struct WindowsBasic {
    pdh: Option<Pdh>,
    c_util: Option<usize>,
    c_freq: Option<usize>,
    c_perf: Option<usize>,
    c_disk_read: Option<usize>,
    c_disk_write: Option<usize>,
    c_disk_idle: Option<usize>,
    c_disk_lat: Option<usize>,
    c_net_rx: Option<usize>,
    c_net_tx: Option<usize>,
    cores: usize,
    has_battery: bool,
    cpu_name: Option<String>,
    descs: Vec<SensorDesc>,
    polls: u32,
}

fn has_battery() -> bool {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: valid out pointer.
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return false;
    }
    // 128 = no system battery, 255 = unknown
    s.BatteryFlag != 128 && s.BatteryFlag != 255
}

fn read_cpu_name() -> Option<String> {
    let k = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0").ok()?;
    let s: String = k.get_value("ProcessorNameString").ok()?;
    Some(s.split_whitespace().collect::<Vec<_>>().join(" "))
}

impl WindowsBasic {
    pub fn new() -> Self {
        let mut pdh = Pdh::new();
        let (mut c_util, mut c_freq, mut c_perf) = (None, None, None);
        let (mut c_dr, mut c_dw, mut c_di, mut c_dl, mut c_rx, mut c_tx) = (None, None, None, None, None, None);
        if let Some(p) = pdh.as_mut() {
            c_util = p.add("\\Processor Information(*)\\% Processor Utility");
            c_freq = p.add("\\Processor Information(*)\\Processor Frequency");
            c_perf = p.add("\\Processor Information(*)\\% Processor Performance");
            c_dr = p.add("\\PhysicalDisk(_Total)\\Disk Read Bytes/sec");
            c_dw = p.add("\\PhysicalDisk(_Total)\\Disk Write Bytes/sec");
            c_di = p.add("\\PhysicalDisk(*)\\% Idle Time");
            c_dl = p.add("\\PhysicalDisk(*)\\Avg. Disk sec/Transfer");
            c_rx = p.add("\\Network Interface(*)\\Bytes Received/sec");
            c_tx = p.add("\\Network Interface(*)\\Bytes Sent/sec");
            p.collect(); // baseline for the rate counters
        }
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let mut s = Self {
            pdh,
            c_util,
            c_freq,
            c_perf,
            c_disk_read: c_dr,
            c_disk_write: c_dw,
            c_disk_idle: c_di,
            c_disk_lat: c_dl,
            c_net_rx: c_rx,
            c_net_tx: c_tx,
            cores,
            has_battery: has_battery(),
            cpu_name: read_cpu_name(),
            descs: Vec::new(),
            polls: 0,
        };
        s.descs = s.build_descriptors();
        s
    }

    fn build_descriptors(&self) -> Vec<SensorDesc> {
        let mut v = Vec::new();
        if self.pdh.is_some() && self.c_util.is_some() {
            v.push(catalog::cpu_load(SRC));
            v.push(catalog::cpu_core_max(SRC));
            for i in 0..self.cores {
                v.push(catalog::cpu_core_load(i, SRC));
            }
        }
        if self.c_freq.is_some() && self.c_perf.is_some() {
            v.push(catalog::cpu_clock(SRC));
        }
        if self.cpu_name.is_some() {
            v.push(catalog::cpu_name(SRC));
        }
        v.push(catalog::cpu_temp_unavailable(SRC));
        v.extend(catalog::ram_set(SRC));
        v.extend(catalog::disk_set(SRC));
        v.extend(catalog::net_set(SRC));
        if self.has_battery {
            v.extend(catalog::battery_set(SRC));
        }
        v
    }
}

impl Default for WindowsBasic {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorProvider for WindowsBasic {
    fn id(&self) -> &'static str {
        "windows-basic"
    }
    fn label(&self) -> &'static str {
        "Windows performance counters"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }

    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        self.polls += 1;
        // ── memory (no sampling delay needed) ──
        let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        // SAFETY: dwLength is set as the API requires.
        if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() && m.ullTotalPhys > 0 {
            let total = m.ullTotalPhys as f64 / 1_048_576.0;
            let avail = m.ullAvailPhys as f64 / 1_048_576.0;
            out.put(ids::RAM_TOTAL, total, Unit::Megabytes);
            out.put(ids::RAM_AVAILABLE, avail, Unit::Megabytes);
            out.put(ids::RAM_USED, total - avail, Unit::Megabytes);
            out.put(ids::RAM_PERCENT, (total - avail) * 100.0 / total, Unit::Percent);
        }
        if let Some(n) = &self.cpu_name {
            out.put_text(ids::CPU_NAME, n.clone());
        }
        // ── battery ──
        if self.has_battery {
            let mut s = SYSTEM_POWER_STATUS::default();
            // SAFETY: valid out pointer.
            if unsafe { GetSystemPowerStatus(&mut s) }.is_ok() {
                if s.BatteryLifePercent != 255 {
                    out.put(ids::BATTERY_PERCENT, s.BatteryLifePercent as f64, Unit::Percent);
                }
                if s.ACLineStatus != 255 {
                    out.put("battery.discharging", if s.ACLineStatus == 0 { 1.0 } else { 0.0 }, Unit::Count);
                }
            }
        }
        // ── performance counters ──
        let Some(pdh) = self.pdh.as_mut() else {
            return PollOutcome::failed("Windows performance counters could not be opened.");
        };
        if !pdh.collect() {
            return PollOutcome::failed("Reading Windows performance counters failed.");
        }
        let pdh = &*pdh;

        if let Some(ci) = self.c_util {
            let mut core_max: Option<f64> = None;
            for (name, v) in pdh.array(ci) {
                let v = v.clamp(0.0, 100.0);
                if name == "_Total" {
                    out.put(ids::CPU_LOAD, v, Unit::Percent);
                } else if let Some(i) = parse_logical_cpu(&name) {
                    out.put(&format!("cpu.core.{i}.load"), v, Unit::Percent);
                    core_max = Some(core_max.map_or(v, |m: f64| m.max(v)));
                }
            }
            if let Some(m) = core_max {
                out.put(ids::CPU_CORE_MAX, m, Unit::Percent);
            }
        }
        if let (Some(cf), Some(cp)) = (self.c_freq, self.c_perf) {
            let perf = pdh.array(cp);
            let freq = pdh.array(cf);
            // Current speed = nominal frequency × performance percentage, averaged over logical CPUs.
            let (mut sum, mut n) = (0.0, 0u32);
            for (name, f) in &freq {
                if parse_logical_cpu(name).is_none() {
                    continue;
                }
                if let Some((_, p)) = perf.iter().find(|(pn, _)| pn == name) {
                    sum += f * p / 100.0;
                    n += 1;
                }
            }
            if n > 0 {
                out.put(ids::CPU_CLOCK, sum / n as f64, Unit::Mhz);
            }
        }
        if let Some(c) = self.c_disk_read {
            if let Some(v) = pdh.value(c) {
                out.put(ids::DISK_READ, v.max(0.0), Unit::BytesPerSec);
            }
        }
        if let Some(c) = self.c_disk_write {
            if let Some(v) = pdh.value(c) {
                out.put(ids::DISK_WRITE, v.max(0.0), Unit::BytesPerSec);
            }
        }
        if let Some(c) = self.c_disk_idle {
            // Busiest physical disk = lowest idle time.
            let busiest = pdh.array(c).into_iter().filter(|(n, _)| n != "_Total").map(|(_, v)| 100.0 - v.min(100.0)).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
            if let Some(b) = busiest {
                out.put(ids::DISK_ACTIVE, b.clamp(0.0, 100.0), Unit::Percent);
            }
        }
        if let Some(c) = self.c_disk_lat {
            let worst = pdh.array(c).into_iter().filter(|(n, _)| n != "_Total").map(|(_, v)| v * 1000.0).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
            if let Some(w) = worst {
                out.put(ids::DISK_LATENCY, w, Unit::Ms);
            }
        }
        for (c, id) in [(self.c_net_rx, ids::NET_DOWN), (self.c_net_tx, ids::NET_UP)] {
            if let Some(c) = c {
                let arr = pdh.array(c);
                if !arr.is_empty() {
                    let sum: f64 = arr.iter().filter(|(n, _)| is_physical_nic(n)).map(|(_, v)| v.max(0.0)).sum();
                    out.put(id, sum, Unit::BytesPerSec);
                }
            }
        }
        PollOutcome::ok()
    }
}
