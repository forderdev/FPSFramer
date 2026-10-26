//! Sensors from `/proc` and `/sys`.
//!
//! FPSFramer ships for Windows, but this provider lets the whole pipeline run
//! (and be tested) on Linux with real readings. It takes a root directory so
//! tests can point it at a synthetic file tree.

use crate::catalog;
use crate::provider::{PollOutcome, SensorProvider};
use fpsframer_core::sensors::{
    ids, Category, Readings, ReadingsExt, SensorDesc, StatusRule, UnavailableCode, Unit,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SRC: &str = "linux";

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

fn parse_cpu_line(line: &str) -> Option<(String, CpuTimes)> {
    let mut it = line.split_whitespace();
    let name = it.next()?;
    if !name.starts_with("cpu") {
        return None;
    }
    let v: Vec<u64> = it.filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return None;
    }
    // user nice system idle iowait irq softirq steal …
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((name.to_string(), CpuTimes { total, idle }))
}

fn load_from(prev: CpuTimes, cur: CpuTimes) -> Option<f64> {
    let dt = cur.total.checked_sub(prev.total)?;
    let di = cur.idle.checked_sub(prev.idle)?;
    if dt == 0 {
        return None;
    }
    Some((100.0 * (1.0 - di as f64 / dt as f64)).clamp(0.0, 100.0))
}

#[derive(Clone, Copy, Default)]
struct DiskTotals {
    read_bytes: u64,
    write_bytes: u64,
    ios: u64,
    io_ms: u64,
    busy_ms_max: u64,
}

#[derive(Clone)]
struct HwmonTemp {
    path: PathBuf,
    id: String,
}

struct Hwmon {
    temps: Vec<(HwmonTemp, SensorDesc)>,
    fans: Vec<(PathBuf, String)>,
    power: Vec<(PathBuf, String)>,
}

pub struct LinuxBasic {
    root: PathBuf,
    cores: usize,
    prev_cpu: Vec<(String, CpuTimes)>,
    prev_disk: Option<(Instant, DiskTotals)>,
    prev_net: Option<(Instant, u64, u64)>,
    has_cpufreq: bool,
    hwmon: Hwmon,
    battery: Option<PathBuf>,
    gpus: Vec<AmdGpu>,
    descs: Vec<SensorDesc>,
}

struct AmdGpu {
    dir: PathBuf,
}

fn read(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}
fn read_f(p: &Path) -> Option<f64> {
    read(p)?.parse().ok()
}

impl LinuxBasic {
    pub fn new() -> Self {
        Self::with_root(PathBuf::from("/"))
    }

    pub fn with_root(root: PathBuf) -> Self {
        let stat = read(&root.join("proc/stat")).unwrap_or_default();
        let cpu: Vec<(String, CpuTimes)> = stat.lines().filter_map(parse_cpu_line).collect();
        let cores = cpu.iter().filter(|(n, _)| n != "cpu").count();
        let has_cpufreq = root.join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").exists();
        let hwmon = scan_hwmon(&root);
        let battery = fs::read_dir(root.join("sys/class/power_supply"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .find(|p| read(&p.join("type")).as_deref() == Some("Battery"));
        let gpus = scan_amdgpu(&root);
        let mut s = Self {
            root,
            cores,
            prev_cpu: Vec::new(),
            prev_disk: None,
            prev_net: None,
            has_cpufreq,
            hwmon,
            battery,
            gpus,
            descs: Vec::new(),
        };
        s.descs = s.build_descriptors();
        s
    }

    fn build_descriptors(&self) -> Vec<SensorDesc> {
        let mut v = vec![catalog::cpu_load(SRC), catalog::cpu_core_max(SRC)];
        for i in 0..self.cores {
            v.push(catalog::cpu_core_load(i, SRC));
        }
        if self.has_cpufreq || self.cpu_mhz_from_cpuinfo().is_some() {
            v.push(catalog::cpu_clock(SRC));
        }
        if let Some(name) = self.cpu_model() {
            let _ = name;
            v.push(catalog::cpu_name(SRC));
        }
        // CPU temperature: from hwmon if a CPU chip exists, otherwise explain.
        if self.hwmon.temps.iter().any(|(t, _)| t.id == ids::CPU_TEMP) {
            v.push(catalog::cpu_temp(SRC));
        } else {
            v.push(
                catalog::cpu_temp(SRC).unavailable(
                    UnavailableCode::NotSupported,
                    "This system does not expose a CPU temperature sensor (no coretemp/k10temp driver found).",
                    Some("On a physical PC, load your board's sensor drivers (for example `sudo sensors-detect`). Virtual machines usually have none."),
                ),
            );
        }
        for (_, d) in &self.hwmon.temps {
            if d.id != ids::CPU_TEMP && d.id != ids::GPU_TEMP {
                v.push(d.clone());
            }
        }
        for (_, id) in &self.hwmon.fans {
            v.push(
                SensorDesc::new(id.clone(), format!("Fan {}", id.rsplit('.').next().unwrap_or("")), Category::Fan, Unit::Rpm)
                    .desc("Fan speed in revolutions per minute.")
                    .source(SRC),
            );
        }
        for (_, id) in &self.hwmon.power {
            if id == ids::CPU_POWER {
                v.push(catalog::cpu_power(SRC));
            }
        }
        v.extend(catalog::ram_set(SRC));
        v.extend(catalog::disk_set(SRC));
        v.extend(catalog::net_set(SRC));
        if self.battery.is_some() {
            v.extend(catalog::battery_set(SRC));
        }
        // GPUs: amdgpu via sysfs. NVIDIA is handled by the NVML-based GpuProvider.
        let multi = self.gpus.len() > 1;
        for (i, g) in self.gpus.iter().enumerate() {
            let name = g.name();
            for f in catalog::GPU_FIELDS {
                let supported = match f.field {
                    "load" => g.dir.join("gpu_busy_percent").exists(),
                    "vram.used" | "vram.total" | "vram.percent" => g.dir.join("mem_info_vram_used").exists(),
                    "temp" => g.hwmon_file("temp1_input").is_some(),
                    "power" => g.hwmon_file("power1_average").is_some() || g.hwmon_file("power1_input").is_some(),
                    _ => false,
                };
                if supported {
                    v.push(catalog::gpu_desc(i, &name, multi, f, SRC));
                }
            }
        }
        v
    }

    fn cpu_model(&self) -> Option<String> {
        let info = read(&self.root.join("proc/cpuinfo"))?;
        info.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).map(|s| s.trim().to_string())
    }

    fn cpu_mhz_from_cpuinfo(&self) -> Option<f64> {
        let info = read(&self.root.join("proc/cpuinfo"))?;
        let v: Vec<f64> = info.lines().filter(|l| l.starts_with("cpu MHz")).filter_map(|l| l.split(':').nth(1)?.trim().parse().ok()).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
    }
}

impl Default for LinuxBasic {
    fn default() -> Self {
        Self::new()
    }
}

impl AmdGpu {
    fn name(&self) -> String {
        read(&self.dir.join("product_name")).unwrap_or_else(|| "AMD Radeon GPU".into())
    }
    fn hwmon_file(&self, f: &str) -> Option<PathBuf> {
        fs::read_dir(self.dir.join("hwmon")).ok()?.flatten().map(|e| e.path().join(f)).find(|p| p.exists())
    }
}

fn scan_amdgpu(root: &Path) -> Vec<AmdGpu> {
    let mut v = Vec::new();
    for e in fs::read_dir(root.join("sys/class/drm")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with("card") && !name.contains('-') {
            let dir = e.path().join("device");
            if read(&dir.join("vendor")).as_deref() == Some("0x1002") {
                v.push(AmdGpu { dir });
            }
        }
    }
    v
}

fn scan_hwmon(root: &Path) -> Hwmon {
    let mut h = Hwmon { temps: Vec::new(), fans: Vec::new(), power: Vec::new() };
    let mut have_cpu = false;
    for e in fs::read_dir(root.join("sys/class/hwmon")).into_iter().flatten().flatten() {
        let dir = e.path();
        let chip = read(&dir.join("name")).unwrap_or_default();
        let n = e.file_name().to_string_lossy().to_string();
        let cpu_chip = matches!(chip.as_str(), "coretemp" | "k10temp" | "zenpower" | "cpu_thermal");
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let f = entry.file_name().to_string_lossy().to_string();
            if let Some(idx) = f.strip_prefix("temp").and_then(|s| s.strip_suffix("_input")) {
                let label = read(&dir.join(format!("temp{idx}_label"))).unwrap_or_default();
                if chip == "amdgpu" || chip == "nouveau" {
                    continue; // handled by the GPU code
                }
                if cpu_chip && !have_cpu && (label.contains("Package") || label.contains("Tctl") || label.contains("Tdie") || idx == "1") {
                    have_cpu = true;
                    h.temps.push((HwmonTemp { path: entry.path(), id: ids::CPU_TEMP.into() }, catalog::cpu_temp(SRC)));
                } else if !cpu_chip && chip != "acpitz" {
                    let id = format!("ext.hwmon.{}.{}.temp{}", n, fpsframer_core::util::slug(&chip), idx);
                    let nm = if label.is_empty() { format!("{chip} temperature") } else { format!("{chip} {label}") };
                    let d = SensorDesc::new(id.clone(), nm, Category::Thermal, Unit::Celsius)
                        .desc("A temperature sensor reported by your hardware.")
                        .rule(StatusRule::higher(75.0, 90.0))
                        .source(SRC);
                    h.temps.push((HwmonTemp { path: entry.path(), id }, d));
                }
            } else if let Some(idx) = f.strip_prefix("fan").and_then(|s| s.strip_suffix("_input")) {
                h.fans.push((entry.path(), format!("ext.hwmon.{}.fan{}", n, idx)));
            } else if f == "power1_average" && cpu_chip {
                h.power.push((entry.path(), ids::CPU_POWER.into()));
            }
        }
    }
    h.temps.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    h
}

impl SensorProvider for LinuxBasic {
    fn id(&self) -> &'static str {
        "linux"
    }
    fn label(&self) -> &'static str {
        "Linux system files (/proc, /sys)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        self.descs.clone()
    }

    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let now = Instant::now();
        // ── CPU ──
        if let Some(stat) = read(&self.root.join("proc/stat")) {
            let cur: Vec<(String, CpuTimes)> = stat.lines().filter_map(parse_cpu_line).collect();
            let mut core_max: Option<f64> = None;
            for (name, t) in &cur {
                if let Some((_, p)) = self.prev_cpu.iter().find(|(n, _)| n == name) {
                    if let Some(l) = load_from(*p, *t) {
                        if name == "cpu" {
                            out.put(ids::CPU_LOAD, l, Unit::Percent);
                        } else if let Some(i) = name.strip_prefix("cpu").and_then(|n| n.parse::<usize>().ok()) {
                            out.put(&format!("cpu.core.{i}.load"), l, Unit::Percent);
                            core_max = Some(core_max.map_or(l, |m: f64| m.max(l)));
                        }
                    }
                }
            }
            if let Some(m) = core_max {
                out.put(ids::CPU_CORE_MAX, m, Unit::Percent);
            }
            self.prev_cpu = cur;
        }
        // CPU clock
        let mut freqs = Vec::new();
        if self.has_cpufreq {
            for i in 0..self.cores {
                if let Some(khz) = read_f(&self.root.join(format!("sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"))) {
                    freqs.push(khz / 1000.0);
                }
            }
        }
        let clock = if freqs.is_empty() { self.cpu_mhz_from_cpuinfo() } else { Some(freqs.iter().sum::<f64>() / freqs.len() as f64) };
        if let Some(c) = clock {
            out.put(ids::CPU_CLOCK, c, Unit::Mhz);
        }
        if let Some(n) = self.cpu_model() {
            out.put_text(ids::CPU_NAME, n);
        }
        // ── memory ──
        if let Some(mem) = read(&self.root.join("proc/meminfo")) {
            let get = |key: &str| mem.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok());
            if let (Some(total), Some(avail)) = (get("MemTotal:"), get("MemAvailable:")) {
                let (total_mb, avail_mb) = (total / 1024.0, avail / 1024.0);
                out.put(ids::RAM_TOTAL, total_mb, Unit::Megabytes);
                out.put(ids::RAM_AVAILABLE, avail_mb, Unit::Megabytes);
                out.put(ids::RAM_USED, total_mb - avail_mb, Unit::Megabytes);
                out.put(ids::RAM_PERCENT, (total_mb - avail_mb) * 100.0 / total_mb, Unit::Percent);
            }
        }
        // ── disk ──
        if let Some(ds) = read(&self.root.join("proc/diskstats")) {
            let mut t = DiskTotals::default();
            for l in ds.lines() {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() < 14 || !is_whole_disk(f[2]) {
                    continue;
                }
                let g = |i: usize| f[i].parse::<u64>().unwrap_or(0);
                t.read_bytes += g(5) * 512;
                t.write_bytes += g(9) * 512;
                t.ios += g(3) + g(7);
                t.io_ms += g(6) + g(10);
                t.busy_ms_max = t.busy_ms_max.max(g(12));
            }
            if let Some((t0, p)) = self.prev_disk {
                let dt = now.duration_since(t0).as_secs_f64();
                if dt > 0.05 {
                    out.put(ids::DISK_READ, t.read_bytes.saturating_sub(p.read_bytes) as f64 / dt, Unit::BytesPerSec);
                    out.put(ids::DISK_WRITE, t.write_bytes.saturating_sub(p.write_bytes) as f64 / dt, Unit::BytesPerSec);
                    out.put(ids::DISK_ACTIVE, (t.busy_ms_max.saturating_sub(p.busy_ms_max) as f64 / (dt * 1000.0) * 100.0).min(100.0), Unit::Percent);
                    let dios = t.ios.saturating_sub(p.ios);
                    if dios > 0 {
                        out.put(ids::DISK_LATENCY, t.io_ms.saturating_sub(p.io_ms) as f64 / dios as f64, Unit::Ms);
                    }
                }
            }
            self.prev_disk = Some((now, t));
        }
        // ── network ──
        if let Some(nd) = read(&self.root.join("proc/net/dev")) {
            let (mut rx, mut tx) = (0u64, 0u64);
            for l in nd.lines().skip(2) {
                let Some((name, rest)) = l.split_once(':') else { continue };
                if !is_physical_nic(name.trim()) {
                    continue;
                }
                let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if f.len() >= 9 {
                    rx += f[0];
                    tx += f[8];
                }
            }
            if let Some((t0, prx, ptx)) = self.prev_net {
                let dt = now.duration_since(t0).as_secs_f64();
                if dt > 0.05 {
                    out.put(ids::NET_DOWN, rx.saturating_sub(prx) as f64 / dt, Unit::BytesPerSec);
                    out.put(ids::NET_UP, tx.saturating_sub(ptx) as f64 / dt, Unit::BytesPerSec);
                }
            }
            self.prev_net = Some((now, rx, tx));
        }
        // ── hwmon ──
        for (t, _) in &self.hwmon.temps {
            if let Some(mc) = read_f(&t.path) {
                out.put(&t.id, mc / 1000.0, Unit::Celsius);
            }
        }
        for (p, id) in &self.hwmon.fans {
            if let Some(rpm) = read_f(p) {
                out.put(id, rpm, Unit::Rpm);
            }
        }
        for (p, id) in &self.hwmon.power {
            if let Some(uw) = read_f(p) {
                out.put(id, uw / 1_000_000.0, Unit::Watt);
            }
        }
        // ── battery ──
        if let Some(b) = &self.battery {
            if let Some(c) = read_f(&b.join("capacity")) {
                out.put(ids::BATTERY_PERCENT, c, Unit::Percent);
            }
            if let Some(s) = read(&b.join("status")) {
                out.put("battery.discharging", if s == "Discharging" { 1.0 } else { 0.0 }, Unit::Count);
            }
        }
        // ── amdgpu ──
        let multi = self.gpus.len() > 1;
        let _ = multi;
        for (i, g) in self.gpus.iter().enumerate() {
            let p = catalog::gpu_prefix(i);
            if let Some(v) = read_f(&g.dir.join("gpu_busy_percent")) {
                out.put(&format!("{p}.load"), v, Unit::Percent);
            }
            if let (Some(u), Some(t)) = (read_f(&g.dir.join("mem_info_vram_used")), read_f(&g.dir.join("mem_info_vram_total"))) {
                if t > 0.0 {
                    out.put(&format!("{p}.vram.used"), u / 1_048_576.0, Unit::Megabytes);
                    out.put(&format!("{p}.vram.total"), t / 1_048_576.0, Unit::Megabytes);
                    out.put(&format!("{p}.vram.percent"), u * 100.0 / t, Unit::Percent);
                }
            }
            if let Some(f) = g.hwmon_file("temp1_input") {
                if let Some(v) = read_f(&f) {
                    out.put(&format!("{p}.temp"), v / 1000.0, Unit::Celsius);
                }
            }
            if let Some(f) = g.hwmon_file("power1_average").or_else(|| g.hwmon_file("power1_input")) {
                if let Some(v) = read_f(&f) {
                    out.put(&format!("{p}.power"), v / 1_000_000.0, Unit::Watt);
                }
            }
        }
        PollOutcome::ok()
    }
}

fn is_whole_disk(name: &str) -> bool {
    let b = name.as_bytes();
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    if let Some(rest) = name.strip_prefix("nvme") {
        // nvme0n1 yes, nvme0n1p1 no
        return rest.contains('n') && !rest.contains('p');
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return all_digits(rest);
    }
    for p in ["sd", "vd", "xvd", "hd"] {
        if let Some(rest) = name.strip_prefix(p) {
            return !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_lowercase());
        }
    }
    let _ = b;
    false
}

fn is_physical_nic(name: &str) -> bool {
    !(name == "lo" || name.starts_with("docker") || name.starts_with("veth") || name.starts_with("br-") || name.starts_with("virbr") || name.starts_with("tun") || name.starts_with("tap"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    fn root() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fpsf-linux-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("proc/net")).unwrap();
        d
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn cpu_load_from_two_samples() {
        let r = root();
        write(&r, "proc/stat", "cpu  100 0 100 800 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0\ncpu1 50 0 50 400 0 0 0 0\n");
        write(&r, "proc/meminfo", "MemTotal: 8388608 kB\nMemAvailable: 4194304 kB\n");
        let mut p = LinuxBasic::with_root(r.clone());
        assert_eq!(p.cores, 2);
        let mut out = Readings::new();
        p.poll(&mut out);
        assert_eq!(out.num(ids::CPU_LOAD), None, "needs two samples, and must not pretend to be 0");
        assert_eq!(out.num(ids::RAM_PERCENT), Some(50.0));
        // cpu0: +100 busy of +200 total ticks = 50 %. cpu1: +0 busy of +100 = 0 %. Aggregate: +200 of +400 = 50 %.
        write(&r, "proc/stat", "cpu  200 0 200 1000 0 0 0 0\ncpu0 100 0 100 500 0 0 0 0\ncpu1 50 0 50 500 0 0 0 0\n");
        let mut out = Readings::new();
        p.poll(&mut out);
        assert_eq!(out.num("cpu.core.0.load"), Some(50.0));
        assert_eq!(out.num("cpu.core.1.load"), Some(0.0), "an idle core is a real 0, not missing");
        assert_eq!(out.num(ids::CPU_CORE_MAX), Some(50.0));
        assert_eq!(out.num(ids::CPU_LOAD), Some(50.0));
    }

    #[test]
    fn missing_cpu_temperature_is_reported_unavailable_with_reason() {
        let r = root();
        write(&r, "proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n");
        let p = LinuxBasic::with_root(r);
        let d = p.descriptors();
        let t = d.iter().find(|d| d.id == ids::CPU_TEMP).unwrap();
        assert!(!t.availability.available);
        assert!(t.availability.reason.as_ref().unwrap().message.contains("temperature"));
    }

    #[test]
    fn hwmon_cpu_temperature_is_picked_up() {
        let r = root();
        write(&r, "proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n");
        write(&r, "sys/class/hwmon/hwmon2/name", "k10temp\n");
        write(&r, "sys/class/hwmon/hwmon2/temp1_input", "61500\n");
        write(&r, "sys/class/hwmon/hwmon2/temp1_label", "Tctl\n");
        let mut p = LinuxBasic::with_root(r);
        let t = p.descriptors().into_iter().find(|d| d.id == ids::CPU_TEMP).unwrap();
        assert!(t.availability.available);
        let mut out = Readings::new();
        p.poll(&mut out);
        assert_eq!(out.num(ids::CPU_TEMP), Some(61.5));
    }

    #[test]
    fn network_rate_ignores_virtual_interfaces() {
        let r = root();
        write(&r, "proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n");
        let dev = |rx: u64, tx: u64| {
            format!("Inter-|   Receive\n face |bytes\n  lo: 99999 0 0 0 0 0 0 0 99999 0 0 0 0 0 0 0\neth0: {rx} 0 0 0 0 0 0 0 {tx} 0 0 0 0 0 0 0\ndocker0: 5000000 0 0 0 0 0 0 0 5000000 0 0 0 0 0 0 0\n")
        };
        write(&r, "proc/net/dev", &dev(1_000, 500));
        let mut p = LinuxBasic::with_root(r.clone());
        let mut out = Readings::new();
        p.poll(&mut out);
        std::thread::sleep(std::time::Duration::from_millis(120));
        write(&r, "proc/net/dev", &dev(1_000 + 120_000, 500 + 12_000));
        let mut out = Readings::new();
        p.poll(&mut out);
        let down = out.num(ids::NET_DOWN).unwrap();
        // 132 kB over at least 0.12 s. The upper bound is exact; the lower bound allows a loaded CI runner to
        // oversleep by several times (it once measured ~0.25 s).
        assert!((150_000.0..1_300_000.0).contains(&down), "~132 kB in >= 0.12 s, got {down}");
        assert!(out.num(ids::NET_UP).unwrap() < down);
    }

    #[test]
    fn whole_disk_filter() {
        for ok in ["sda", "nvme0n1", "vdb", "mmcblk0"] {
            assert!(is_whole_disk(ok), "{ok}");
        }
        for no in ["sda1", "nvme0n1p2", "loop0", "dm-0", "mmcblk0p1", "ram0"] {
            assert!(!is_whole_disk(no), "{no}");
        }
    }

    #[test]
    fn battery_is_only_listed_when_present() {
        let r = root();
        write(&r, "proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n");
        assert!(!LinuxBasic::with_root(r.clone()).descriptors().iter().any(|d| d.id == ids::BATTERY_PERCENT));
        write(&r, "sys/class/power_supply/BAT0/type", "Battery\n");
        write(&r, "sys/class/power_supply/BAT0/capacity", "42\n");
        write(&r, "sys/class/power_supply/BAT0/status", "Discharging\n");
        let mut p = LinuxBasic::with_root(r);
        assert!(p.descriptors().iter().any(|d| d.id == ids::BATTERY_PERCENT));
        let mut out = Readings::new();
        p.poll(&mut out);
        assert_eq!(out.num(ids::BATTERY_PERCENT), Some(42.0));
        assert_eq!(out.num("battery.discharging"), Some(1.0));
    }

    #[test]
    fn real_machine_smoke_test() {
        // Runs against the actual /proc of the machine running the tests.
        if !Path::new("/proc/stat").exists() {
            return;
        }
        let mut p = LinuxBasic::new();
        let mut out = Readings::new();
        p.poll(&mut out);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut out = Readings::new();
        p.poll(&mut out);
        let load = out.num(ids::CPU_LOAD).expect("cpu load after two samples");
        assert!((0.0..=100.0).contains(&load));
        assert!(out.num(ids::RAM_TOTAL).unwrap() > 100.0);
    }
}
