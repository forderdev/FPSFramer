//! Frame-time statistics.
//!
//! Frame times arrive as `(timestamp_ns, frame_time_ms)` pairs from a capture
//! backend. [`FrameTracker`] turns them into the numbers shown in the overlay:
//! FPS, frame time, min/avg/max FPS and the 1% / 0.1% lows.
//!
//! ## Definitions (also explained to users in the UI)
//! * **FPS** – frames presented per second, measured over the last
//!   `fps_window_ms` (default 1 s).
//! * **Frame time** – milliseconds between two consecutive presented frames.
//! * **Min FPS / Max FPS** – the slowest / fastest *single frame* in the stats
//!   window, expressed as FPS.
//! * **1% Low** – the average FPS of the slowest 1 % of frames
//!   (`LowsMethod::Average`) or the FPS at the 99th-percentile frame time
//!   (`LowsMethod::Percentile`). Both need enough frames to be meaningful.
//! * **0.1% Low** – same with the slowest 0.1 % of frames.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const FINE_BINS: usize = 2000; // 0.05 ms resolution up to 100 ms
const FINE_PER_MS: f64 = 20.0;
const COARSE_BINS: usize = 4900; // 1 ms resolution from 100 ms to 5000 ms
const TOTAL_BINS: usize = FINE_BINS + COARSE_BINS;

/// Frame times above this are treated as pauses (alt-tab, loading screens,
/// window minimised) rather than rendering performance, and are not recorded.
pub const MAX_FRAME_MS: f64 = 5000.0;
/// Minimum frames before a 1 % low is reported.
pub const MIN_FRAMES_LOW1: u64 = 100;
/// Minimum frames before a 0.1 % low is reported.
pub const MIN_FRAMES_LOW01: u64 = 1000;

fn bin_of(ms: f64) -> usize {
    if ms < 100.0 {
        ((ms * FINE_PER_MS) as usize).min(FINE_BINS - 1)
    } else {
        (FINE_BINS + (ms - 100.0) as usize).min(TOTAL_BINS - 1)
    }
}

fn bin_mid(i: usize) -> f64 {
    if i < FINE_BINS {
        (i as f64 + 0.5) / FINE_PER_MS
    } else {
        100.0 + (i - FINE_BINS) as f64 + 0.5
    }
}

/// Fixed-bin histogram of frame times (milliseconds).
#[derive(Clone)]
pub struct Histogram {
    bins: Vec<u32>,
    count: u64,
    sum_ms: f64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    pub fn new() -> Self {
        Self { bins: vec![0; TOTAL_BINS], count: 0, sum_ms: 0.0 }
    }
    pub fn record(&mut self, ms: f64) {
        self.bins[bin_of(ms)] += 1;
        self.count += 1;
        self.sum_ms += ms;
    }
    /// Remove a sample previously passed to [`record`](Self::record).
    pub fn remove(&mut self, ms: f64) {
        let b = bin_of(ms);
        if self.bins[b] > 0 {
            self.bins[b] -= 1;
            self.count -= 1;
            self.sum_ms -= ms;
            if self.count == 0 {
                self.sum_ms = 0.0;
            }
        }
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn mean_ms(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum_ms / self.count as f64)
    }
    pub fn min_ms(&self) -> Option<f64> {
        self.bins.iter().position(|&c| c > 0).map(bin_mid)
    }
    pub fn max_ms(&self) -> Option<f64> {
        self.bins.iter().rposition(|&c| c > 0).map(bin_mid)
    }
    /// Nearest-rank percentile of frame time, `p` in 0..=100.
    pub fn percentile_ms(&self, p: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let rank = ((p / 100.0) * self.count as f64).ceil().max(1.0) as u64;
        let mut acc = 0u64;
        for (i, &c) in self.bins.iter().enumerate() {
            acc += c as u64;
            if acc >= rank {
                return Some(bin_mid(i));
            }
        }
        self.max_ms()
    }
    /// Mean frame time of the slowest `fraction` (0..1) of frames.
    pub fn mean_of_slowest_ms(&self, fraction: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let want = ((fraction * self.count as f64).ceil() as u64).clamp(1, self.count);
        let mut left = want;
        let mut sum = 0.0;
        for i in (0..TOTAL_BINS).rev() {
            let c = self.bins[i] as u64;
            if c == 0 {
                continue;
            }
            let take = c.min(left);
            sum += take as f64 * bin_mid(i);
            left -= take;
            if left == 0 {
                break;
            }
        }
        Some(sum / want as f64)
    }
    pub fn merge(&mut self, other: &Histogram) {
        for (a, b) in self.bins.iter_mut().zip(&other.bins) {
            *a += *b;
        }
        self.count += other.count;
        self.sum_ms += other.sum_ms;
    }
    /// Sparse form for storage: `(bin, count)` pairs plus the exact sum.
    pub fn to_sparse(&self) -> SparseHistogram {
        SparseHistogram {
            bins: self
                .bins
                .iter()
                .enumerate()
                .filter(|(_, &c)| c > 0)
                .map(|(i, &c)| (i as u32, c))
                .collect(),
            sum_ms: self.sum_ms,
        }
    }
    pub fn from_sparse(s: &SparseHistogram) -> Self {
        let mut h = Self::new();
        for &(i, c) in &s.bins {
            if (i as usize) < TOTAL_BINS {
                h.bins[i as usize] = c;
                h.count += c as u64;
            }
        }
        h.sum_ms = s.sum_ms;
        h
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SparseHistogram {
    pub bins: Vec<(u32, u32)>,
    pub sum_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LowsMethod {
    /// Average FPS of the slowest N % of frames (captures how bad the hitches were).
    #[default]
    Average,
    /// FPS at the (100-N)th percentile frame time.
    Percentile,
}

/// Convert a frame time (ms) into FPS.
pub fn ms_to_fps(ms: f64) -> f64 {
    if ms > 0.0 {
        1000.0 / ms
    } else {
        0.0
    }
}

pub fn low_fps(h: &Histogram, fraction: f64, method: LowsMethod, min_frames: u64) -> Option<f64> {
    if h.count() < min_frames {
        return None;
    }
    let ms = match method {
        LowsMethod::Average => h.mean_of_slowest_ms(fraction)?,
        LowsMethod::Percentile => h.percentile_ms(100.0 * (1.0 - fraction))?,
    };
    Some(ms_to_fps(ms))
}

#[derive(Debug, Clone, Copy)]
pub struct TrackerConfig {
    /// Window for the headline FPS number.
    pub fps_window_ms: u32,
    /// Window for min/avg/max and lows. `None` = whole session.
    pub stats_window_s: Option<u32>,
    pub lows_method: LowsMethod,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self { fps_window_ms: 1000, stats_window_s: None, lows_method: LowsMethod::Average }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameStats {
    pub fps: Option<f64>,
    pub frametime: Option<f64>,
    pub frametime_max: Option<f64>,
    pub fps_avg: Option<f64>,
    pub fps_min: Option<f64>,
    pub fps_max: Option<f64>,
    pub low1: Option<f64>,
    pub low01: Option<f64>,
    pub p99_ms: Option<f64>,
    pub frames_in_stats: u64,
    pub total_frames: u64,
}

/// Frames reach us in batches (Windows delivers trace events a buffer at a time), so the newest frame
/// is always a little older than "now". These bound how long we trust the newest frame as the end of the
/// measuring window before declaring that the game has stalled.
const MIN_GRACE_NS: u64 = 500_000_000;
const MAX_GRACE_NS: u64 = 3_000_000_000;
/// How far back delivery delays are remembered.
const LAG_MEMORY_NS: u64 = 10_000_000_000;

pub struct FrameTracker {
    cfg: TrackerConfig,
    frames: VecDeque<(u64, f32)>,
    window: Histogram,
    session: Histogram,
    pending: Vec<f32>,
    total: u64,
    last_t: Option<u64>,
    /// `(when observed, how old the newest frame had become just before a new batch arrived)`.
    lag: VecDeque<(u64, u64)>,
}

impl FrameTracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Self {
            cfg,
            frames: VecDeque::new(),
            window: Histogram::new(),
            session: Histogram::new(),
            pending: Vec::new(),
            total: 0,
            last_t: None,
            lag: VecDeque::new(),
        }
    }

    pub fn set_config(&mut self, cfg: TrackerConfig) {
        let window_changed = cfg.stats_window_s != self.cfg.stats_window_s;
        self.cfg = cfg;
        if window_changed {
            // Rebuild the windowed histogram from whatever frames we still hold.
            self.window = Histogram::new();
            for &(_, ft) in &self.frames {
                self.window.record(ft as f64);
            }
        }
    }

    fn keep_ns(&self) -> u64 {
        match self.cfg.stats_window_s {
            Some(s) => (s as u64).max(1) * 1_000_000_000,
            None => {
                let w = self.cfg.fps_window_ms as u64 * 1_000_000;
                (2 * w).max(w + self.grace_ns())
            }
        }
    }

    /// Call once when a new batch of frames is about to be pushed (at `now_ns`), before pushing it.
    /// The age of the newest frame we held until now is how stale the data got between deliveries,
    /// which is how the tracker learns how late frames normally arrive.
    pub fn batch_arrived(&mut self, now_ns: u64) {
        let Some(&(newest, _)) = self.frames.back() else { return };
        self.lag.push_back((now_ns, now_ns.saturating_sub(newest)));
        while self.lag.front().is_some_and(|&(t, _)| now_ns.saturating_sub(t) > LAG_MEMORY_NS) {
            self.lag.pop_front();
        }
    }

    /// How long after its timestamp the newest frame may be before we call the game stalled.
    fn grace_ns(&self) -> u64 {
        let worst_lag = self.lag.iter().map(|&(_, l)| l).max().unwrap_or(0);
        // A game running at a very low frame rate legitimately goes a long time between frames.
        let slow_frames = self.frames.back().map_or(0, |&(_, ft)| (ft as f64 * 3.0e6) as u64);
        worst_lag.saturating_mul(2).max(slow_frames).clamp(MIN_GRACE_NS, MAX_GRACE_NS)
    }

    /// Record one frame. Invalid or pause-length frame times are ignored.
    pub fn push(&mut self, t_ns: u64, ft_ms: f64) {
        self.last_t = Some(t_ns);
        if !(ft_ms.is_finite() && ft_ms > 0.0 && ft_ms <= MAX_FRAME_MS) {
            return;
        }
        let ft = ft_ms as f32;
        self.frames.push_back((t_ns, ft));
        self.session.record(ft as f64);
        if self.cfg.stats_window_s.is_some() {
            self.window.record(ft as f64);
        }
        if self.pending.len() < 4096 {
            self.pending.push(ft);
        }
        self.total += 1;
    }

    /// Drop frames that fell out of the retention window.
    pub fn expire(&mut self, now_ns: u64) {
        let keep = self.keep_ns();
        while let Some(&(t, ft)) = self.frames.front() {
            if now_ns.saturating_sub(t) > keep {
                self.frames.pop_front();
                if self.cfg.stats_window_s.is_some() {
                    self.window.remove(ft as f64);
                }
            } else {
                break;
            }
        }
    }

    /// Frame times collected since the last call (for the live frame-time graph).
    pub fn take_pending(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.pending)
    }

    pub fn total_frames(&self) -> u64 {
        self.total
    }

    pub fn last_frame_ns(&self) -> Option<u64> {
        self.last_t
    }

    pub fn session_histogram(&self) -> &Histogram {
        &self.session
    }

    pub fn reset(&mut self) {
        let cfg = self.cfg;
        *self = FrameTracker::new(cfg);
    }

    pub fn stats(&mut self, now_ns: u64) -> FrameStats {
        self.expire(now_ns);
        let mut out = FrameStats { total_frames: self.total, ..Default::default() };

        // --- headline FPS over the short window -------------------------------
        // While frames keep arriving the window ends at the newest frame, not at "now": frames are
        // delivered in batches, and measuring up to "now" would count a quiet tail that is merely
        // still in transit as slow frames. Once nothing has arrived for longer than the grace period
        // the game really has stalled, and the window ends at "now" so FPS decays towards zero.
        let w_ns = self.cfg.fps_window_ms as u64 * 1_000_000;
        let grace = self.grace_ns();
        let anchored = self.frames.back().is_some_and(|&(t, _)| now_ns.saturating_sub(t) <= grace);
        let end_ns = match self.frames.back() {
            Some(&(t, _)) if anchored => t,
            _ => now_ns,
        };
        let cutoff = end_ns.saturating_sub(w_ns);
        let mut n = 0u64;
        let mut sum = 0.0f64;
        let mut max_ft = 0.0f32;
        let mut first_t = 0u64;
        let mut last_t = 0u64;
        for &(t, ft) in self.frames.iter().rev() {
            if t <= cutoff {
                break;
            }
            if n == 0 {
                last_t = t;
            }
            first_t = t;
            n += 1;
            sum += ft as f64;
            max_ft = max_ft.max(ft);
        }
        if n > 0 {
            let mean = sum / n as f64;
            let fps = if n >= 2 && anchored && last_t > first_t {
                (n - 1) as f64 / ((last_t - first_t) as f64 / 1e9)
            } else {
                n as f64 / (w_ns as f64 / 1e9)
            };
            out.fps = Some(fps);
            out.frametime = Some(mean);
            out.frametime_max = Some(max_ft as f64);
        } else if self.total > 0 {
            // The game was presenting before but nothing arrived in the window:
            // that is a genuine 0 FPS (frozen / not presenting).
            out.fps = Some(0.0);
        }

        // --- stats window ------------------------------------------------------
        let h = if self.cfg.stats_window_s.is_some() { &self.window } else { &self.session };
        out.frames_in_stats = h.count();
        if h.count() > 0 {
            out.fps_avg = h.mean_ms().map(ms_to_fps);
            out.fps_min = h.max_ms().map(ms_to_fps);
            out.fps_max = h.min_ms().map(ms_to_fps);
            out.low1 = low_fps(h, 0.01, self.cfg.lows_method, MIN_FRAMES_LOW1);
            out.low01 = low_fps(h, 0.001, self.cfg.lows_method, MIN_FRAMES_LOW01);
            out.p99_ms = h.percentile_ms(99.0);
        }
        out
    }
}

/// Keep at most `max_points` values by taking the max of each bucket, so a
/// spike is never smoothed away when drawing the frame-time graph.
pub fn decimate_max(values: &[f32], max_points: usize) -> Vec<f32> {
    if values.len() <= max_points || max_points == 0 {
        return values.to_vec();
    }
    let per = (values.len() as f64 / max_points as f64).ceil() as usize;
    values
        .chunks(per)
        .map(|c| c.iter().cloned().fold(f32::MIN, f32::max))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    fn feed(t: &mut FrameTracker, start_ns: u64, ft_ms: f64, n: usize) -> u64 {
        let mut now = start_ns;
        for _ in 0..n {
            now += (ft_ms * 1e6) as u64;
            t.push(now, ft_ms);
        }
        now
    }

    #[test]
    fn steady_60fps_is_reported_as_60() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let end = feed(&mut t, 0, 1000.0 / 60.0, 300);
        let s = t.stats(end);
        let fps = s.fps.unwrap();
        assert!((fps - 60.0).abs() < 0.2, "fps was {fps}");
        assert!((s.frametime.unwrap() - 16.667).abs() < 0.05);
        assert!((s.fps_avg.unwrap() - 60.0).abs() < 0.6);
    }

    /// Frames stamped every 10 ms, but only handed over once per `batch_ms`; the sampler looks every `tick_ms`.
    fn run_batched(batch_ms: u64, tick_ms: u64, total_s: u64, stop_frames_at_s: u64) -> Vec<(u64, Option<f64>)> {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let mut out = Vec::new();
        let mut next_frame = 10 * MS; // timestamp of the next frame to be created
        let mut pending: Vec<u64> = Vec::new(); // created but not yet delivered
        let mut now = 0u64;
        while now < total_s * 1000 * MS {
            now += tick_ms * MS;
            while next_frame <= now && next_frame <= stop_frames_at_s * 1000 * MS {
                pending.push(next_frame);
                next_frame += 10 * MS;
            }
            // The operating system hands over everything created before the last batch boundary.
            let boundary = (now / (batch_ms * MS)) * batch_ms * MS;
            let ready: Vec<u64> = pending.iter().copied().filter(|&f| f <= boundary).collect();
            pending.retain(|&f| f > boundary);
            if !ready.is_empty() {
                t.batch_arrived(now);
                for &f in &ready {
                    t.push(f, 10.0);
                }
            }
            out.push((now / MS, t.stats(now).fps));
        }
        out
    }

    #[test]
    fn batched_delivery_does_not_make_fps_sawtooth() {
        // Trace events arrive once a second, the overlay refreshes four times a second.
        for (batch, tick) in [(1000, 250), (1000, 500), (100, 250), (250, 100)] {
            let samples = run_batched(batch, tick, 25, 25);
            for (at_ms, fps) in samples.iter().filter(|(at, _)| *at > 6_000) {
                let fps = fps.unwrap_or(0.0);
                assert!((fps - 100.0).abs() < 3.0, "batch {batch} ms / tick {tick} ms: {fps:.1} FPS at {at_ms} ms");
            }
        }
    }

    #[test]
    fn a_freeze_still_shows_up_when_frames_arrive_in_batches() {
        let samples = run_batched(1000, 250, 30, 12); // frames stop after 12 s
        let at = |ms: u64| samples.iter().find(|(t, _)| *t == ms).unwrap().1.unwrap_or(0.0);
        assert!((at(11_000) - 100.0).abs() < 3.0);
        assert!(at(20_000) < 1.0, "8 s after the last frame the game must read as frozen, got {}", at(20_000));
        assert!(at(30_000) == 0.0);
    }

    #[test]
    fn a_very_slow_game_is_not_called_stalled_between_frames() {
        // 2 frames per second: a frame every 500 ms.
        let mut t = FrameTracker::new(TrackerConfig::default());
        let mut now = 0u64;
        for _ in 0..30 {
            now += 500 * MS;
            t.push(now, 500.0);
        }
        for extra in [0, 200, 400, 700] {
            let fps = t.stats(now + extra * MS).fps.unwrap();
            assert!((fps - 2.0).abs() < 0.3, "{fps} FPS {extra} ms after a frame");
        }
    }

    #[test]
    fn stalled_game_reports_zero_not_none() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let end = feed(&mut t, 0, 10.0, 200);
        let s = t.stats(end + 5_000 * MS);
        assert_eq!(s.fps, Some(0.0), "a frozen game is a genuine 0 FPS");
    }

    #[test]
    fn no_frames_ever_means_unavailable() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let s = t.stats(10_000 * MS);
        assert_eq!(s.fps, None);
        assert_eq!(s.fps_avg, None);
    }

    #[test]
    fn one_percent_low_average_vs_percentile() {
        // 990 fast frames (10 ms) and 10 slow ones (100 ms).
        let mut t = FrameTracker::new(TrackerConfig::default());
        let mut now = 0u64;
        for i in 0..1000 {
            let ft = if i % 100 == 50 { 100.0 } else { 10.0 };
            now += (ft * 1e6) as u64;
            t.push(now, ft);
        }
        let s = t.stats(now);
        let low1 = s.low1.unwrap();
        assert!((low1 - 10.0).abs() < 0.2, "avg of slowest 1% (10 frames of 100 ms) = 10 fps, got {low1}");
        let low01 = s.low01.unwrap();
        assert!((low01 - 10.0).abs() < 0.2, "got {low01}");

        let cfg = TrackerConfig { lows_method: LowsMethod::Percentile, ..TrackerConfig::default() };
        t.set_config(cfg);
        let p = t.stats(now).low1.unwrap();
        assert!((p - 100.0).abs() < 2.0, "99th percentile frame time is 10 ms => 100 fps, got {p}");
    }

    #[test]
    fn lows_need_enough_frames() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let end = feed(&mut t, 0, 16.0, 50);
        let s = t.stats(end);
        assert_eq!(s.low1, None, "50 frames is not enough for a 1% low");
        assert_eq!(s.low01, None);
        let end = feed(&mut t, end, 16.0, 100);
        let s = t.stats(end);
        assert!(s.low1.is_some());
        assert_eq!(s.low01, None, "still under 1000 frames");
    }

    #[test]
    fn min_and_max_fps_are_single_frame_extremes() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        let mut now = 0;
        for ft in [10.0, 10.0, 50.0, 5.0, 10.0] {
            now += (ft * 1e6) as u64;
            t.push(now, ft);
        }
        let s = t.stats(now);
        assert!((s.fps_min.unwrap() - 20.0).abs() < 0.2);
        assert!((s.fps_max.unwrap() - 200.0).abs() < 2.0);
    }

    #[test]
    fn pause_length_frames_are_ignored() {
        let mut t = FrameTracker::new(TrackerConfig::default());
        t.push(1, 20_000.0); // alt-tab for 20 s
        t.push(2, -3.0);
        t.push(3, f64::NAN);
        assert_eq!(t.session_histogram().count(), 0);
        assert!(t.take_pending().is_empty());
    }

    #[test]
    fn rolling_window_forgets_old_frames() {
        let cfg = TrackerConfig { stats_window_s: Some(10), ..TrackerConfig::default() };
        let mut t = FrameTracker::new(cfg);
        // 20 s at 100 fps (10 ms) then 12 s at 50 fps (20 ms).
        let mid = feed(&mut t, 0, 10.0, 2000);
        let end = feed(&mut t, mid, 20.0, 600);
        let s = t.stats(end);
        assert!((s.fps_avg.unwrap() - 50.0).abs() < 1.0, "old 100 fps data must age out, got {:?}", s.fps_avg);
        assert!(t.session_histogram().count() == 2600);
    }

    #[test]
    fn histogram_roundtrips_through_sparse_form() {
        let mut h = Histogram::new();
        for ms in [4.0, 8.0, 8.0, 16.7, 250.0] {
            h.record(ms);
        }
        let back = Histogram::from_sparse(&h.to_sparse());
        assert_eq!(back.count(), 5);
        assert!((back.mean_ms().unwrap() - h.mean_ms().unwrap()).abs() < 1e-9);
        assert_eq!(back.percentile_ms(100.0), h.percentile_ms(100.0));
    }

    #[test]
    fn decimate_keeps_spikes() {
        let mut v = vec![10.0f32; 1000];
        v[437] = 99.0;
        let d = decimate_max(&v, 100);
        assert!(d.len() <= 100);
        assert!(d.contains(&99.0));
    }
}
