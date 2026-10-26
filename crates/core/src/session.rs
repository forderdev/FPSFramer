//! Session recording: per-second columns, a whole-session frame-time histogram,
//! spikes, drops and the final summary.

use crate::analysis::{
    analyze_bottleneck, below_target_pct, column_stats, find_drops, make_findings, Columns, FindingInputs, FpsDrop,
    SessionSummary, Spike, SpikeDetector,
};
use crate::frames::{low_fps, ms_to_fps, Histogram, LowsMethod, MAX_FRAME_MS, MIN_FRAMES_LOW01, MIN_FRAMES_LOW1};
use crate::model::Severity;
use crate::sensors::{ids, Readings, Reading};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SESSION_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HardwareInfo {
    pub cpu: String,
    pub gpus: Vec<String>,
    pub ram_gb: f64,
    pub os: String,
    pub displays: Vec<String>,
    pub app_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// Started automatically because a game was being measured.
    #[default]
    Game,
    /// Started by the user (hotkey / button); may have no game.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub kind: SessionKind,
    pub started_at: u64,
    pub ended_at: u64,
    pub duration_s: f64,
    pub game_name: Option<String>,
    pub exe: Option<String>,
    pub api: Option<String>,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    /// True when the data came from demo mode (never real hardware).
    pub demo: bool,
    pub hardware: HardwareInfo,
    pub notes: String,
    pub starred: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertLogEntry {
    pub t_s: f64,
    pub rule_id: String,
    pub name: String,
    pub severity: Severity,
    pub message: String,
    /// `"fired"` or `"cleared"`.
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub schema_version: u32,
    pub meta: SessionMeta,
    pub summary: SessionSummary,
    pub interval_ms: u32,
    pub columns: Columns,
    pub hist: crate::frames::SparseHistogram,
    pub spikes: Vec<Spike>,
    pub drops: Vec<FpsDrop>,
    pub alerts: Vec<AlertLogEntry>,
}

pub struct RecorderConfig {
    pub record_all_sensors: bool,
    pub target_fps: Option<f64>,
    pub lows_method: LowsMethod,
}

pub struct SessionRecorder {
    meta: SessionMeta,
    cfg: RecorderConfig,
    start_ns: u64,
    hist: Histogram,
    spikes: SpikeDetector,
    cur_sec: usize,
    frames: u32,
    sum_ms: f64,
    max_ms: f64,
    acc: BTreeMap<String, (f64, u32)>,
    columns: Columns,
    closed: usize,
    alerts: Vec<AlertLogEntry>,
    total_frames: u64,
}

fn is_frame_stat(id: &str) -> bool {
    id == ids::FPS || id.starts_with("fps.") || id.starts_with("frametime")
}

impl SessionRecorder {
    pub fn new(meta: SessionMeta, start_ns: u64, cfg: RecorderConfig) -> Self {
        Self {
            meta,
            cfg,
            start_ns,
            hist: Histogram::new(),
            spikes: SpikeDetector::new(),
            cur_sec: 0,
            frames: 0,
            sum_ms: 0.0,
            max_ms: 0.0,
            acc: BTreeMap::new(),
            columns: Columns::new(),
            closed: 0,
            alerts: Vec::new(),
            total_frames: 0,
        }
    }

    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }
    pub fn meta_mut(&mut self) -> &mut SessionMeta {
        &mut self.meta
    }
    pub fn elapsed_s(&self, now_ns: u64) -> f64 {
        now_ns.saturating_sub(self.start_ns) as f64 / 1e9
    }
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    fn secs_at(&self, t_ns: u64) -> usize {
        (t_ns.saturating_sub(self.start_ns) / 1_000_000_000) as usize
    }

    fn advance(&mut self, t_ns: u64) {
        let idx = self.secs_at(t_ns).min(self.cur_sec + 3600); // never loop unboundedly after a long suspend
        while self.cur_sec < idx {
            self.close_second();
        }
    }

    fn pad(&mut self, name: &str) {
        let closed = self.closed;
        let c = self.columns.entry(name.to_string()).or_default();
        while c.len() < closed {
            c.push(None);
        }
    }

    fn close_second(&mut self) {
        let have_frames = self.total_frames > 0;
        let mut row: Vec<(String, Option<f32>)> = Vec::new();
        if have_frames {
            row.push((ids::FPS.into(), Some(self.frames as f32)));
            if self.frames > 0 {
                row.push((ids::FRAMETIME.into(), Some((self.sum_ms / self.frames as f64) as f32)));
                row.push((ids::FRAMETIME_MAX.into(), Some(self.max_ms as f32)));
            }
        }
        for (k, (sum, n)) in std::mem::take(&mut self.acc) {
            if n > 0 {
                row.push((k, Some((sum / n as f64) as f32)));
            }
        }
        let names: Vec<String> = row.iter().map(|(k, _)| k.clone()).collect();
        for name in &names {
            self.pad(name);
        }
        for (k, v) in row {
            self.columns.get_mut(&k).unwrap().push(v);
        }
        // Columns that had no value this second get a gap, not a zero.
        let closed_after = self.closed + 1;
        for c in self.columns.values_mut() {
            while c.len() < closed_after {
                c.push(None);
            }
        }
        self.closed = closed_after;
        self.cur_sec += 1;
        self.frames = 0;
        self.sum_ms = 0.0;
        self.max_ms = 0.0;
    }

    pub fn on_frame(&mut self, t_ns: u64, ft_ms: f64) {
        if !(ft_ms.is_finite() && ft_ms > 0.0 && ft_ms <= MAX_FRAME_MS) {
            return;
        }
        self.advance(t_ns);
        self.frames += 1;
        self.total_frames += 1;
        self.sum_ms += ft_ms;
        self.max_ms = self.max_ms.max(ft_ms);
        self.hist.record(ft_ms);
        self.spikes.push(self.elapsed_s(t_ns), ft_ms);
    }

    pub fn on_sample(&mut self, t_ns: u64, readings: &Readings, recordable: &dyn Fn(&str) -> bool) {
        self.advance(t_ns);
        for (id, r) in readings {
            if is_frame_stat(id) {
                continue;
            }
            let Reading::Num(v) = r else { continue };
            if !(self.cfg.record_all_sensors || recordable(id)) {
                continue;
            }
            let e = self.acc.entry(id.clone()).or_insert((0.0, 0));
            e.0 += *v;
            e.1 += 1;
        }
    }

    pub fn on_alert(&mut self, t_ns: u64, mut entry: AlertLogEntry) {
        entry.t_s = self.elapsed_s(t_ns);
        if self.alerts.len() < 1000 {
            self.alerts.push(entry);
        }
    }

    /// Close the recording and compute everything derived from it.
    pub fn finish(mut self, end_ns: u64, ended_ms: u64) -> SessionData {
        self.advance(end_ns);
        // Keep the trailing partial second only if it holds at least half a second of data.
        let partial = self.elapsed_s(end_ns) - self.cur_sec as f64;
        if partial >= 0.5 && (self.frames > 0 || !self.acc.is_empty()) {
            if self.frames > 0 && partial < 1.0 {
                self.frames = (self.frames as f64 / partial).round() as u32; // scale to a per-second rate
            }
            self.close_second();
        }
        let duration = self.elapsed_s(end_ns);
        self.meta.ended_at = ended_ms;
        self.meta.duration_s = duration;

        let spikes = self.spikes.finish();
        let cols = self.columns;
        let drops = find_drops(&cols);
        let summary = build_summary(&self.hist, &cols, &spikes, &drops, self.cfg.target_fps, duration, self.cfg.lows_method);
        SessionData {
            schema_version: SESSION_SCHEMA,
            meta: self.meta,
            summary,
            interval_ms: 1000,
            columns: cols,
            hist: self.hist.to_sparse(),
            spikes,
            drops,
            alerts: self.alerts,
        }
    }
}

pub fn build_summary(
    hist: &Histogram,
    cols: &Columns,
    spikes: &[Spike],
    drops: &[FpsDrop],
    target: Option<f64>,
    duration_s: f64,
    method: LowsMethod,
) -> SessionSummary {
    let frames = hist.count();
    let mut s = SessionSummary { duration_s, frames, target_fps: target, ..Default::default() };
    if frames > 0 {
        s.fps_avg = hist.mean_ms().map(ms_to_fps);
        s.fps_min = hist.max_ms().map(ms_to_fps);
        s.fps_max = hist.min_ms().map(ms_to_fps);
        s.low1 = low_fps(hist, 0.01, method, MIN_FRAMES_LOW1);
        s.low01 = low_fps(hist, 0.001, method, MIN_FRAMES_LOW01);
        s.median_ms = hist.percentile_ms(50.0);
        s.p99_ms = hist.percentile_ms(99.0);
        s.worst_ms = hist.max_ms();
        if let (Some(a), Some(l)) = (s.fps_avg, s.low1) {
            if a > 0.0 {
                s.consistency = Some(l / a);
            }
        }
    }
    s.spike_count = spikes.len() as u32;
    s.drop_count = drops.len() as u32;
    s.sensor_stats = column_stats(cols);
    if let Some(t) = target {
        s.below_target_pct = below_target_pct(cols, t);
    }
    if !cols.is_empty() {
        s.bottleneck = Some(analyze_bottleneck(cols));
    }
    s.findings = make_findings(&FindingInputs { summary: &s, spikes, drops });
    s
}

/// Per-second table as CSV (opens directly in Excel / Google Sheets).
pub fn to_csv(data: &SessionData) -> String {
    let mut names: Vec<&String> = data.columns.keys().collect();
    let rank = |s: &str| match s {
        "fps" => 0,
        "frametime" => 1,
        "frametime.max" => 2,
        _ => 3,
    };
    names.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    let len = data.columns.values().map(|c| c.len()).max().unwrap_or(0);
    let mut out = String::from("time_s");
    for n in &names {
        out.push(',');
        out.push_str(&csv_field(n));
    }
    out.push('\n');
    for i in 0..len {
        out.push_str(&i.to_string());
        for n in &names {
            out.push(',');
            if let Some(Some(v)) = data.columns[*n].get(i) {
                out.push_str(&format!("{}", (*v as f64 * 100.0).round() / 100.0));
            }
        }
        out.push('\n');
    }
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::{ReadingsExt, Unit};

    fn cfg() -> RecorderConfig {
        RecorderConfig { record_all_sensors: false, target_fps: Some(60.0), lows_method: LowsMethod::Average }
    }

    fn record(seconds: u32, fps: u32, hitch_at: Option<u32>) -> SessionData {
        let mut r = SessionRecorder::new(SessionMeta { id: "s1".into(), ..Default::default() }, 0, cfg());
        let ft_ms = 1000.0 / fps as f64;
        let mut t = 0u64;
        let mut next_sample = 0u64;
        let end = seconds as u64 * 1_000_000_000;
        while t < end {
            let ft = match hitch_at {
                Some(h) if t / 1_000_000_000 == h as u64 && (t % 1_000_000_000) < (ft_ms * 1e6) as u64 => 90.0,
                _ => ft_ms,
            };
            t += (ft * 1e6) as u64;
            r.on_frame(t, ft);
            if t >= next_sample {
                let mut rd = Readings::new();
                rd.put(ids::GPU_LOAD, 97.0, Unit::Percent);
                rd.put(ids::CPU_LOAD, 40.0, Unit::Percent);
                rd.put("fps", 999.0, Unit::Fps); // live sensor must NOT overwrite exact recorder values
                r.on_sample(t, &rd, &|_| true);
                next_sample += 500_000_000;
            }
        }
        r.finish(end, 1_000)
    }

    #[test]
    fn per_second_columns_are_exact_and_aligned() {
        let d = record(60, 100, None);
        let fps = &d.columns[ids::FPS];
        assert!((59..=60).contains(&fps.len()), "len {}", fps.len());
        for v in fps.iter().take(58).flatten() {
            assert!((*v - 100.0).abs() <= 1.0, "fps column from frames, got {v}");
        }
        let n = fps.len();
        for (name, c) in &d.columns {
            assert!(c.len() == n, "column {name} misaligned: {} vs {n}", c.len());
        }
        assert!(d.columns.contains_key(ids::GPU_LOAD));
    }

    #[test]
    fn summary_reflects_hitch_and_bottleneck() {
        let d = record(120, 100, Some(60));
        let s = &d.summary;
        assert!((s.fps_avg.unwrap() - 100.0).abs() < 1.5);
        assert!(s.spike_count >= 1, "the 90 ms hitch must be a spike");
        assert!(s.worst_ms.unwrap() > 80.0);
        let b = s.bottleneck.as_ref().unwrap();
        assert_eq!(b.verdict, crate::analysis::Verdict::GpuLimited);
        assert!(s.findings.iter().any(|f| f.id == "gpu-limited"));
        assert!(s.consistency.unwrap() > 0.0);
    }

    #[test]
    fn csv_has_header_and_blank_for_gaps() {
        let mut d = record(5, 60, None);
        d.columns.insert("x,y".into(), vec![Some(1.0), None, Some(3.0), None, None]);
        let csv = to_csv(&d);
        let mut lines = csv.lines();
        let header = lines.next().unwrap();
        assert!(header.starts_with("time_s,fps,frametime,frametime.max"));
        assert!(header.contains("\"x,y\""), "fields with commas are quoted");
        let _row0 = lines.next().unwrap();
        let row1 = lines.next().unwrap(); // the injected "x,y" column has a gap at second 1
        assert!(row1.ends_with(','), "a missing value must be blank, not 0: {row1}");
    }

    #[test]
    fn session_without_frames_records_hardware_only() {
        let mut r = SessionRecorder::new(SessionMeta::default(), 0, cfg());
        for i in 0..10u64 {
            let mut rd = Readings::new();
            rd.put(ids::CPU_LOAD, 10.0, Unit::Percent);
            r.on_sample(i * 1_000_000_000 + 1, &rd, &|_| true);
        }
        let d = r.finish(10_000_000_000, 5);
        assert!(!d.columns.contains_key(ids::FPS), "no FPS column when nothing was measured");
        assert_eq!(d.summary.fps_avg, None);
        assert!(d.columns[ids::CPU_LOAD].iter().flatten().count() >= 9);
    }

    #[test]
    fn json_roundtrip() {
        let d = record(10, 144, None);
        let json = serde_json::to_string(&d).unwrap();
        let back: SessionData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.summary.frames, d.summary.frames);
        assert_eq!(back.columns.len(), d.columns.len());
    }
}
