//! Demo mode: a scripted, **clearly labelled** simulation.
//!
//! It exists so the app can be explored (and tested) on a machine without a game
//! or without supported sensors. Everything it produces is fake; the engine
//! stamps every snapshot with `demo: true`, and the UI and overlay show a
//! "DEMO DATA" marker. It is never enabled unless the user asks for it
//! (`--demo` or Settings → Advanced).

use crate::capture::{Api, CaptureState, FrameEvent, FrameSource};
use crate::catalog;
use crate::clock::now_ns;
use crate::proc::{ForegroundInfo, ProcInfo, ProcessSource};
use crate::provider::{PollOutcome, SensorProvider};
use fpsframer_core::sensors::{ids, Readings, ReadingsExt, SensorDesc, Unit};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub const DEMO_PID: u32 = 424_242;
pub const DEMO_EXE: &str = "demogame.exe";
const SRC: &str = "demo";
const CORES: usize = 8;

pub struct DemoWorld {
    running: AtomicBool,
    started_ns: AtomicU64,
}

impl DemoWorld {
    pub fn new(auto_start: bool) -> Arc<Self> {
        let w = Arc::new(Self { running: AtomicBool::new(false), started_ns: AtomicU64::new(0) });
        if auto_start {
            w.set_running(true);
        }
        w
    }
    pub fn set_running(&self, on: bool) {
        if on && !self.is_running() {
            self.started_ns.store(now_ns(), Ordering::SeqCst);
        }
        self.running.store(on, Ordering::SeqCst);
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    /// Seconds since the demo game started.
    pub fn game_time(&self) -> f64 {
        now_ns().saturating_sub(self.started_ns.load(Ordering::SeqCst)) as f64 / 1e9
    }
}

fn noise(x: f64) -> f64 {
    ((x * 12.9898).sin() * 43758.5453).fract().abs() * 2.0 - 1.0
}

/// Target frame rate of the scripted game at game-time `t`.
fn base_fps(t: f64) -> f64 {
    let cycle = t % 120.0;
    let heavy = (40.0..52.0).contains(&cycle);
    let wobble = 14.0 * (t / 17.0).sin() + 4.0 * (t / 3.1).sin();
    if heavy {
        56.0 + wobble * 0.4
    } else {
        96.0 + wobble
    }
}

const HITCHES: [f64; 4] = [18.0, 41.5, 77.0, 103.0];

// ───────────────────────────── sensors ─────────────────────────────

pub struct DemoProvider {
    world: Arc<DemoWorld>,
    idle_t0: u64,
}

impl DemoProvider {
    pub fn new(world: Arc<DemoWorld>) -> Self {
        Self { world, idle_t0: now_ns() }
    }
}

impl SensorProvider for DemoProvider {
    fn id(&self) -> &'static str {
        "demo"
    }
    fn label(&self) -> &'static str {
        "Demo data (simulated)"
    }
    fn descriptors(&self) -> Vec<SensorDesc> {
        let mut v = vec![catalog::cpu_load(SRC), catalog::cpu_core_max(SRC), catalog::cpu_clock(SRC), catalog::cpu_name(SRC), catalog::cpu_temp(SRC), catalog::cpu_power(SRC)];
        for i in 0..CORES {
            v.push(catalog::cpu_core_load(i, SRC));
        }
        v.extend(catalog::ram_set(SRC));
        v.extend(catalog::disk_set(SRC));
        v.extend(catalog::net_set(SRC));
        for f in catalog::GPU_FIELDS {
            v.push(catalog::gpu_desc(0, "Demo GPU", false, f, SRC));
        }
        v.push(catalog::gpu_text_desc(0, "Demo GPU", false, "name", "model", "The name of your graphics card.", SRC));
        v.push(catalog::gpu_text_desc(0, "Demo GPU", false, "throttle", "slowdown reason", "Whether the graphics driver is currently slowing the card down, and why.", SRC));
        v
    }

    fn poll(&mut self, out: &mut Readings) -> PollOutcome {
        let playing = self.world.is_running();
        let wall = (now_ns() - self.idle_t0) as f64 / 1e9;
        let t = if playing { self.world.game_time() } else { 0.0 };
        let heavy = playing && (40.0..52.0).contains(&(t % 120.0));
        let warm = if playing { 1.0 - (-t / 90.0).exp() } else { 0.0 };
        let n = |k: f64| noise(wall * 1.7 + k);

        let gpu_load = if playing { (96.0 + 3.0 * n(1.0)).clamp(0.0, 100.0) } else { 4.0 + 3.0 * n(1.0).abs() };
        let cpu_load = if playing { 36.0 + if heavy { 6.0 } else { 0.0 } + 6.0 * n(2.0) } else { 6.0 + 4.0 * n(2.0).abs() };
        out.put(ids::CPU_LOAD, cpu_load, Unit::Percent);
        let mut cmax: f64 = 0.0;
        for i in 0..CORES {
            let base = if playing { [88.0, 62.0, 40.0, 33.0, 28.0, 25.0, 22.0, 20.0][i] } else { 5.0 };
            let v = (base + 7.0 * n(10.0 + i as f64)).clamp(0.0, 100.0);
            cmax = cmax.max(v);
            out.put(&format!("cpu.core.{i}.load"), v, Unit::Percent);
        }
        out.put(ids::CPU_CORE_MAX, cmax, Unit::Percent);
        out.put(ids::CPU_CLOCK, if playing { 4650.0 + 80.0 * n(3.0) } else { 1900.0 + 400.0 * n(3.0).abs() }, Unit::Mhz);
        out.put_text(ids::CPU_NAME, "Demo CPU 8-Core Processor");
        out.put(ids::CPU_TEMP, 38.0 + 36.0 * warm + 2.0 * n(4.0), Unit::Celsius);
        out.put(ids::CPU_POWER, if playing { 78.0 + 6.0 * n(5.0) } else { 14.0 + 2.0 * n(5.0).abs() }, Unit::Watt);

        let ram_total = 32768.0;
        let ram_pct = if playing { 52.0 + 5.0 * warm } else { 36.0 } + 0.5 * n(6.0);
        out.put(ids::RAM_TOTAL, ram_total, Unit::Megabytes);
        out.put(ids::RAM_PERCENT, ram_pct, Unit::Percent);
        out.put(ids::RAM_USED, ram_total * ram_pct / 100.0, Unit::Megabytes);
        out.put(ids::RAM_AVAILABLE, ram_total * (100.0 - ram_pct) / 100.0, Unit::Megabytes);

        let loading = playing && t < 12.0;
        out.put(ids::DISK_READ, if loading { 900e6 + 100e6 * n(7.0) } else if playing { 4e6 * (1.0 + n(7.0)).max(0.0) } else { 0.2e6 * (1.0 + n(7.0)).max(0.0) }, Unit::BytesPerSec);
        out.put(ids::DISK_WRITE, 1.5e6 * (1.0 + n(8.0)).max(0.0), Unit::BytesPerSec);
        out.put(ids::DISK_ACTIVE, if loading { 85.0 } else { 3.0 + 2.0 * n(9.0).abs() }, Unit::Percent);
        out.put(ids::DISK_LATENCY, if loading { 1.8 } else { 0.4 + 0.2 * n(9.5).abs() }, Unit::Ms);
        out.put(ids::NET_DOWN, if playing { 0.4e6 } else { 0.05e6 } * (1.0 + 0.5 * n(11.0)), Unit::BytesPerSec);
        out.put(ids::NET_UP, if playing { 0.12e6 } else { 0.01e6 } * (1.0 + 0.5 * n(12.0)), Unit::BytesPerSec);

        out.put("gpu.load", gpu_load, Unit::Percent);
        let gtemp = 36.0 + 44.0 * warm + 1.5 * n(13.0);
        out.put("gpu.temp", gtemp, Unit::Celsius);
        out.put("gpu.clock", if playing { 2650.0 - 25.0 * warm + 15.0 * n(14.0) } else { 210.0 }, Unit::Mhz);
        out.put("gpu.memclock", if playing { 10_500.0 } else { 405.0 }, Unit::Mhz);
        out.put("gpu.power", if playing { 205.0 + 10.0 * n(15.0) } else { 22.0 }, Unit::Watt);
        out.put("gpu.fan", if playing { 35.0 + 40.0 * warm } else { 0.0 }, Unit::Percent);
        let vram_total = 12288.0;
        let vram = if playing { 6800.0 + 600.0 * warm } else { 700.0 };
        out.put("gpu.vram.used", vram, Unit::Megabytes);
        out.put("gpu.vram.total", vram_total, Unit::Megabytes);
        out.put("gpu.vram.percent", vram * 100.0 / vram_total, Unit::Percent);
        out.put("gpu.throttle.thermal", 0.0, Unit::Count);
        out.put("gpu.throttle.power", 0.0, Unit::Count);
        out.put_text("gpu.throttle", "None");
        out.put_text("gpu.name", "Demo GPU (simulated)");
        PollOutcome::ok()
    }
}

// ───────────────────────────── frames ─────────────────────────────

pub struct DemoFrames {
    world: Arc<DemoWorld>,
    last_t: f64,
    next_ns: u64,
    frame_no: u64,
    state: CaptureState,
    seen: u64,
}

impl DemoFrames {
    pub fn new(world: Arc<DemoWorld>) -> Self {
        Self { world, last_t: 0.0, next_ns: 0, frame_no: 0, state: CaptureState::Stopped, seen: 0 }
    }
}

impl FrameSource for DemoFrames {
    fn name(&self) -> &'static str {
        "demo"
    }
    fn start(&mut self) -> CaptureState {
        self.state = CaptureState::Running;
        self.state.clone()
    }
    fn drain(&mut self, out: &mut Vec<FrameEvent>) {
        if !self.world.is_running() {
            self.next_ns = 0;
            self.last_t = 0.0;
            return;
        }
        let now = now_ns();
        if self.next_ns == 0 {
            self.next_ns = now;
            self.last_t = self.world.game_time();
        }
        let mut guard = 0;
        while self.next_ns <= now && guard < 4000 {
            guard += 1;
            let t = self.world.game_time() - (now - self.next_ns) as f64 / 1e9;
            let mut ft = 1000.0 / base_fps(t) * (1.0 + 0.06 * noise(self.frame_no as f64 * 0.37));
            if let Some(_h) = HITCHES.iter().find(|h| {
                let c = t % 120.0;
                let p = self.last_t % 120.0;
                p < **h && **h <= c
            }) {
                ft = 90.0 + 40.0 * noise(self.frame_no as f64).abs();
            }
            self.last_t = t;
            self.frame_no += 1;
            self.next_ns += (ft * 1e6) as u64;
            self.seen += 1;
            out.push(FrameEvent { pid: DEMO_PID, chain: 1, t_ns: self.next_ns, api: Api::Dxgi });
        }
    }
    fn state(&self) -> CaptureState {
        self.state.clone()
    }
    fn stop(&mut self) {
        self.state = CaptureState::Stopped;
    }
    fn events_seen(&self) -> u64 {
        self.seen
    }
}

// ───────────────────────────── processes ─────────────────────────────

pub struct DemoProcs {
    world: Arc<DemoWorld>,
    inner: Box<dyn ProcessSource>,
}

impl DemoProcs {
    pub fn new(world: Arc<DemoWorld>, inner: Box<dyn ProcessSource>) -> Self {
        Self { world, inner }
    }
}

impl ProcessSource for DemoProcs {
    fn list(&mut self) -> Vec<ProcInfo> {
        let mut v = self.inner.list();
        if self.world.is_running() {
            v.push(ProcInfo { pid: DEMO_PID, exe: DEMO_EXE.into() });
        }
        v
    }
    fn foreground(&mut self) -> Option<ForegroundInfo> {
        if self.world.is_running() {
            Some(ForegroundInfo { pid: DEMO_PID, exe: DEMO_EXE.into(), fullscreen: true, monitor: None })
        } else {
            None
        }
    }
    fn graphics_modules(&mut self, pid: u32) -> Vec<String> {
        if pid == DEMO_PID {
            vec!["d3d12.dll".into(), "dxgi.dll".into()]
        } else {
            self.inner.graphics_modules(pid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_frames_only_flow_while_the_demo_game_runs() {
        let w = DemoWorld::new(false);
        let mut f = DemoFrames::new(w.clone());
        f.start();
        let mut out = Vec::new();
        f.drain(&mut out);
        assert!(out.is_empty());
        w.set_running(true);
        f.drain(&mut out); // first call anchors the clock
        std::thread::sleep(std::time::Duration::from_millis(250));
        f.drain(&mut out);
        assert!(out.len() >= 10, "got {} frames in 250 ms", out.len());
        assert!(out.windows(2).all(|p| p[1].t_ns >= p[0].t_ns));
        assert!(out.iter().all(|e| e.pid == DEMO_PID));
    }

    #[test]
    fn demo_sensors_are_labelled_and_idle_vs_playing_differ() {
        let w = DemoWorld::new(false);
        let mut p = DemoProvider::new(w.clone());
        assert!(p.descriptors().iter().all(|d| d.source == "demo"));
        let mut idle = Readings::new();
        p.poll(&mut idle);
        w.set_running(true);
        let mut play = Readings::new();
        p.poll(&mut play);
        assert!(play.num("gpu.load").unwrap() > idle.num("gpu.load").unwrap() + 50.0);
        assert!(play.num("gpu.vram.used").unwrap() > idle.num("gpu.vram.used").unwrap());
    }

    #[test]
    fn scripted_game_has_a_heavy_scene() {
        assert!(base_fps(45.0) < 70.0);
        assert!(base_fps(10.0) > 80.0);
    }
}
