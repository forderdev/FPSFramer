//! GPU helpers shared by the platform providers.

use crate::catalog;
use crate::nvml::{self, Nvml, NvmlDevice, NvmlSample};
use crate::provider::{Health, PollOutcome, SensorProvider};
use fpsframer_core::sensors::{ids, Readings, SensorDesc, UnavailableCode};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    pub fn from_pci(id: u32) -> Vendor {
        match id {
            0x10DE => Vendor::Nvidia,
            0x1002 | 0x1022 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            _ => Vendor::Other,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Vendor::Nvidia => "NVIDIA",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
            Vendor::Other => "this",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuSlot {
    pub name: String,
    pub vendor: Vendor,
    pub vram_total_mb: Option<f64>,
    /// Locally unique id Windows uses in performance-counter instance names.
    pub luid: Option<(u32, u32)>,
    /// Other adapter ids that turned out to be the same physical card (see [`merge_phantom_nvidia`]).
    /// Their counters are folded into this card.
    pub aliases: Vec<(u32, u32)>,
}

impl GpuSlot {
    /// Discrete cards (with their own memory) rank above integrated graphics.
    pub fn is_discrete(&self) -> bool {
        match self.vendor {
            Vendor::Nvidia => true,
            _ => self.vram_total_mb.is_some_and(|v| v >= 1536.0) && !self.name.to_lowercase().contains("integrated"),
        }
    }
}

/// Windows can list one NVIDIA card more than once (same name, different adapter id). The NVIDIA driver, by
/// contrast, knows how many physical devices there are. Same-name NVIDIA adapters beyond that number are
/// folded into the first one as aliases, so one card is one GPU in the app (and two identical cards in one PC
/// are still two, because NVML then reports two devices).
pub fn merge_phantom_nvidia(slots: Vec<GpuSlot>, nvml_names: &[String]) -> Vec<GpuSlot> {
    let mut out: Vec<GpuSlot> = Vec::new();
    for slot in slots {
        if slot.vendor == Vendor::Nvidia {
            let real = nvml_names.iter().filter(|n| n.eq_ignore_ascii_case(&slot.name)).count();
            let kept = out.iter().filter(|s| s.vendor == Vendor::Nvidia && s.name.eq_ignore_ascii_case(&slot.name)).count();
            if real > 0 && kept >= real {
                if let Some(first) = out.iter_mut().find(|s| s.vendor == Vendor::Nvidia && s.name.eq_ignore_ascii_case(&slot.name)) {
                    if let Some(l) = slot.luid {
                        first.aliases.push(l);
                    }
                    first.aliases.extend(slot.aliases);
                    if slot.vram_total_mb.unwrap_or(0.0) > first.vram_total_mb.unwrap_or(0.0) {
                        first.vram_total_mb = slot.vram_total_mb;
                    }
                    continue;
                }
            }
        }
        out.push(slot);
    }
    out
}

/// Most capable GPU first (so `gpu.*` is the one that actually runs games).
/// `primary` moves a chosen index (into the *unsorted* list) to the front.
pub fn order_slots(mut slots: Vec<GpuSlot>, primary: Option<usize>) -> Vec<GpuSlot> {
    let chosen = primary.and_then(|i| slots.get(i).map(|s| s.name.clone()));
    slots.sort_by(|a, b| {
        b.is_discrete()
            .cmp(&a.is_discrete())
            .then(b.vram_total_mb.unwrap_or(0.0).total_cmp(&a.vram_total_mb.unwrap_or(0.0)))
    });
    if let Some(name) = chosen {
        if let Some(pos) = slots.iter().position(|s| s.name == name) {
            let s = slots.remove(pos);
            slots.insert(0, s);
        }
    }
    slots
}

// ───────────────── Windows GPU performance-counter helpers (pure) ─────────────────

/// `pid_1234_luid_0x00000000_0x0000F2C1_phys_0_eng_3_engtype_3D`
/// → `((0, 0xF2C1), "phys_0_eng_3")`.
pub fn parse_engine_instance(name: &str) -> Option<((u32, u32), String)> {
    let luid = parse_luid(name)?;
    let rest = name.split("_phys_").nth(1)?;
    let eng_idx = rest.find("_engtype_")?;
    Some((luid, format!("phys_{}", &rest[..eng_idx])))
}

/// Extract `luid_0xHHHHHHHH_0xLLLLLLLL`.
pub fn parse_luid(name: &str) -> Option<(u32, u32)> {
    let s = name.split("luid_0x").nth(1)?;
    let mut parts = s.split('_');
    let hi = parts.next()?;
    let lo = parts.next()?.trim_start_matches("0x");
    let lo: String = lo.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    Some((u32::from_str_radix(hi, 16).ok()?, u32::from_str_radix(&lo, 16).ok()?))
}

/// Combine per-process engine utilisation into one number per GPU:
/// processes sharing an engine add up (capped at 100), and the GPU's load is
/// its busiest engine – the same rule Task Manager uses.
pub fn aggregate_engine_load(instances: &[(String, f64)]) -> HashMap<(u32, u32), f64> {
    let mut per_engine: HashMap<((u32, u32), String), f64> = HashMap::new();
    for (name, v) in instances {
        if let Some((luid, eng)) = parse_engine_instance(name) {
            *per_engine.entry((luid, eng)).or_default() += v.max(0.0);
        }
    }
    let mut out: HashMap<(u32, u32), f64> = HashMap::new();
    for ((luid, _), v) in per_engine {
        let e = out.entry(luid).or_default();
        *e = e.max(v.min(100.0));
    }
    out
}

// ───────────────────────── NVML-only provider (Linux / fallback) ─────────────────────────

pub struct NvmlProvider {
    nvml: Option<Nvml>,
    devices: Vec<NvmlDevice>,
    descs: Vec<SensorDesc>,
    error: Option<String>,
    probe: Vec<NvmlSample>,
}

impl NvmlProvider {
    pub fn new() -> Self {
        match Nvml::load() {
            Ok(n) => {
                let devices = n.devices();
                let probe: Vec<NvmlSample> = devices.iter().map(|d| n.sample(d)).collect();
                let multi = devices.len() > 1;
                let mut descs = Vec::new();
                for (i, d) in devices.iter().enumerate() {
                    let p = &probe[i];
                    for f in catalog::GPU_FIELDS {
                        let ok = match f.field {
                            "load" => p.util_pct.is_some(),
                            "temp" => p.temp_c.is_some(),
                            "clock" => p.clock_gfx.is_some(),
                            "memclock" => p.clock_mem.is_some(),
                            "power" => p.power_w.is_some(),
                            "fan" => p.fan_pct.is_some(),
                            "vram.used" | "vram.total" | "vram.percent" => p.vram_used_mb.is_some(),
                            "throttle.thermal" | "throttle.power" => p.throttle_thermal.is_some(),
                            _ => false,
                        };
                        let mut desc = catalog::gpu_desc(i, &d.name, multi, f, "nvml");
                        if !ok {
                            desc = desc.unavailable(
                                UnavailableCode::NotSupported,
                                "This graphics card or driver does not report this value.",
                                None,
                            );
                        }
                        descs.push(desc);
                    }
                    descs.push(catalog::gpu_text_desc(i, &d.name, multi, "name", "model", "The name of your graphics card.", "nvml"));
                    if p.throttle_thermal.is_some() {
                        descs.push(catalog::gpu_text_desc(i, &d.name, multi, "throttle", "slowdown reason", "Whether the graphics driver is currently slowing the card down, and why.", "nvml"));
                    }
                }
                NvmlProvider { nvml: Some(n), devices, descs, error: None, probe }
            }
            Err(e) => NvmlProvider { nvml: None, devices: vec![], descs: vec![], error: Some(e), probe: vec![] },
        }
    }

    pub fn available(&self) -> bool {
        self.nvml.is_some() && !self.devices.is_empty()
    }
    pub fn load_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn device_names(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.name.clone()).collect()
    }
    pub fn driver_version(&self) -> Option<String> {
        self.nvml.as_ref().and_then(|n| n.driver_version())
    }
}

impl Default for NvmlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorProvider for NvmlProvider {
    fn id(&self) -> &'static str {
        "nvml"
    }
    fn label(&self) -> &'static str {
        "NVIDIA driver (NVML)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }
    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let Some(n) = &self.nvml else { return PollOutcome::ok() };
        let _ = &self.probe;
        for (i, d) in self.devices.iter().enumerate() {
            let s = n.sample(d);
            let prefix = catalog::gpu_prefix(i);
            nvml::write_sample(out, &prefix, &s);
            out.insert(format!("{prefix}.name"), fpsframer_core::sensors::Reading::Text(d.name.clone()));
        }
        let _ = ids::GPU_LOAD;
        PollOutcome { catalog_changed: false, health: Health::Ok }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv(name: &str, luid: u32, vram: f64) -> GpuSlot {
        GpuSlot { name: name.into(), vendor: Vendor::Nvidia, vram_total_mb: Some(vram), luid: Some((0, luid)), aliases: vec![] }
    }

    #[test]
    fn one_card_listed_twice_becomes_one_gpu() {
        let slots = vec![nv("NVIDIA GeForce RTX 3060 Ti", 1, 8192.0), nv("NVIDIA GeForce RTX 3060 Ti", 2, 8192.0)];
        let out = merge_phantom_nvidia(slots, &["NVIDIA GeForce RTX 3060 Ti".to_string()]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].luid, Some((0, 1)));
        assert_eq!(out[0].aliases, vec![(0, 2)], "the second id's counters still count towards the card");
    }

    #[test]
    fn two_real_identical_cards_stay_two() {
        let slots = vec![nv("NVIDIA GeForce RTX 3060 Ti", 1, 8192.0), nv("NVIDIA GeForce RTX 3060 Ti", 2, 8192.0)];
        let names = vec!["NVIDIA GeForce RTX 3060 Ti".to_string(); 2];
        assert_eq!(merge_phantom_nvidia(slots, &names).len(), 2);
    }

    #[test]
    fn without_the_nvidia_driver_nothing_is_merged_or_dropped() {
        let slots = vec![nv("NVIDIA GeForce RTX 3060 Ti", 1, 8192.0), nv("NVIDIA GeForce RTX 3060 Ti", 2, 8192.0)];
        assert_eq!(merge_phantom_nvidia(slots, &[]).len(), 2);
    }

    #[test]
    fn other_vendors_and_different_cards_are_untouched() {
        let intel = GpuSlot { name: "Intel UHD Graphics 730".into(), vendor: Vendor::Intel, vram_total_mb: Some(128.0), luid: Some((0, 9)), aliases: vec![] };
        let slots = vec![intel, nv("NVIDIA GeForce RTX 3060 Ti", 1, 8192.0), nv("NVIDIA GeForce GTX 1050", 3, 2048.0)];
        let names = vec!["NVIDIA GeForce RTX 3060 Ti".to_string(), "NVIDIA GeForce GTX 1050".to_string()];
        assert_eq!(merge_phantom_nvidia(slots, &names).len(), 3);
    }

    #[test]
    fn engine_instance_names_parse() {
        let n = "pid_1234_luid_0x00000000_0x0000F2C1_phys_0_eng_3_engtype_3D";
        assert_eq!(parse_luid(n), Some((0, 0xF2C1)));
        assert_eq!(parse_engine_instance(n), Some(((0, 0xF2C1), "phys_0_eng_3".to_string())));
        assert_eq!(parse_luid("garbage"), None);
        assert_eq!(parse_luid("luid_0x00000001_0x0000ABCD_phys_0"), Some((1, 0xABCD)));
    }

    #[test]
    fn engine_load_sums_processes_per_engine_then_takes_busiest_engine() {
        // Two processes share the 3D engine of GPU A (40 + 35 = 75); its copy engine is at 10.
        // GPU B has a single idle engine.
        let inst = vec![
            ("pid_1_luid_0x00000000_0x0000000A_phys_0_eng_0_engtype_3D".to_string(), 40.0),
            ("pid_2_luid_0x00000000_0x0000000A_phys_0_eng_0_engtype_3D".to_string(), 35.0),
            ("pid_2_luid_0x00000000_0x0000000A_phys_0_eng_1_engtype_Copy".to_string(), 10.0),
            ("pid_3_luid_0x00000000_0x0000000B_phys_0_eng_0_engtype_3D".to_string(), 0.0),
        ];
        let r = aggregate_engine_load(&inst);
        assert_eq!(r[&(0, 0xA)], 75.0);
        assert_eq!(r[&(0, 0xB)], 0.0, "an idle GPU is a real 0 %");
    }

    #[test]
    fn engine_load_is_capped_at_100() {
        let inst = vec![
            ("pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D".to_string(), 80.0),
            ("pid_2_luid_0x0_0x1_phys_0_eng_0_engtype_3D".to_string(), 80.0),
        ];
        assert_eq!(aggregate_engine_load(&inst)[&(0, 1)], 100.0);
    }

    #[test]
    fn discrete_gpu_is_primary() {
        let slots = vec![
            GpuSlot { name: "Intel UHD Graphics".into(), vendor: Vendor::Intel, vram_total_mb: Some(128.0), luid: None, aliases: vec![] },
            GpuSlot { name: "NVIDIA GeForce RTX 4070".into(), vendor: Vendor::Nvidia, vram_total_mb: Some(12288.0), luid: None, aliases: vec![] },
        ];
        let o = order_slots(slots.clone(), None);
        assert_eq!(o[0].name, "NVIDIA GeForce RTX 4070");
        let forced = order_slots(slots, Some(0));
        assert_eq!(forced[0].name, "Intel UHD Graphics", "the user's choice wins");
    }

    #[test]
    fn nvml_provider_without_driver_reports_nothing() {
        let p = NvmlProvider::new();
        if !p.available() {
            assert!(p.descriptors().is_empty());
        }
    }
}
