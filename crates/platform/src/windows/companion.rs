//! Companion sensor sources on Windows: HWiNFO (shared memory) and
//! LibreHardwareMonitor / OpenHardwareMonitor (WMI).
//!
//! Both are optional. If neither program is running these providers report nothing
//! and the matching sensors stay "unavailable – needs a companion" with instructions.

use crate::companion::{ext_kind_from_lhm, hw_kind_from_lhm_identifier, map_ext, parse_hwinfo, ExtSensor, HwinfoError};
use crate::provider::{Health, PollOutcome, SensorProvider};
use fpsframer_core::sensors::{Readings, SensorDesc};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::w;
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery, FILE_MAP_READ, MEMORY_BASIC_INFORMATION,
};
use windows::Win32::Foundation::CloseHandle;
use wmi::{Variant, WMIConnection};

fn signature(descs: &[SensorDesc]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for d in descs {
        d.id.hash(&mut h);
        d.availability.available.hash(&mut h);
    }
    h.finish()
}

// ───────────────────────────── HWiNFO ─────────────────────────────

pub struct HwinfoProvider {
    descs: Vec<SensorDesc>,
    sig: u64,
    reported_unsupported: bool,
}

impl HwinfoProvider {
    pub fn new() -> Self {
        Self { descs: Vec::new(), sig: 0, reported_unsupported: false }
    }

    fn read_block() -> Option<Vec<u8>> {
        // SAFETY: opens the named mapping read-only, copies at most the mapped region, unmaps.
        unsafe {
            let h = OpenFileMappingW(FILE_MAP_READ.0, false, w!("Global\\HWiNFO_SENS_SM2")).ok()?;
            let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(h);
                return None;
            }
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            let q = VirtualQuery(Some(view.Value), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>());
            let region = if q > 0 { mbi.RegionSize } else { 0 };
            let mut out = None;
            if (44..=64 * 1024 * 1024).contains(&region) {
                out = Some(std::slice::from_raw_parts(view.Value as *const u8, region).to_vec());
            }
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(h);
            out
        }
    }
}

impl Default for HwinfoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorProvider for HwinfoProvider {
    fn id(&self) -> &'static str {
        "hwinfo"
    }
    fn label(&self) -> &'static str {
        "HWiNFO (shared memory)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }
    fn min_interval_ms(&self) -> u64 {
        1000
    }

    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let parsed = Self::read_block().map(|b| parse_hwinfo(&b));
        match parsed {
            Some(Ok(list)) => {
                let m = map_ext(&list, "HWiNFO");
                out.extend(m.readings);
                let sig = signature(&m.descs);
                let changed = sig != self.sig;
                self.sig = sig;
                self.descs = m.descs;
                self.reported_unsupported = false;
                PollOutcome { catalog_changed: changed, health: Health::Ok }
            }
            Some(Err(HwinfoError::Unsupported(msg))) => {
                let changed = !self.descs.is_empty();
                self.descs.clear();
                self.sig = 0;
                let first = !self.reported_unsupported;
                self.reported_unsupported = true;
                PollOutcome { catalog_changed: changed || first, health: Health::Degraded(msg) }
            }
            _ => {
                // Not running (or shared memory disabled / HWiNFO closed).
                let changed = !self.descs.is_empty();
                self.descs.clear();
                self.sig = 0;
                PollOutcome { catalog_changed: changed, health: Health::Ok }
            }
        }
    }
}

// ───────────────────── LibreHardwareMonitor / OpenHardwareMonitor ─────────────────────

#[derive(Default)]
struct LhmShared {
    sensors: Vec<ExtSensor>,
    running: bool,
}

pub struct LhmProvider {
    shared: Arc<Mutex<LhmShared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    descs: Vec<SensorDesc>,
    sig: u64,
}

fn as_string(v: Option<&Variant>) -> String {
    match v {
        Some(Variant::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn as_f64(v: Option<&Variant>) -> Option<f64> {
    match v {
        Some(Variant::R4(f)) => Some(*f as f64),
        Some(Variant::R8(f)) => Some(*f),
        Some(Variant::I4(i)) => Some(*i as f64),
        Some(Variant::UI4(i)) => Some(*i as f64),
        _ => None,
    }
}

fn query_once(con: &WMIConnection) -> Result<Vec<ExtSensor>, wmi::WMIError> {
    let hw: Vec<HashMap<String, Variant>> = con.raw_query("SELECT Identifier, Name FROM Hardware")?;
    let names: HashMap<String, String> = hw.iter().map(|r| (as_string(r.get("Identifier")), as_string(r.get("Name")))).collect();
    let rows: Vec<HashMap<String, Variant>> = con.raw_query("SELECT Identifier, Name, Parent, SensorType, Value FROM Sensor")?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let Some(value) = as_f64(r.get("Value")) else { continue };
        let parent = as_string(r.get("Parent"));
        let stype = as_string(r.get("SensorType"));
        out.push(ExtSensor {
            source: "lhm",
            hw: hw_kind_from_lhm_identifier(&parent),
            hw_name: names.get(&parent).cloned().unwrap_or_else(|| parent.clone()),
            name: as_string(r.get("Name")),
            kind: ext_kind_from_lhm(&stype),
            value,
            unit: String::new(),
        });
    }
    Ok(out)
}

fn worker(shared: Arc<Mutex<LhmShared>>, stop: Arc<AtomicBool>) {
    let mut con: Option<WMIConnection> = None;
    let mut next_connect = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if con.is_none() && std::time::Instant::now() >= next_connect {
            for ns in ["ROOT\\LibreHardwareMonitor", "ROOT\\OpenHardwareMonitor"] {
                if let Ok(c) = WMIConnection::with_namespace_path(ns) {
                    con = Some(c);
                    break;
                }
            }
            next_connect = std::time::Instant::now() + Duration::from_secs(10);
        }
        if let Some(c) = &con {
            match query_once(c) {
                Ok(list) if !list.is_empty() => {
                    let mut s = shared.lock();
                    s.sensors = list;
                    s.running = true;
                }
                _ => {
                    con = None; // program closed or namespace vanished: reconnect later
                    let mut s = shared.lock();
                    s.sensors.clear();
                    s.running = false;
                }
            }
        }
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl LhmProvider {
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(LhmShared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, st2) = (shared.clone(), stop.clone());
        let thread = std::thread::Builder::new().name("fpsframer-lhm".into()).spawn(move || worker(s2, st2)).ok();
        Self { shared, stop, thread, descs: Vec::new(), sig: 0 }
    }
}

impl Default for LhmProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LhmProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl SensorProvider for LhmProvider {
    fn id(&self) -> &'static str {
        "lhm"
    }
    fn label(&self) -> &'static str {
        "LibreHardwareMonitor / OpenHardwareMonitor (WMI)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }
    fn min_interval_ms(&self) -> u64 {
        1000
    }
    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let list = {
            let s = self.shared.lock();
            if s.running { s.sensors.clone() } else { Vec::new() }
        };
        if list.is_empty() {
            let changed = !self.descs.is_empty();
            self.descs.clear();
            self.sig = 0;
            return PollOutcome { catalog_changed: changed, health: Health::Ok };
        }
        let m = map_ext(&list, "LibreHardwareMonitor");
        out.extend(m.readings);
        let sig = signature(&m.descs);
        let changed = sig != self.sig;
        self.sig = sig;
        self.descs = m.descs;
        PollOutcome { catalog_changed: changed, health: Health::Ok }
    }
}
