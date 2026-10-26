//! GPU sensors on Windows: adapters from DXGI, load and memory from the vendor-neutral
//! Windows GPU performance counters (the same numbers Task Manager shows), and
//! temperature / clocks / power / fan from NVML on NVIDIA cards.

use super::pdh::Pdh;
use crate::catalog;
use crate::gpu::{aggregate_engine_load, merge_phantom_nvidia, order_slots, parse_luid, GpuSlot, Vendor};
use crate::nvml::{self, Nvml, NvmlDevice, NvmlSample};
use crate::provider::{PollOutcome, SensorProvider};
use fpsframer_core::sensors::{Readings, ReadingsExt, SensorDesc, UnavailableCode, Unit};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};

const SRC: &str = "windows-gpu";

/// Hardware GPUs known to Windows (the software "Basic Render Driver" is skipped).
pub fn enumerate_adapters() -> Vec<GpuSlot> {
    let mut out = Vec::new();
    // SAFETY: plain DXGI enumeration.
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            let Ok(d) = a.GetDesc1() else { continue };
            if d.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 || d.VendorId == 0x1414 {
                continue;
            }
            let end = d.Description.iter().position(|&c| c == 0).unwrap_or(d.Description.len());
            let name = String::from_utf16_lossy(&d.Description[..end]).trim().to_string();
            out.push(GpuSlot {
                name: name.clone(),
                vendor: Vendor::from_pci(d.VendorId),
                vram_total_mb: (d.DedicatedVideoMemory > 0).then(|| d.DedicatedVideoMemory as f64 / 1_048_576.0),
                luid: Some((d.AdapterLuid.HighPart as u32, d.AdapterLuid.LowPart)),
                aliases: Vec::new(),
            });
            log::info!(
                "Graphics adapter {}: \"{}\" vendor {:04X} device {:04X} luid {:08X}:{:08X} video memory {} MB",
                i - 1,
                name,
                d.VendorId,
                d.DeviceId,
                d.AdapterLuid.HighPart as u32,
                d.AdapterLuid.LowPart,
                d.DedicatedVideoMemory / 1_048_576
            );
        }
    }
    out
}

/// The graphics cards as the app should treat them: Windows' adapter list with phantom duplicates of an NVIDIA
/// card folded away (see [`merge_phantom_nvidia`]).
pub fn adapters() -> Vec<GpuSlot> {
    let raw = enumerate_adapters();
    let nvml_names: Vec<String> = if raw.iter().any(|s| s.vendor == Vendor::Nvidia) {
        Nvml::load().map(|n| n.devices().into_iter().map(|d| d.name).collect()).unwrap_or_default()
    } else {
        Vec::new()
    };
    let merged = merge_phantom_nvidia(raw.clone(), &nvml_names);
    if merged.len() != raw.len() {
        log::info!("{} adapters are listed but the NVIDIA driver reports {} card(s); duplicates were merged.", raw.len(), nvml_names.len());
    }
    merged
}

pub fn adapter_names() -> Vec<String> {
    adapters().into_iter().map(|s| s.name).collect()
}

struct EngineSampler {
    active: Option<(Pdh, usize)>,
    pending: Option<(Pdh, usize, Instant)>,
    last_rebuild: Instant,
}

fn new_engine_query() -> Option<(Pdh, usize)> {
    let mut p = Pdh::new()?;
    let c = p.add("\\GPU Engine(*)\\Utilization Percentage")?;
    p.collect(); // baseline sample
    Some((p, c))
}

impl EngineSampler {
    fn new() -> Self {
        Self { active: new_engine_query(), pending: None, last_rebuild: Instant::now() }
    }

    /// Per-process counter instances come and go as programs start, so the wildcard query is
    /// rebuilt every few seconds (in the background of normal sampling) to include a game that
    /// has just launched.
    fn sample(&mut self) -> Option<HashMap<(u32, u32), f64>> {
        if self.pending.is_none() && self.last_rebuild.elapsed() > Duration::from_secs(4) {
            self.last_rebuild = Instant::now();
            if let Some((p, c)) = new_engine_query() {
                self.pending = Some((p, c, Instant::now()));
            }
        }
        if let Some((mut p, c, t0)) = self.pending.take() {
            if t0.elapsed() < Duration::from_millis(350) {
                self.pending = Some((p, c, t0));
            } else if p.collect() && !p.array(c).is_empty() {
                self.active = Some((p, c));
            }
        }
        let (p, c) = self.active.as_mut()?;
        if !p.collect() {
            return None;
        }
        let arr = p.array(*c);
        (!arr.is_empty()).then(|| aggregate_engine_load(&arr))
    }
}

pub struct WindowsGpu {
    slots: Vec<GpuSlot>,
    nvml: Option<Nvml>,
    devices: Vec<NvmlDevice>,
    /// slot index → NVML device index
    map: Vec<Option<usize>>,
    probe: Vec<NvmlSample>,
    engine: EngineSampler,
    mem: Option<(Pdh, usize)>,
    descs: Vec<SensorDesc>,
    nvml_error: Option<String>,
}

impl WindowsGpu {
    pub fn new(primary: Option<usize>) -> Self {
        let slots = order_slots(adapters(), primary);
        let (nvml, devices, nvml_error) = if slots.iter().any(|s| s.vendor == Vendor::Nvidia) {
            match Nvml::load() {
                Ok(n) => {
                    let d = n.devices();
                    (Some(n), d, None)
                }
                Err(e) => (None, vec![], Some(e)),
            }
        } else {
            (None, vec![], None)
        };
        // Match each NVIDIA adapter to an NVML device: by name first, then by order.
        let mut used = vec![false; devices.len()];
        let mut map = vec![None; slots.len()];
        for (i, s) in slots.iter().enumerate() {
            if s.vendor != Vendor::Nvidia {
                continue;
            }
            let pick = devices
                .iter()
                .enumerate()
                .find(|(j, d)| !used[*j] && d.name.eq_ignore_ascii_case(&s.name))
                .or_else(|| devices.iter().enumerate().find(|(j, _)| !used[*j]))
                .map(|(j, _)| j);
            if let Some(j) = pick {
                used[j] = true;
                map[i] = Some(j);
            }
        }
        let probe = match &nvml {
            Some(n) => map.iter().map(|m| m.map(|j| n.sample(&devices[j])).unwrap_or_default()).collect(),
            None => vec![NvmlSample::default(); slots.len()],
        };
        let mem = Pdh::new().and_then(|mut p| {
            let c = p.add("\\GPU Adapter Memory(*)\\Dedicated Usage")?;
            p.collect();
            Some((p, c))
        });
        let mut s = Self { slots, nvml, devices, map, probe, engine: EngineSampler::new(), mem, descs: vec![], nvml_error };
        s.descs = s.build_descriptors();
        s
    }

    pub fn names(&self) -> Vec<String> {
        self.slots.iter().map(|s| s.name.clone()).collect()
    }

    fn build_descriptors(&self) -> Vec<SensorDesc> {
        let multi = self.slots.len() > 1;
        let mut v = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            let p = &self.probe[i];
            let has_nvml = self.map[i].is_some();
            let has_vram = slot.vram_total_mb.is_some_and(|m| m >= 256.0);
            for f in catalog::GPU_FIELDS {
                let nvml_has = has_nvml
                    && match f.field {
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
                let generic = match f.field {
                    "load" => self.engine.active.is_some(),
                    "vram.used" | "vram.percent" => has_vram && self.mem.is_some(),
                    "vram.total" => has_vram,
                    _ => false,
                };
                let mut d = catalog::gpu_desc(i, &slot.name, multi, f, if nvml_has { "nvml" } else { SRC });
                if !(nvml_has || generic) {
                    d = if matches!(f.field, "vram.used" | "vram.total" | "vram.percent") && !has_vram {
                        d.unavailable(
                            UnavailableCode::NoHardware,
                            "This graphics chip shares your system memory instead of having its own video memory.",
                            None,
                        )
                    } else if slot.vendor == Vendor::Nvidia {
                        match &self.nvml_error {
                            Some(_) => d.unavailable(
                                UnavailableCode::NeedsCompanion,
                                "The NVIDIA driver component FPSFramer needs (nvml.dll) was not found.",
                                Some("Install or repair the NVIDIA graphics driver from nvidia.com."),
                            ),
                            None => d.unavailable(UnavailableCode::NotSupported, "This graphics card or driver does not report this value.", None),
                        }
                    } else {
                        let (c, m, fix) = catalog::gpu_unavailable_hint(slot.vendor.label());
                        d.unavailable(c, m, fix.as_deref())
                    };
                }
                v.push(d);
            }
            v.push(catalog::gpu_text_desc(i, &slot.name, multi, "name", "model", "The name of your graphics card.", SRC));
            if has_nvml && p.throttle_thermal.is_some() {
                v.push(catalog::gpu_text_desc(i, &slot.name, multi, "throttle", "slowdown reason", "Whether the graphics driver is currently slowing the card down, and why.", "nvml"));
            }
        }
        v
    }
}

impl SensorProvider for WindowsGpu {
    fn id(&self) -> &'static str {
        "windows-gpu"
    }
    fn label(&self) -> &'static str {
        "Graphics cards (Windows counters + NVIDIA driver)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }

    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let load_by_luid = self.engine.sample();
        let mut mem_by_luid: HashMap<(u32, u32), f64> = HashMap::new();
        if let Some((p, c)) = self.mem.as_mut() {
            if p.collect() {
                for (n, v) in p.array(*c) {
                    if let Some(luid) = parse_luid(&n) {
                        *mem_by_luid.entry(luid).or_default() += v;
                    }
                }
            }
        }
        for (i, slot) in self.slots.iter().enumerate() {
            let prefix = catalog::gpu_prefix(i);
            let sample = match (&self.nvml, self.map[i]) {
                (Some(n), Some(j)) => Some(n.sample(&self.devices[j])),
                _ => None,
            };
            if let Some(s) = &sample {
                nvml::write_sample(out, &prefix, s);
            }
            // Fill what NVML didn't provide from the generic Windows counters. A card Windows lists under
            // several adapter ids reports under any of them; the busiest report wins.
            let luids: Vec<(u32, u32)> = slot.luid.into_iter().chain(slot.aliases.iter().copied()).collect();
            if !luids.is_empty() {
                if out.num_missing(&format!("{prefix}.load")) {
                    let load = luids.iter().filter_map(|l| load_by_luid.as_ref().and_then(|m| m.get(l))).copied().reduce(f64::max);
                    if let Some(l) = load {
                        out.put(&format!("{prefix}.load"), l.clamp(0.0, 100.0), Unit::Percent);
                    }
                }
                if let Some(total) = slot.vram_total_mb.filter(|m| *m >= 256.0) {
                    if out.num_missing(&format!("{prefix}.vram.used")) {
                        if let Some(bytes) = luids.iter().filter_map(|l| mem_by_luid.get(l)).copied().reduce(f64::max) {
                            let used = bytes / 1_048_576.0;
                            out.put(&format!("{prefix}.vram.used"), used, Unit::Megabytes);
                            out.put(&format!("{prefix}.vram.total"), total, Unit::Megabytes);
                            out.put(&format!("{prefix}.vram.percent"), (used * 100.0 / total).min(100.0), Unit::Percent);
                        }
                    }
                }
            }
            out.put_text(&format!("{prefix}.name"), slot.name.clone());
        }
        PollOutcome::ok()
    }
}

trait MissingExt {
    fn num_missing(&self, id: &str) -> bool;
}
impl MissingExt for Readings {
    fn num_missing(&self, id: &str) -> bool {
        !self.contains_key(id)
    }
}
