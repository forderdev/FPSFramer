//! Companion sensor sources.
//!
//! Windows cannot expose CPU temperature, fan speeds or voltages without a
//! kernel driver, and FPSFramer deliberately installs none. Instead, if the user
//! already runs **HWiNFO** (with *Shared Memory Support* enabled) or
//! **LibreHardwareMonitor**, their readings are imported here.
//!
//! This module is platform-independent: it parses HWiNFO's shared-memory layout
//! from bytes and maps any external sensor list onto FPSFramer sensor ids.
//! Fetching the bytes / WMI rows is done in `windows::companion`.

use crate::provider::{Health, PollOutcome};
use fpsframer_core::sensors::{Category, Readings, ReadingsExt, SensorDesc, StatusRule, Unit};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtKind {
    Temperature,
    Fan,
    Voltage,
    Power,
    Clock,
    Load,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwKind {
    Cpu,
    Gpu,
    Mainboard,
    Memory,
    Storage,
    Network,
    Battery,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtSensor {
    pub source: &'static str,
    pub hw: HwKind,
    pub hw_name: String,
    pub name: String,
    pub kind: ExtKind,
    pub value: f64,
    /// Unit string as reported by the source (`"RPM"`, `"%"`, `"MHz"`…).
    pub unit: String,
}

// ───────────────────────────── HWiNFO shared memory ─────────────────────────────

pub const HWINFO_SIGNATURE: u32 = 0x5369_5748; // "HWiS"
const HEADER_LEN: usize = 44; // see parse_hwinfo
const READING_LEN: usize = 316; // 4+4+4+128+128+16+4*8, packed
const SENSOR_LEN_MIN: usize = 264; // 4+4+128+128

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn f64_at(b: &[u8], o: usize) -> Option<f64> {
    b.get(o..o + 8).map(|x| f64::from_le_bytes([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7]]))
}
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    // HWiNFO writes ANSI (Windows-1252); degrees sign 0xB0 is the common non-ASCII char.
    b[..end].iter().map(|&c| c as char).collect()
}

#[derive(Debug, PartialEq)]
pub enum HwinfoError {
    TooShort,
    NotActive,
    Unsupported(String),
}

/// Parse the `Global\HWiNFO_SENS_SM2` shared memory block.
///
/// Layout (little endian, packed): header { u32 signature, u32 version, u32 revision,
/// i64 poll_time, u32 sensor_offset, u32 sensor_size, u32 sensor_count,
/// u32 reading_offset, u32 reading_size, u32 reading_count } followed by
/// sensor elements { u32 id, u32 instance, char[128] name_orig, char[128] name_user }
/// and reading elements { u32 type, u32 sensor_index, u32 reading_id, char[128] label_orig,
/// char[128] label_user, char[16] unit, f64 value, f64 min, f64 max, f64 avg }.
pub fn parse_hwinfo(buf: &[u8]) -> Result<Vec<ExtSensor>, HwinfoError> {
    if buf.len() < HEADER_LEN {
        return Err(HwinfoError::TooShort);
    }
    if u32_at(buf, 0) != Some(HWINFO_SIGNATURE) {
        return Err(HwinfoError::NotActive);
    }
    let s_off = u32_at(buf, 20).ok_or(HwinfoError::TooShort)? as usize;
    let s_size = u32_at(buf, 24).ok_or(HwinfoError::TooShort)? as usize;
    let s_cnt = u32_at(buf, 28).ok_or(HwinfoError::TooShort)? as usize;
    let r_off = u32_at(buf, 32).ok_or(HwinfoError::TooShort)? as usize;
    let r_size = u32_at(buf, 36).ok_or(HwinfoError::TooShort)? as usize;
    let r_cnt = u32_at(buf, 40).ok_or(HwinfoError::TooShort)? as usize;
    // If HWiNFO ever changes the element size our field offsets would be wrong: refuse rather than guess.
    if r_size != READING_LEN || s_size < SENSOR_LEN_MIN {
        return Err(HwinfoError::Unsupported(format!(
            "HWiNFO reports an element size ({r_size}/{s_size}) that this version of FPSFramer does not understand."
        )));
    }
    if s_cnt > 4096 || r_cnt > 20_000 {
        return Err(HwinfoError::Unsupported("HWiNFO reported an implausible number of sensors.".into()));
    }
    let mut sensors: Vec<(String, HwKind)> = Vec::with_capacity(s_cnt);
    for i in 0..s_cnt {
        let o = s_off + i * s_size;
        let name = buf.get(o + 8 + 128..o + 8 + 256).map(cstr).filter(|s| !s.is_empty()).or_else(|| buf.get(o + 8..o + 8 + 128).map(cstr)).ok_or(HwinfoError::TooShort)?;
        let kind = hw_kind_from_name(&name);
        sensors.push((name, kind));
    }
    let mut out = Vec::with_capacity(r_cnt);
    for i in 0..r_cnt {
        let o = r_off + i * r_size;
        let Some(t) = u32_at(buf, o) else { return Err(HwinfoError::TooShort) };
        let idx = u32_at(buf, o + 4).ok_or(HwinfoError::TooShort)? as usize;
        let label_user = buf.get(o + 12 + 128..o + 12 + 256).map(cstr).unwrap_or_default();
        let label = if label_user.is_empty() { buf.get(o + 12..o + 12 + 128).map(cstr).unwrap_or_default() } else { label_user };
        let unit = buf.get(o + 268..o + 284).map(cstr).unwrap_or_default();
        let value = f64_at(buf, o + 284).ok_or(HwinfoError::TooShort)?;
        let kind = match t {
            1 => ExtKind::Temperature,
            2 => ExtKind::Voltage,
            3 => ExtKind::Fan,
            4 | 8 => ExtKind::Other,
            5 => ExtKind::Power,
            6 => ExtKind::Clock,
            7 => ExtKind::Load,
            _ => ExtKind::Other,
        };
        if !value.is_finite() {
            continue;
        }
        let (hw_name, hw) = sensors.get(idx).cloned().unwrap_or((String::new(), HwKind::Other));
        out.push(ExtSensor { source: "hwinfo", hw, hw_name, name: label, kind, value, unit });
    }
    Ok(out)
}

fn hw_kind_from_name(n: &str) -> HwKind {
    let l = n.to_lowercase();
    if l.starts_with("cpu") {
        HwKind::Cpu
    } else if l.starts_with("gpu") {
        HwKind::Gpu
    } else if l.starts_with("drive") || l.starts_with("s.m.a.r.t") || l.contains("smart") {
        HwKind::Storage
    } else if l.starts_with("network") {
        HwKind::Network
    } else if l.starts_with("memory") || l.starts_with("dimm") {
        HwKind::Memory
    } else if l.starts_with("battery") {
        HwKind::Battery
    } else if l.starts_with("system") || l.contains("motherboard") || l.contains("embedded controller") {
        HwKind::Mainboard
    } else {
        HwKind::Other
    }
}

/// Hardware kind from a LibreHardwareMonitor identifier such as `/intelcpu/0` or `/gpu-nvidia/0`.
pub fn hw_kind_from_lhm_identifier(id: &str) -> HwKind {
    let l = id.to_lowercase();
    if l.contains("cpu") {
        HwKind::Cpu
    } else if l.contains("gpu") {
        HwKind::Gpu
    } else if l.contains("lpc") || l.contains("mainboard") || l.contains("motherboard") || l.contains("superio") || l.contains("embeddedcontroller") {
        HwKind::Mainboard
    } else if l.starts_with("/ram") || l.contains("memory") {
        HwKind::Memory
    } else if l.contains("nvme") || l.contains("hdd") || l.contains("ssd") || l.contains("storage") {
        HwKind::Storage
    } else if l.contains("nic") || l.contains("network") {
        HwKind::Network
    } else if l.contains("battery") {
        HwKind::Battery
    } else {
        HwKind::Other
    }
}

pub fn ext_kind_from_lhm(sensor_type: &str) -> ExtKind {
    match sensor_type {
        "Temperature" => ExtKind::Temperature,
        "Fan" => ExtKind::Fan,
        "Voltage" => ExtKind::Voltage,
        "Power" => ExtKind::Power,
        "Clock" => ExtKind::Clock,
        "Load" => ExtKind::Load,
        _ => ExtKind::Other,
    }
}

// ───────────────────────────── mapping ─────────────────────────────

/// Which FPSFramer sensor an external reading can stand in for, and how good a match it is (lower = better).
pub fn alias_for(s: &ExtSensor) -> Option<(&'static str, u8)> {
    let n = s.name.to_lowercase();
    match (s.hw, s.kind) {
        (HwKind::Cpu, ExtKind::Temperature) => {
            if n.contains("cpu package") || n.contains("tctl/tdie") || n.contains("tdie") {
                Some(("cpu.temp", 1))
            } else if n == "package" || n.contains("package") {
                Some(("cpu.temp", 2))
            } else if n.contains("core max") {
                Some(("cpu.temp", 3))
            } else if n == "cpu" || n.contains("cpu (tctl") || n == "cpu die" || n == "cpu die (average)" {
                Some(("cpu.temp", 4))
            } else {
                None
            }
        }
        (HwKind::Cpu, ExtKind::Power) => {
            if n.contains("cpu package power") || n == "package" || n == "cpu package" {
                Some(("cpu.power", 1))
            } else if n.contains("package") {
                Some(("cpu.power", 2))
            } else {
                None
            }
        }
        (HwKind::Gpu, ExtKind::Temperature) => {
            if n.contains("hot spot") || n.contains("hotspot") {
                Some(("gpu.hotspot", 1))
            } else if n.contains("gpu temperature") {
                Some(("gpu.temp", 1))
            } else if n == "gpu core" || n == "gpu" {
                Some(("gpu.temp", 2))
            } else {
                None
            }
        }
        (HwKind::Gpu, ExtKind::Power) => {
            if n.contains("gpu power") || n == "gpu package" || n.contains("gpu core power") {
                Some(("gpu.power", 1))
            } else {
                None
            }
        }
        (HwKind::Gpu, ExtKind::Clock) => {
            if n.contains("memory") {
                Some(("gpu.memclock", 1))
            } else if n == "gpu clock" || n == "gpu core" || n.contains("gpu core clock") {
                Some(("gpu.clock", 1))
            } else {
                None
            }
        }
        (HwKind::Gpu, ExtKind::Load) => {
            if n == "gpu core load" || n == "gpu core" || n.contains("gpu utilization") {
                Some(("gpu.load", 1))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn category_and_unit(s: &ExtSensor) -> (Category, Unit) {
    match s.kind {
        ExtKind::Temperature => (Category::Thermal, Unit::Celsius),
        ExtKind::Fan => (Category::Fan, if s.unit.contains('%') { Unit::Percent } else { Unit::Rpm }),
        ExtKind::Voltage => (Category::Voltage, Unit::Volt),
        ExtKind::Power => (Category::Power, Unit::Watt),
        ExtKind::Clock => (if s.hw == HwKind::Gpu { Category::Gpu } else { Category::Cpu }, Unit::Mhz),
        ExtKind::Load => (
            match s.hw {
                HwKind::Cpu => Category::Cpu,
                HwKind::Gpu => Category::Gpu,
                HwKind::Memory => Category::Memory,
                HwKind::Storage => Category::Storage,
                HwKind::Network => Category::Network,
                _ => Category::Other,
            },
            Unit::Percent,
        ),
        ExtKind::Other => (Category::Other, Unit::Count),
    }
}

pub fn alias_unit(id: &str) -> Unit {
    match id {
        "cpu.temp" | "gpu.temp" | "gpu.hotspot" => Unit::Celsius,
        "cpu.power" | "gpu.power" => Unit::Watt,
        "gpu.clock" | "gpu.memclock" => Unit::Mhz,
        "gpu.load" => Unit::Percent,
        _ => Unit::Count,
    }
}

pub struct Mapped {
    pub descs: Vec<SensorDesc>,
    pub readings: Readings,
}

const MAX_GENERIC: usize = 150;

/// Map a list of external sensors to FPSFramer descriptors and readings.
pub fn map_ext(list: &[ExtSensor], source_label: &str) -> Mapped {
    let mut descs: Vec<SensorDesc> = Vec::new();
    let mut readings = Readings::new();
    let mut used_ids: HashMap<String, u32> = HashMap::new();

    // Best alias candidate per target id. Only the first GPU group is aliased.
    let first_gpu = list.iter().find(|s| s.hw == HwKind::Gpu).map(|s| s.hw_name.clone());
    let mut best: BTreeMap<&'static str, (u8, &ExtSensor)> = BTreeMap::new();
    for s in list {
        if s.hw == HwKind::Gpu && Some(&s.hw_name) != first_gpu.as_ref() {
            continue;
        }
        if let Some((id, pri)) = alias_for(s) {
            if best.get(id).is_none_or(|(p, _)| pri < *p) {
                best.insert(id, (pri, s));
            }
        }
    }
    for (id, (_, s)) in &best {
        let unit = alias_unit(id);
        let base = match *id {
            "cpu.temp" => crate::catalog::cpu_temp(source_label),
            "cpu.power" => crate::catalog::cpu_power(source_label),
            "gpu.hotspot" => SensorDesc::new("gpu.hotspot", "GPU hot-spot temperature", Category::Thermal, Unit::Celsius)
                .short("GPU hotspot")
                .desc("The hottest point on the graphics chip. It runs hotter than the main GPU temperature.")
                .rule(StatusRule::higher(90.0, 105.0))
                .rec()
                .source(source_label),
            other => {
                let f = other.strip_prefix("gpu.").unwrap_or(other);
                match crate::catalog::GPU_FIELDS.iter().find(|g| g.field == f) {
                    Some(g) => crate::catalog::gpu_desc(0, &s.hw_name, false, g, source_label),
                    None => continue,
                }
            }
        };
        descs.push(base);
        readings.put(id, s.value, unit);
        used_ids.insert((*id).to_string(), 1);
    }

    // Everything else, as searchable generic sensors.
    let mut generic = 0usize;
    for s in list {
        if generic >= MAX_GENERIC {
            break;
        }
        if matches!(s.kind, ExtKind::Other) {
            continue;
        }
        let (cat, unit) = category_and_unit(s);
        let base = format!("ext.{}.{}.{}", s.source, fpsframer_core::util::slug(&s.hw_name), fpsframer_core::util::slug(&s.name));
        let n = used_ids.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = if *n > 1 { format!("{base}-{n}") } else { base };
        let mut d = SensorDesc::new(id.clone(), s.name.clone(), cat, unit)
            .desc(format!("Reported by {source_label} for {}.", if s.hw_name.is_empty() { "your hardware" } else { &s.hw_name }))
            .source(source_label)
            .kw(&["companion", "external", source_label]);
        if !s.hw_name.is_empty() {
            d = d.group(s.hw_name.clone());
        }
        if unit == Unit::Celsius {
            d = d.rule(StatusRule::higher(80.0, 95.0));
        }
        descs.push(d);
        readings.put(&id, s.value, unit);
        generic += 1;
    }
    Mapped { descs, readings }
}

/// Shared by the Windows companion provider.
pub fn outcome(changed: bool, health: Health) -> PollOutcome {
    PollOutcome { catalog_changed: changed, health }
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Build a synthetic HWiNFO shared-memory block.
    pub fn hwinfo_block(sensors: &[&str], readings: &[(u32, u32, &str, &str, f64)]) -> Vec<u8> {
        let s_size = 264usize;
        let r_size = 316usize;
        let s_off = 44usize;
        let r_off = s_off + s_size * sensors.len();
        let mut b = vec![0u8; r_off + r_size * readings.len()];
        b[0..4].copy_from_slice(&0x5369_5748u32.to_le_bytes());
        b[4..8].copy_from_slice(&2u32.to_le_bytes());
        b[20..24].copy_from_slice(&(s_off as u32).to_le_bytes());
        b[24..28].copy_from_slice(&(s_size as u32).to_le_bytes());
        b[28..32].copy_from_slice(&(sensors.len() as u32).to_le_bytes());
        b[32..36].copy_from_slice(&(r_off as u32).to_le_bytes());
        b[36..40].copy_from_slice(&(r_size as u32).to_le_bytes());
        b[40..44].copy_from_slice(&(readings.len() as u32).to_le_bytes());
        for (i, n) in sensors.iter().enumerate() {
            let o = s_off + i * s_size;
            b[o + 8..o + 8 + n.len()].copy_from_slice(n.as_bytes());
        }
        for (i, (t, idx, label, unit, v)) in readings.iter().enumerate() {
            let o = r_off + i * r_size;
            b[o..o + 4].copy_from_slice(&t.to_le_bytes());
            b[o + 4..o + 8].copy_from_slice(&idx.to_le_bytes());
            b[o + 12..o + 12 + label.len()].copy_from_slice(label.as_bytes());
            b[o + 268..o + 268 + unit.len()].copy_from_slice(unit.as_bytes());
            b[o + 284..o + 292].copy_from_slice(&v.to_le_bytes());
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::hwinfo_block;
    use super::*;
    use fpsframer_core::sensors::ReadingsExt;

    #[test]
    fn parses_a_synthetic_hwinfo_block() {
        let b = hwinfo_block(
            &["CPU [#0]: Intel Core i7-12700K: DTS", "GPU [#0]: NVIDIA GeForce RTX 4070: ", "System: ROG STRIX Z690"],
            &[
                (1, 0, "CPU Package", "°C", 61.0),
                (5, 0, "CPU Package Power", "W", 74.5),
                (1, 1, "GPU Hot Spot Temperature", "°C", 72.0),
                (3, 2, "CPU Fan", "RPM", 1180.0),
                (2, 2, "+12V", "V", 12.096),
            ],
        );
        let v = parse_hwinfo(&b).unwrap();
        assert_eq!(v.len(), 5);
        assert_eq!(v[0].hw, HwKind::Cpu);
        assert_eq!(v[0].kind, ExtKind::Temperature);
        assert_eq!(v[2].hw, HwKind::Gpu);
        assert_eq!(v[3].hw, HwKind::Mainboard);
        assert_eq!(v[3].kind, ExtKind::Fan);
        assert_eq!(v[4].kind, ExtKind::Voltage);
    }

    #[test]
    fn closed_or_foreign_blocks_are_rejected() {
        let mut b = hwinfo_block(&["CPU"], &[]);
        b[0..4].copy_from_slice(&0x4441_4544u32.to_le_bytes()); // 'DEAD': HWiNFO closed
        assert_eq!(parse_hwinfo(&b), Err(HwinfoError::NotActive));
        assert_eq!(parse_hwinfo(&[0u8; 8]), Err(HwinfoError::TooShort));
    }

    #[test]
    fn unknown_element_size_is_refused_instead_of_misread() {
        let mut b = hwinfo_block(&["CPU"], &[(1, 0, "CPU Package", "°C", 50.0)]);
        b[36..40].copy_from_slice(&400u32.to_le_bytes());
        assert!(matches!(parse_hwinfo(&b), Err(HwinfoError::Unsupported(_))));
    }

    #[test]
    fn truncated_block_does_not_panic() {
        let b = hwinfo_block(&["CPU"], &[(1, 0, "CPU Package", "°C", 50.0), (1, 0, "Core Max", "°C", 55.0)]);
        for cut in [10usize, 60, 300, b.len() - 5] {
            let _ = parse_hwinfo(&b[..cut]); // must return Err, never panic
        }
    }

    #[test]
    fn cpu_package_temperature_becomes_cpu_temp_and_outranks_core_max() {
        let list = vec![
            ExtSensor { source: "hwinfo", hw: HwKind::Cpu, hw_name: "CPU".into(), name: "Core Max".into(), kind: ExtKind::Temperature, value: 70.0, unit: "°C".into() },
            ExtSensor { source: "hwinfo", hw: HwKind::Cpu, hw_name: "CPU".into(), name: "CPU Package".into(), kind: ExtKind::Temperature, value: 64.0, unit: "°C".into() },
            ExtSensor { source: "hwinfo", hw: HwKind::Cpu, hw_name: "CPU".into(), name: "Core #3".into(), kind: ExtKind::Temperature, value: 66.0, unit: "°C".into() },
        ];
        let m = map_ext(&list, "HWiNFO");
        assert_eq!(m.readings.num("cpu.temp"), Some(64.0));
        assert!(m.descs.iter().any(|d| d.id == "cpu.temp" && d.availability.available));
        assert!(m.descs.iter().any(|d| d.name == "Core #3"), "per-core temp is kept as a generic sensor");
    }

    #[test]
    fn only_first_gpu_is_aliased_and_generic_ids_are_unique() {
        let mk = |hw: &str, name: &str, v: f64| ExtSensor { source: "lhm", hw: HwKind::Gpu, hw_name: hw.into(), name: name.into(), kind: ExtKind::Temperature, value: v, unit: "°C".into() };
        let list = vec![mk("GPU A", "GPU Core", 60.0), mk("GPU B", "GPU Core", 40.0), mk("GPU A", "GPU Core", 61.0)];
        let m = map_ext(&list, "LibreHardwareMonitor");
        assert_eq!(m.readings.num("gpu.temp"), Some(60.0));
        let mut ids: Vec<_> = m.descs.iter().map(|d| d.id.clone()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate sensor ids");
    }

    #[test]
    fn lhm_identifier_mapping() {
        assert_eq!(hw_kind_from_lhm_identifier("/intelcpu/0"), HwKind::Cpu);
        assert_eq!(hw_kind_from_lhm_identifier("/gpu-nvidia/0"), HwKind::Gpu);
        assert_eq!(hw_kind_from_lhm_identifier("/lpc/nct6798d"), HwKind::Mainboard);
        assert_eq!(hw_kind_from_lhm_identifier("/nvme/0"), HwKind::Storage);
        assert_eq!(ext_kind_from_lhm("Fan"), ExtKind::Fan);
        assert_eq!(ext_kind_from_lhm("Throughput"), ExtKind::Other);
    }
}
