//! Post-session analysis: frame-time spikes, FPS drops, bottleneck estimation
//! and plain-language findings.
//!
//! Everything here works on recorded data only. Conclusions are labelled with a
//! [`Certainty`]: *measured* facts (a temperature peak) are distinguished from
//! *likely* explanations (GPU-bound) so a guess is never presented as a diagnosis.

use crate::sensors::ids;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub type Columns = BTreeMap<String, Vec<Option<f32>>>;

// ───────────────────────────── spikes ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spike {
    /// Seconds since the session started.
    pub t_s: f64,
    /// Worst frame time in the spike.
    pub frame_ms: f64,
    /// Typical frame time just before the spike.
    pub baseline_ms: f64,
    /// Number of slow frames merged into this spike.
    pub frames: u32,
}

/// Streaming detector: a frame is a spike when it is at least 25 ms *and* at
/// least 2.5× slower than the recent typical frame time.
pub struct SpikeDetector {
    baseline: f64,
    warm: u32,
    spikes: Vec<Spike>,
    open: Option<(Spike, f64)>, // (spike, last t)
}

const SPIKE_MIN_MS: f64 = 25.0;
const SPIKE_RATIO: f64 = 2.5;
const SPIKE_MERGE_S: f64 = 0.5;
const MAX_SPIKES: usize = 500;

impl Default for SpikeDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SpikeDetector {
    pub fn new() -> Self {
        Self { baseline: 0.0, warm: 0, spikes: Vec::new(), open: None }
    }

    pub fn push(&mut self, t_s: f64, ft_ms: f64) {
        if self.warm < 60 {
            self.baseline = if self.warm == 0 { ft_ms } else { self.baseline * 0.9 + ft_ms * 0.1 };
            self.warm += 1;
            return;
        }
        let is_spike = ft_ms >= SPIKE_MIN_MS && ft_ms >= self.baseline * SPIKE_RATIO;
        if is_spike {
            match &mut self.open {
                Some((s, last)) if t_s - *last <= SPIKE_MERGE_S => {
                    s.frame_ms = s.frame_ms.max(ft_ms);
                    s.frames += 1;
                    *last = t_s;
                }
                _ => {
                    self.close_open();
                    self.open = Some((
                        Spike { t_s, frame_ms: ft_ms, baseline_ms: self.baseline, frames: 1 },
                        t_s,
                    ));
                }
            }
        } else {
            self.baseline = self.baseline * 0.98 + ft_ms * 0.02;
            if let Some((_, last)) = &self.open {
                if t_s - *last > SPIKE_MERGE_S {
                    self.close_open();
                }
            }
        }
    }

    fn close_open(&mut self) {
        if let Some((s, _)) = self.open.take() {
            if self.spikes.len() >= MAX_SPIKES {
                // Keep the worst: replace the mildest one if this is worse.
                if let Some((i, m)) = self
                    .spikes
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.frame_ms.total_cmp(&b.1.frame_ms))
                    .map(|(i, m)| (i, m.frame_ms))
                {
                    if s.frame_ms > m {
                        self.spikes[i] = s;
                    }
                }
            } else {
                self.spikes.push(s);
            }
        }
    }

    pub fn finish(mut self) -> Vec<Spike> {
        self.close_open();
        self.spikes.sort_by(|a, b| a.t_s.total_cmp(&b.t_s));
        self.spikes
    }
}

// ───────────────────────────── FPS drops ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DropContext {
    pub cpu_load: Option<f64>,
    pub cpu_core_max: Option<f64>,
    pub gpu_load: Option<f64>,
    pub cpu_temp: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub ram_percent: Option<f64>,
    pub vram_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FpsDrop {
    pub start_s: f64,
    pub duration_s: f64,
    pub min_fps: f64,
    pub baseline_fps: f64,
    pub context: DropContext,
    /// Hedged, plain-language hint; `None` when no single factor stands out.
    pub hint: Option<String>,
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// Find stretches where per-second FPS fell well below its recent typical level.
pub fn find_drops(cols: &Columns) -> Vec<FpsDrop> {
    let Some(fps) = cols.get(ids::FPS) else { return Vec::new() };
    let mut hist: VecDeque<f32> = VecDeque::new();
    let mut out = Vec::new();
    let mut cur: Option<(usize, usize, f32, f32)> = None; // start, len, min, baseline

    let flush = |cur: &mut Option<(usize, usize, f32, f32)>, out: &mut Vec<FpsDrop>| {
        if let Some((start, len, min, base)) = cur.take() {
            if len >= 2 || min < base * 0.5 {
                let ctx = context_over(cols, start, start + len);
                let hint = drop_hint(&ctx);
                out.push(FpsDrop {
                    start_s: start as f64,
                    duration_s: len as f64,
                    min_fps: min as f64,
                    baseline_fps: base as f64,
                    context: ctx,
                    hint,
                });
            }
        }
    };

    for (i, v) in fps.iter().enumerate() {
        let Some(v) = *v else {
            flush(&mut cur, &mut out);
            continue;
        };
        let baseline = if hist.len() >= 8 {
            let mut c: Vec<f32> = hist.iter().cloned().collect();
            Some(median(&mut c))
        } else {
            None
        };
        let dropped = baseline.is_some_and(|b| b >= 20.0 && v < b * 0.7);
        if dropped {
            let b = baseline.unwrap();
            match &mut cur {
                Some((_, len, min, _)) => {
                    *len += 1;
                    *min = min.min(v);
                    // A long plateau is a new normal, not a drop: adopt it.
                    if *len > 30 {
                        flush(&mut cur, &mut out);
                        hist.clear();
                    }
                }
                None => cur = Some((i, 1, v, b)),
            }
        } else {
            flush(&mut cur, &mut out);
            hist.push_back(v);
            if hist.len() > 20 {
                hist.pop_front();
            }
        }
    }
    flush(&mut cur, &mut out);
    out.sort_by(|a, b| (b.baseline_fps - b.min_fps).total_cmp(&(a.baseline_fps - a.min_fps)));
    out.truncate(50);
    out.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    out
}

fn avg_over(col: Option<&Vec<Option<f32>>>, from: usize, to: usize) -> Option<f64> {
    let col = col?;
    let (mut s, mut n) = (0.0f64, 0u32);
    for v in col.iter().skip(from).take(to.saturating_sub(from)).flatten() {
        s += *v as f64;
        n += 1;
    }
    (n > 0).then(|| s / n as f64)
}

fn context_over(cols: &Columns, from: usize, to: usize) -> DropContext {
    DropContext {
        cpu_load: avg_over(cols.get(ids::CPU_LOAD), from, to),
        cpu_core_max: avg_over(cols.get(ids::CPU_CORE_MAX), from, to),
        gpu_load: avg_over(cols.get(ids::GPU_LOAD), from, to),
        cpu_temp: avg_over(cols.get(ids::CPU_TEMP), from, to),
        gpu_temp: avg_over(cols.get(ids::GPU_TEMP), from, to),
        ram_percent: avg_over(cols.get(ids::RAM_PERCENT), from, to),
        vram_percent: avg_over(cols.get(ids::GPU_VRAM_PERCENT), from, to),
    }
}

fn drop_hint(c: &DropContext) -> Option<String> {
    if c.vram_percent.is_some_and(|v| v >= 95.0) {
        Some("Video memory (VRAM) was nearly full. This can cause sudden stutters.".into())
    } else if c.ram_percent.is_some_and(|v| v >= 92.0) {
        Some("System memory was nearly full, so Windows may have been swapping to disk.".into())
    } else if c.cpu_temp.is_some_and(|v| v >= 92.0) || c.gpu_temp.is_some_and(|v| v >= 90.0) {
        Some("Temperatures were very high, so the hardware may have slowed itself down to cool off.".into())
    } else if c.gpu_load.is_some_and(|v| v >= 95.0) {
        Some("The graphics card was fully loaded, so a heavy scene is a likely reason.".into())
    } else if c.gpu_load.is_some_and(|v| v < 70.0)
        && (c.cpu_core_max.is_some_and(|v| v >= 90.0) || c.cpu_load.is_some_and(|v| v >= 90.0))
    {
        Some("The graphics card had spare capacity while the processor was very busy, so a CPU-heavy moment is a likely reason.".into())
    } else if c.gpu_load.is_some_and(|v| v < 50.0) && c.cpu_load.is_some_and(|v| v < 50.0) {
        Some("Neither the CPU nor the GPU was busy, so the game may have been loading or waiting (for example on disk or network).".into())
    } else {
        None
    }
}

// ───────────────────────── bottleneck estimate ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    GpuLimited,
    CpuLimited,
    FrameCapped,
    Balanced,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Moderate,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BottleneckReport {
    pub analyzed_seconds: u32,
    pub verdict: Verdict,
    pub confidence: Confidence,
    pub gpu_bound_pct: Option<f64>,
    pub cpu_bound_pct: Option<f64>,
    pub vram_pressure_pct: Option<f64>,
    pub ram_pressure_pct: Option<f64>,
    pub hot_pct: Option<f64>,
    pub throttle_pct: Option<f64>,
    pub on_battery_pct: Option<f64>,
    /// Frame-rate cap (V-Sync or limiter) the session appears to be sitting at.
    pub cap_fps: Option<f64>,
}

const COMMON_CAPS: [f64; 13] = [24.0, 30.0, 60.0, 72.0, 75.0, 90.0, 100.0, 120.0, 144.0, 165.0, 180.0, 240.0, 360.0];

fn at(col: Option<&Vec<Option<f32>>>, i: usize) -> Option<f64> {
    col.and_then(|c| c.get(i)).and_then(|v| *v).map(|v| v as f64)
}

pub fn analyze_bottleneck(cols: &Columns) -> BottleneckReport {
    let n = cols.values().map(|c| c.len()).max().unwrap_or(0);
    let fps = cols.get(ids::FPS);
    let gpu = cols.get(ids::GPU_LOAD);
    let cpu = cols.get(ids::CPU_LOAD);
    let core = cols.get(ids::CPU_CORE_MAX);
    let vram = cols.get(ids::GPU_VRAM_PERCENT);
    let ram = cols.get(ids::RAM_PERCENT);
    let cpu_t = cols.get(ids::CPU_TEMP);
    let gpu_t = cols.get(ids::GPU_TEMP);
    let thr_t = cols.get("gpu.throttle.thermal");
    let thr_p = cols.get("gpu.throttle.power");
    let batt = cols.get("battery.discharging");

    let (mut analyzed, mut gpu_b, mut cpu_b, mut have_load) = (0u32, 0u32, 0u32, 0u32);
    let (mut vram_n, mut vram_p, mut ram_n, mut ram_p) = (0u32, 0u32, 0u32, 0u32);
    let (mut hot_n, mut hot_p, mut thr_n, mut thr_c, mut bat_n, mut bat_c) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);

    for i in 0..n {
        // Only judge seconds where the game was actually rendering (or no FPS data exists at all).
        let playing = match fps {
            Some(_) => at(fps, i).is_some_and(|f| f > 0.5),
            None => true,
        };
        if !playing {
            continue;
        }
        analyzed += 1;
        let g = at(gpu, i);
        let c = at(cpu, i);
        let m = at(core, i);
        match g {
            Some(g) if c.is_some() || m.is_some() => {
                have_load += 1;
                if g >= 95.0 {
                    gpu_b += 1;
                } else if g < 85.0 && (m.is_some_and(|v| v >= 85.0) || c.is_some_and(|v| v >= 90.0)) {
                    cpu_b += 1;
                }
            }
            Some(g) => {
                have_load += 1;
                if g >= 95.0 {
                    gpu_b += 1;
                }
            }
            None => {}
        }
        if let Some(v) = at(vram, i) {
            vram_n += 1;
            if v >= 95.0 {
                vram_p += 1;
            }
        }
        if let Some(v) = at(ram, i) {
            ram_n += 1;
            if v >= 92.0 {
                ram_p += 1;
            }
        }
        let (ct, gt) = (at(cpu_t, i), at(gpu_t, i));
        if ct.is_some() || gt.is_some() {
            hot_n += 1;
            if ct.is_some_and(|v| v >= 90.0) || gt.is_some_and(|v| v >= 88.0) {
                hot_p += 1;
            }
        }
        let (tt, tp) = (at(thr_t, i), at(thr_p, i));
        if tt.is_some() || tp.is_some() {
            thr_n += 1;
            if tt.is_some_and(|v| v > 0.5) || tp.is_some_and(|v| v > 0.5) {
                thr_c += 1;
            }
        }
        if let Some(b) = at(batt, i) {
            bat_n += 1;
            if b > 0.5 {
                bat_c += 1;
            }
        }
    }
    let pct = |num: u32, den: u32| (den > 0).then(|| num as f64 * 100.0 / den as f64);

    // Frame-cap detection: FPS pinned to a common value while the GPU is not saturated.
    let mut cap_fps = None;
    if let Some(f) = fps {
        let mut vals: Vec<f32> = f.iter().flatten().cloned().filter(|v| *v > 5.0).collect();
        if vals.len() >= 30 {
            let m = median(&mut vals) as f64;
            let near = vals.iter().filter(|v| (**v as f64 - m).abs() <= (m * 0.02).max(1.5)).count();
            let steady = near as f64 / vals.len() as f64 >= 0.85;
            let gpu_avg = avg_over(gpu, 0, n);
            if steady && gpu_avg.is_none_or(|g| g < 95.0) {
                if let Some(c) = COMMON_CAPS.iter().find(|c| (**c - m).abs() <= (**c * 0.03).max(1.5)) {
                    cap_fps = Some(*c);
                }
            }
        }
    }

    let gpu_pct = pct(gpu_b, have_load);
    let cpu_pct = pct(cpu_b, have_load);
    let (verdict, confidence) = if cap_fps.is_some() {
        (Verdict::FrameCapped, Confidence::Moderate)
    } else if analyzed < 20 || have_load < 20 {
        (Verdict::Unknown, Confidence::Low)
    } else if gpu_pct.unwrap_or(0.0) >= 60.0 {
        let p = gpu_pct.unwrap_or(0.0);
        (Verdict::GpuLimited, conf(p, analyzed))
    } else if cpu_pct.unwrap_or(0.0) >= 40.0 {
        let p = cpu_pct.unwrap_or(0.0);
        (Verdict::CpuLimited, conf(p + 20.0, analyzed))
    } else {
        (Verdict::Balanced, Confidence::Low)
    };

    BottleneckReport {
        analyzed_seconds: analyzed,
        verdict,
        confidence,
        gpu_bound_pct: gpu_pct,
        cpu_bound_pct: cpu_pct,
        vram_pressure_pct: pct(vram_p, vram_n),
        ram_pressure_pct: pct(ram_p, ram_n),
        hot_pct: pct(hot_p, hot_n),
        throttle_pct: pct(thr_c, thr_n),
        on_battery_pct: pct(bat_c, bat_n),
        cap_fps,
    }
}

fn conf(pct: f64, secs: u32) -> Confidence {
    if pct >= 80.0 && secs >= 120 {
        Confidence::High
    } else if pct >= 60.0 && secs >= 60 {
        Confidence::Moderate
    } else {
        Confidence::Low
    }
}

// ─────────────────────────── summary + findings ───────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnStat {
    pub min: f64,
    pub avg: f64,
    pub max: f64,
    /// Seconds of data behind these numbers.
    pub samples: u32,
}

pub fn column_stats(cols: &Columns) -> BTreeMap<String, ColumnStat> {
    let mut out = BTreeMap::new();
    for (k, v) in cols {
        let (mut mn, mut mx, mut sum, mut n) = (f64::MAX, f64::MIN, 0.0, 0u32);
        for x in v.iter().flatten() {
            let x = *x as f64;
            mn = mn.min(x);
            mx = mx.max(x);
            sum += x;
            n += 1;
        }
        if n > 0 {
            out.insert(k.clone(), ColumnStat { min: mn, avg: sum / n as f64, max: mx, samples: n });
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum FindingLevel {
    Good,
    Info,
    Notice,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Certainty {
    /// Directly read from a sensor.
    Measured,
    /// Inferred from several readings; usually right but not guaranteed.
    Likely,
    /// A possibility worth checking.
    Possible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub level: FindingLevel,
    pub certainty: Certainty,
    pub title: String,
    pub detail: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub duration_s: f64,
    pub frames: u64,
    pub fps_avg: Option<f64>,
    pub fps_min: Option<f64>,
    pub fps_max: Option<f64>,
    pub low1: Option<f64>,
    pub low01: Option<f64>,
    pub median_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub worst_ms: Option<f64>,
    /// 1% low ÷ average FPS. Close to 1 = even pacing; low = uneven.
    pub consistency: Option<f64>,
    pub spike_count: u32,
    pub drop_count: u32,
    pub target_fps: Option<f64>,
    pub below_target_pct: Option<f64>,
    pub sensor_stats: BTreeMap<String, ColumnStat>,
    pub bottleneck: Option<BottleneckReport>,
    pub findings: Vec<Finding>,
}

fn f0(v: f64) -> String {
    format!("{v:.0}")
}

pub struct FindingInputs<'a> {
    pub summary: &'a SessionSummary,
    pub spikes: &'a [Spike],
    pub drops: &'a [FpsDrop],
}

/// Turn numbers into advice. Wording is deliberately hedged for anything inferred.
fn add(out: &mut Vec<Finding>, id: &str, level: FindingLevel, certainty: Certainty, title: &str, detail: String, evidence: Vec<String>) {
    out.push(Finding { id: id.into(), level, certainty, title: title.into(), detail, evidence });
}

pub fn make_findings(inp: &FindingInputs) -> Vec<Finding> {
    let s = inp.summary;
    let mut out: Vec<Finding> = Vec::new();
    let st = &s.sensor_stats;

    if let Some(b) = &s.bottleneck {
        match b.verdict {
            Verdict::FrameCapped => {
                let cap = b.cap_fps.unwrap_or(0.0);
                add(&mut out, 
                    "frame-capped",
                    FindingLevel::Info,
                    Certainty::Likely,
                    &format!("Frame rate looks capped at about {} FPS", f0(cap)),
                    "FPS stayed in a very narrow band while the graphics card was not fully loaded. That usually means V-Sync or a frame limiter is active. This is often intentional: it reduces heat, noise and power use. If you want higher FPS, check V-Sync and frame-limit settings in the game and in your graphics driver.".into(),
                    vec![format!("Median FPS sat within a few FPS of {}", f0(cap))],
                );
            }
            Verdict::GpuLimited => add(&mut out, 
                "gpu-limited",
                FindingLevel::Notice,
                Certainty::Likely,
                "Your graphics card was probably the limiting factor",
                "GPU usage was at 95% or higher for much of the session. When the graphics card is the limit, FPS typically improves by lowering resolution, shadows, ray tracing or other heavy graphics settings, or by turning on an upscaler (DLSS, FSR or XeSS) if the game offers one. This is an estimate based on usage readings, not a guaranteed diagnosis.".into(),
                vec![format!("GPU ≥ 95% for {}% of the measured time", f0(b.gpu_bound_pct.unwrap_or(0.0)))],
            ),
            Verdict::CpuLimited => add(&mut out, 
                "cpu-limited",
                FindingLevel::Notice,
                Certainty::Likely,
                "Your processor may have been limiting performance",
                "While the graphics card still had room to spare, at least one CPU core was working close to its limit. Games often depend heavily on one or two cores. Lowering CPU-heavy settings (crowd size, view distance, physics), closing background programs, or raising the resolution (which shifts work to the graphics card) may help. This is an estimate from usage readings, not a confirmed diagnosis.".into(),
                vec![format!("GPU < 85% while a CPU core was ≥ 85% for {}% of the measured time", f0(b.cpu_bound_pct.unwrap_or(0.0)))],
            ),
            Verdict::Balanced | Verdict::Unknown => {}
        }
        if b.verdict == Verdict::Unknown && b.analyzed_seconds >= 20 {
            add(&mut out, 
                "no-load-data",
                FindingLevel::Info,
                Certainty::Measured,
                "Not enough sensor data to tell what limited performance",
                "CPU or GPU usage readings were missing for this session, so FPSFramer can't estimate whether the processor or the graphics card was the limit. See Help → Sensors to find out what is missing on this PC.".into(),
                vec![],
            );
        }
        if let Some(t) = b.throttle_pct.filter(|t| *t >= 5.0) {
            add(&mut out, 
                "gpu-throttle",
                FindingLevel::Warning,
                Certainty::Measured,
                "The graphics card reported slowing itself down",
                "The GPU driver reported a thermal or power slowdown during the session. This means the card reduced its clock speed to stay within temperature or power limits, which lowers FPS. Check case airflow, fan curves and dust, and make sure the card's power cables are fully connected.".into(),
                vec![format!("Slowdown reported for {}% of the session", f0(t))],
            );
        }
        if let Some(p) = b.vram_pressure_pct.filter(|p| *p >= 5.0) {
            add(&mut out, 
                "vram-full",
                FindingLevel::Notice,
                Certainty::Measured,
                "Video memory (VRAM) was almost full",
                "When VRAM fills up, the game has to swap textures in and out, which commonly causes stutter. Lowering texture quality or resolution usually helps.".into(),
                vec![format!("VRAM ≥ 95% for {}% of the session", f0(p))],
            );
        }
        if let Some(p) = b.ram_pressure_pct.filter(|p| *p >= 5.0) {
            add(&mut out, 
                "ram-full",
                FindingLevel::Notice,
                Certainty::Measured,
                "System memory (RAM) was almost full",
                "With little free memory Windows has to use the slower page file on disk, which can cause hitching. Closing browsers and other background apps can help; if it happens often, more RAM may be worth considering.".into(),
                vec![format!("RAM ≥ 92% for {}% of the session", f0(p))],
            );
        }
        if let Some(p) = b.on_battery_pct.filter(|p| *p >= 50.0) {
            add(&mut out, 
                "on-battery",
                FindingLevel::Notice,
                Certainty::Measured,
                "The laptop was running on battery",
                "Many laptops limit CPU and GPU power on battery, which lowers FPS. Plugging in and choosing a high-performance power mode usually gives noticeably better results.".into(),
                vec![format!("On battery for {}% of the session", f0(p))],
            );
        }
    }

    // Temperatures.
    for (id, label, warn_at, hot_at) in [
        (ids::CPU_TEMP, "CPU", 85.0, 95.0),
        (ids::GPU_TEMP, "Graphics card", 83.0, 90.0),
    ] {
        if let Some(c) = st.get(id) {
            if c.max >= warn_at {
                let hot = c.max >= hot_at;
                add(&mut out, 
                    &format!("temp-{}", id.split('.').next().unwrap_or("x")),
                    if hot { FindingLevel::Warning } else { FindingLevel::Notice },
                    Certainty::Measured,
                    &format!("{label} temperature reached {}°C", f0(c.max)),
                    "Very high temperatures can make hardware slow itself down (thermal throttling) to protect itself. Check that fans are spinning, vents and filters are clear of dust, and the case has good airflow. Laptops benefit from a hard, flat surface or a cooling stand.".into(),
                    vec![format!("Average {}°C, peak {}°C", f0(c.avg), f0(c.max))],
                );
            }
        }
    }
    if !st.contains_key(ids::CPU_TEMP) && s.duration_s >= 60.0 {
        add(&mut out, 
            "no-cpu-temp",
            FindingLevel::Info,
            Certainty::Measured,
            "CPU temperature wasn't available",
            "Windows doesn't allow apps to read CPU temperature without a hardware driver. Run HWiNFO (with Shared Memory enabled) or LibreHardwareMonitor alongside FPSFramer to add it, so overheating can be checked.".into(),
            vec![],
        );
    }

    // Smoothness.
    if let (Some(avg), Some(low1)) = (s.fps_avg, s.low1) {
        let ratio = low1 / avg;
        if ratio < 0.6 || s.spike_count >= 10 {
            let worst = inp.spikes.iter().map(|x| x.frame_ms).fold(0.0, f64::max);
            add(&mut out, 
                "stutter",
                FindingLevel::Warning,
                Certainty::Measured,
                "Frame delivery was uneven (stutter)",
                format!(
                    "Your 1% Low ({} FPS) was only {}% of your average ({} FPS), and {} frame-time spikes over 25 ms were detected. Common causes are shader compilation (especially the first time you play), asset streaming, background activity, thermal throttling or running out of memory. Check the spike list to see when they happened.",
                    f0(low1), f0(ratio * 100.0), f0(avg), s.spike_count
                ),
                vec![format!("Worst single frame: {} ms", f0(worst))],
            );
        } else if ratio >= 0.8 && s.spike_count < 3 {
            add(&mut out, 
                "smooth",
                FindingLevel::Good,
                Certainty::Measured,
                "Smooth frame delivery",
                format!("Your 1% Low ({} FPS) stayed close to your average ({} FPS) and there were very few spikes, so the game felt consistent.", f0(low1), f0(avg)),
                vec![],
            );
        }
    }

    if let (Some(t), Some(p)) = (s.target_fps, s.below_target_pct) {
        if p >= 10.0 {
            add(&mut out, 
                "below-target",
                FindingLevel::Notice,
                Certainty::Measured,
                &format!("FPS was below your {} FPS target", f0(t)),
                "FPS dropped under your target for part of the session. The bottleneck and drop details below may show why.".into(),
                vec![format!("Below target for {}% of the time", f0(p))],
            );
        }
    }

    if !inp.drops.is_empty() {
        let worst = inp
            .drops
            .iter()
            .max_by(|a, b| (a.baseline_fps - a.min_fps).total_cmp(&(b.baseline_fps - b.min_fps)))
            .unwrap();
        add(&mut out, 
            "fps-drops",
            FindingLevel::Info,
            if worst.hint.is_some() { Certainty::Possible } else { Certainty::Measured },
            &format!("{} noticeable FPS drop{}", inp.drops.len(), if inp.drops.len() == 1 { "" } else { "s" }),
            format!(
                "The largest started at {} and fell from about {} to {} FPS for {} s.{}",
                fmt_clock(worst.start_s),
                f0(worst.baseline_fps),
                f0(worst.min_fps),
                f0(worst.duration_s),
                worst.hint.as_ref().map(|h| format!(" {h}")).unwrap_or_default()
            ),
            vec![],
        );
    }

    if !out.iter().any(|f| f.level >= FindingLevel::Notice) && !out.iter().any(|f| f.level == FindingLevel::Good) && s.fps_avg.is_some() {
        add(&mut out, 
            "no-issues",
            FindingLevel::Good,
            Certainty::Measured,
            "No significant problems detected",
            "Nothing in the recorded data stood out as a problem.".into(),
            vec![],
        );
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.level));
    out
}

pub fn fmt_clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Percentage of seconds in which per-second FPS was below `target * 0.95`.
pub fn below_target_pct(cols: &Columns, target: f64) -> Option<f64> {
    let fps = cols.get(ids::FPS)?;
    let vals: Vec<f32> = fps.iter().flatten().cloned().collect();
    if vals.is_empty() {
        return None;
    }
    let below = vals.iter().filter(|v| (**v as f64) < target * 0.95).count();
    Some(below as f64 * 100.0 / vals.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(v: &[f32]) -> Vec<Option<f32>> {
        v.iter().map(|x| Some(*x)).collect()
    }

    #[test]
    fn spikes_are_detected_and_merged() {
        let mut d = SpikeDetector::new();
        let mut t = 0.0;
        for _ in 0..300 {
            t += 0.010;
            d.push(t, 10.0);
        }
        // One hitch made of three consecutive slow frames, then a separate one 3 s later.
        for ft in [60.0, 45.0, 80.0] {
            t += ft / 1000.0;
            d.push(t, ft);
        }
        for _ in 0..300 {
            t += 0.010;
            d.push(t, 10.0);
        }
        t += 0.12;
        d.push(t, 120.0);
        for _ in 0..50 {
            t += 0.010;
            d.push(t, 10.0);
        }
        let spikes = d.finish();
        assert_eq!(spikes.len(), 2, "{spikes:?}");
        assert_eq!(spikes[0].frames, 3);
        assert_eq!(spikes[0].frame_ms, 80.0);
        assert_eq!(spikes[1].frame_ms, 120.0);
    }

    #[test]
    fn normal_variation_is_not_a_spike() {
        let mut d = SpikeDetector::new();
        let mut t = 0.0;
        for i in 0..2000 {
            let ft = 16.0 + (i % 7) as f64; // 16..22 ms
            t += ft / 1000.0;
            d.push(t, ft);
        }
        assert!(d.finish().is_empty());
    }

    #[test]
    fn fps_drop_is_found_with_context_and_hint() {
        let mut fps = vec![100.0f32; 40];
        fps.extend([40.0, 38.0, 42.0, 41.0]);
        fps.extend(vec![100.0; 30]);
        let n = fps.len();
        let mut cols = Columns::new();
        cols.insert(ids::FPS.into(), col(&fps));
        let mut gpu = vec![70.0f32; n];
        for g in gpu.iter_mut().skip(40).take(4) {
            *g = 99.0;
        }
        cols.insert(ids::GPU_LOAD.into(), col(&gpu));
        let drops = find_drops(&cols);
        assert_eq!(drops.len(), 1);
        let d = &drops[0];
        assert_eq!(d.start_s, 40.0);
        assert_eq!(d.duration_s, 4.0);
        assert_eq!(d.min_fps, 38.0);
        assert!(d.hint.as_ref().unwrap().contains("graphics card"));
    }

    #[test]
    fn a_new_plateau_is_not_reported_forever() {
        let mut fps = vec![100.0f32; 30];
        fps.extend(vec![55.0; 200]); // entered a heavy area and stayed there
        let mut cols = Columns::new();
        cols.insert(ids::FPS.into(), col(&fps));
        let drops = find_drops(&cols);
        assert!(drops.len() <= 2, "plateau must be adopted, got {}", drops.len());
    }

    #[test]
    fn gpu_bound_session() {
        let n = 300;
        let mut cols = Columns::new();
        let fps: Vec<f32> = (0..n).map(|i| 80.0 + (i % 17) as f32).collect();
        cols.insert(ids::FPS.into(), col(&fps));
        cols.insert(ids::GPU_LOAD.into(), col(&vec![99.0; n]));
        cols.insert(ids::CPU_LOAD.into(), col(&vec![40.0; n]));
        cols.insert(ids::CPU_CORE_MAX.into(), col(&vec![60.0; n]));
        let r = analyze_bottleneck(&cols);
        assert_eq!(r.verdict, Verdict::GpuLimited);
        assert_eq!(r.confidence, Confidence::High);
        assert!(r.gpu_bound_pct.unwrap() > 99.0);
    }

    #[test]
    fn cpu_bound_session() {
        let n = 300;
        let mut cols = Columns::new();
        let fps: Vec<f32> = (0..n).map(|i| 70.0 + (i % 23) as f32).collect();
        cols.insert(ids::FPS.into(), col(&fps));
        cols.insert(ids::GPU_LOAD.into(), col(&vec![55.0; n]));
        cols.insert(ids::CPU_LOAD.into(), col(&vec![35.0; n]));
        cols.insert(ids::CPU_CORE_MAX.into(), col(&vec![97.0; n]));
        let r = analyze_bottleneck(&cols);
        assert_eq!(r.verdict, Verdict::CpuLimited);
    }

    #[test]
    fn frame_cap_is_recognised_instead_of_calling_it_a_bottleneck() {
        let n = 200;
        let mut cols = Columns::new();
        cols.insert(ids::FPS.into(), col(&vec![60.0; n]));
        cols.insert(ids::GPU_LOAD.into(), col(&vec![62.0; n]));
        cols.insert(ids::CPU_LOAD.into(), col(&vec![30.0; n]));
        let r = analyze_bottleneck(&cols);
        assert_eq!(r.verdict, Verdict::FrameCapped);
        assert_eq!(r.cap_fps, Some(60.0));
    }

    #[test]
    fn missing_sensors_give_unknown_not_a_guess() {
        let n = 200;
        let mut cols = Columns::new();
        let fps: Vec<f32> = (0..n).map(|i| 80.0 + (i % 13) as f32).collect();
        cols.insert(ids::FPS.into(), col(&fps));
        let r = analyze_bottleneck(&cols);
        assert_eq!(r.verdict, Verdict::Unknown);
        assert_eq!(r.gpu_bound_pct, None);
    }

    #[test]
    fn idle_seconds_are_not_analysed_as_gameplay() {
        let mut cols = Columns::new();
        let mut fps = vec![0.0f32; 100];
        fps.extend((0..100).map(|i| 90.0 + (i % 9) as f32));
        cols.insert(ids::FPS.into(), col(&fps));
        cols.insert(ids::GPU_LOAD.into(), col(&vec![99.0; 200]));
        cols.insert(ids::CPU_LOAD.into(), col(&vec![20.0; 200]));
        let r = analyze_bottleneck(&cols);
        assert_eq!(r.analyzed_seconds, 100);
    }

    #[test]
    fn findings_are_hedged_and_ordered() {
        let mut stats = BTreeMap::new();
        stats.insert(ids::GPU_TEMP.to_string(), ColumnStat { min: 40.0, avg: 80.0, max: 92.0, samples: 100 });
        let summary = SessionSummary {
            duration_s: 300.0,
            fps_avg: Some(100.0),
            low1: Some(40.0),
            spike_count: 12,
            sensor_stats: stats,
            bottleneck: Some(BottleneckReport {
                analyzed_seconds: 300,
                verdict: Verdict::GpuLimited,
                confidence: Confidence::High,
                gpu_bound_pct: Some(90.0),
                cpu_bound_pct: Some(1.0),
                vram_pressure_pct: None,
                ram_pressure_pct: None,
                hot_pct: Some(20.0),
                throttle_pct: None,
                on_battery_pct: None,
                cap_fps: None,
            }),
            ..Default::default()
        };
        let f = make_findings(&FindingInputs { summary: &summary, spikes: &[], drops: &[] });
        let ids: Vec<&str> = f.iter().map(|x| x.id.as_str()).collect();
        assert!(ids.contains(&"gpu-limited") && ids.contains(&"stutter") && ids.contains(&"temp-gpu"));
        let gpu = f.iter().find(|x| x.id == "gpu-limited").unwrap();
        assert_eq!(gpu.certainty, Certainty::Likely, "bottleneck is an estimate, not a fact");
        assert!(gpu.detail.contains("not a guaranteed diagnosis"));
        let temp = f.iter().find(|x| x.id == "temp-gpu").unwrap();
        assert_eq!(temp.certainty, Certainty::Measured);
        assert!(f.windows(2).all(|w| w[0].level >= w[1].level), "most important first");
    }

    #[test]
    fn column_stats_ignore_gaps() {
        let mut cols = Columns::new();
        cols.insert("a".into(), vec![Some(1.0), None, Some(3.0)]);
        cols.insert("b".into(), vec![None, None]);
        let s = column_stats(&cols);
        assert_eq!(s["a"].avg, 2.0);
        assert!(!s.contains_key("b"), "an all-gap column has no stats (not zero)");
    }

    #[test]
    fn below_target() {
        let mut cols = Columns::new();
        cols.insert(ids::FPS.into(), col(&[60.0, 60.0, 40.0, 30.0]));
        assert_eq!(below_target_pct(&cols, 60.0), Some(50.0));
    }
}
