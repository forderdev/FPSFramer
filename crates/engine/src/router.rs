//! Frame routing and game detection.
//!
//! Present events arrive from every program on the PC (browsers, the desktop
//! compositor, launchers…). The router keeps a short rate history per swap chain
//! and picks the one that is "the game": a program presenting at least 10 frames
//! per second that is not a known non-game, preferring the foreground window,
//! with stickiness so a background video call can't steal the overlay.

use fpsframer_platform::capture::{Api, FrameEvent};
use std::collections::{HashMap, VecDeque};

pub type ChainKey = (u32, u64);

const MIN_RATE: usize = 10;
const WINDOW_NS: u64 = 1_000_000_000;
/// A tracked program that stops presenting for this long is considered finished.
pub const STALL_DROP_NS: u64 = 8_000_000_000;

struct Chain {
    last_t: u64,
    recent: VecDeque<u64>,
    api: Api,
}

#[derive(Default)]
pub struct Router {
    chains: HashMap<ChainKey, Chain>,
    tracked: Option<ChainKey>,
    candidate: Option<(ChainKey, u32)>,
}

pub struct SelectCtx<'a> {
    pub now_ns: u64,
    pub foreground_pid: Option<u32>,
    /// pid → lower-case exe name, if known.
    pub exe_of: &'a dyn Fn(u32) -> Option<String>,
    /// Never a game (system, browser, launcher, user-ignored).
    pub excluded: &'a dyn Fn(&str) -> bool,
    /// Only measure this program (manual mode).
    pub manual: Option<&'a str>,
    /// Has its own profile.
    pub known: &'a dyn Fn(&str) -> bool,
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    Keep,
    /// Start measuring this swap chain (a previous one, if any, ended).
    Switch(ChainKey),
    /// The tracked program stopped presenting.
    Drop,
}

impl Router {
    #[cfg(test)]
    pub fn tracked(&self) -> Option<ChainKey> {
        self.tracked
    }

    pub fn api_of(&self, k: ChainKey) -> Option<Api> {
        self.chains.get(&k).map(|c| c.api)
    }

    pub fn clear_tracked(&mut self) {
        self.tracked = None;
        self.candidate = None;
    }

    /// Feed one event. Returns the frame time (ms) if it belongs to the tracked chain.
    pub fn ingest(&mut self, e: &FrameEvent) -> Option<(u64, f64)> {
        let key = (e.pid, e.chain);
        let c = self.chains.entry(key).or_insert(Chain { last_t: 0, recent: VecDeque::new(), api: e.api });
        let ft = if c.last_t > 0 && e.t_ns > c.last_t { Some((e.t_ns - c.last_t) as f64 / 1e6) } else { None };
        if e.t_ns >= c.last_t {
            c.last_t = e.t_ns;
        }
        c.api = e.api;
        c.recent.push_back(e.t_ns);
        while let Some(&f) = c.recent.front() {
            if e.t_ns.saturating_sub(f) > 2 * WINDOW_NS {
                c.recent.pop_front();
            } else {
                break;
            }
        }
        if self.tracked == Some(key) {
            ft.map(|ft| (e.t_ns, ft))
        } else {
            None
        }
    }

    fn rate(c: &Chain, now: u64) -> usize {
        c.recent.iter().rev().take_while(|&&t| now.saturating_sub(t) <= WINDOW_NS).count()
    }

    #[cfg(test)]
    pub fn rate_of(&self, k: ChainKey, now: u64) -> usize {
        self.chains.get(&k).map_or(0, |c| Self::rate(c, now))
    }

    /// Forget chains that have been silent for a while.
    pub fn prune(&mut self, now: u64) {
        let tracked = self.tracked;
        self.chains.retain(|k, c| Some(*k) == tracked || now.saturating_sub(c.last_t) < 30 * WINDOW_NS);
    }

    pub fn select(&mut self, ctx: &SelectCtx) -> Decision {
        // Has the tracked program finished?
        if let Some(t) = self.tracked {
            let dead = match self.chains.get(&t) {
                None => true,
                Some(c) => ctx.now_ns.saturating_sub(c.last_t) > STALL_DROP_NS,
            };
            if dead {
                self.tracked = None;
                self.candidate = None;
                return Decision::Drop;
            }
        }

        struct Cand {
            key: ChainKey,
            score: i64,
            fg: bool,
        }
        let mut best: Option<Cand> = None;
        for (&key, c) in &self.chains {
            let rate = Self::rate(c, ctx.now_ns);
            if rate < MIN_RATE {
                continue;
            }
            let Some(exe) = (ctx.exe_of)(key.0) else { continue };
            if (ctx.excluded)(&exe) {
                continue;
            }
            if let Some(m) = ctx.manual {
                if !exe.eq_ignore_ascii_case(m) {
                    continue;
                }
            }
            let fg = ctx.foreground_pid == Some(key.0);
            let mut score = rate as i64;
            if fg {
                score += 1000;
            }
            if self.tracked == Some(key) {
                score += 400;
            }
            if (ctx.known)(&exe) {
                score += 300;
            }
            if best.as_ref().is_none_or(|b| score > b.score || (score == b.score && key < b.key)) {
                best = Some(Cand { key, score, fg });
            }
        }
        let Some(best) = best else {
            self.candidate = None;
            return Decision::Keep;
        };
        if self.tracked == Some(best.key) {
            self.candidate = None;
            return Decision::Keep;
        }
        // Switching away from a program that is still presenting requires the new one to be
        // the foreground window; a background program never takes over.
        if let Some(t) = self.tracked {
            let tracked_alive = self.chains.get(&t).is_some_and(|c| Self::rate(c, ctx.now_ns) >= MIN_RATE);
            if tracked_alive && !best.fg {
                self.candidate = None;
                return Decision::Keep;
            }
        }
        let n = match self.candidate {
            Some((k, n)) if k == best.key => n + 1,
            _ => 1,
        };
        self.candidate = Some((best.key, n));
        let needed = if self.tracked.is_some() { 2 } else { 1 };
        if n >= needed {
            self.tracked = Some(best.key);
            self.candidate = None;
            return Decision::Switch(best.key);
        }
        Decision::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as Map;

    const MS: u64 = 1_000_000;

    fn run_frames(r: &mut Router, pid: u32, from: u64, fps: u64, secs: u64) -> u64 {
        let step = 1000 / fps * MS;
        let mut t = from;
        let end = from + secs * 1000 * MS;
        while t < end {
            r.ingest(&FrameEvent { pid, chain: 1, t_ns: t, api: Api::Dxgi });
            t += step;
        }
        t
    }

    fn ctx<'a>(
        now: u64,
        fg: Option<u32>,
        exes: &'a Map<u32, &'static str>,
        excluded: &'a dyn Fn(&str) -> bool,
        known: &'a dyn Fn(&str) -> bool,
        exe_of: &'a dyn Fn(u32) -> Option<String>,
    ) -> SelectCtx<'a> {
        let _ = exes;
        SelectCtx { now_ns: now, foreground_pid: fg, exe_of, excluded, manual: None, known }
    }

    fn setup() -> (Map<u32, &'static str>, impl Fn(&str) -> bool, impl Fn(&str) -> bool) {
        let mut m = Map::new();
        m.insert(100, "game.exe");
        m.insert(200, "chrome.exe");
        m.insert(300, "other.exe");
        (m, |e: &str| e == "chrome.exe" || e == "dwm.exe", |e: &str| e == "game.exe")
    }

    #[test]
    fn picks_the_game_and_ignores_a_browser_presenting_at_60fps() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        run_frames(&mut r, 200, 0, 60, 3);
        let now = run_frames(&mut r, 100, 0, 144, 3);
        let d = r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of));
        assert_eq!(d, Decision::Switch((100, 1)));
        assert_eq!(r.tracked(), Some((100, 1)));
    }

    #[test]
    fn slow_presenters_are_not_games() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        let now = run_frames(&mut r, 300, 0, 5, 4); // a 5 fps UI
        assert_eq!(r.select(&ctx(now, Some(300), &m, &excl, &known, &exe_of)), Decision::Keep);
        assert_eq!(r.tracked(), None);
    }

    #[test]
    fn background_program_does_not_steal_the_tracked_game() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        run_frames(&mut r, 100, 0, 90, 3);
        let now = run_frames(&mut r, 300, 0, 240, 3); // faster, but in the background
        r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of));
        assert_eq!(r.tracked().map(|k| k.0), Some(100));
        let d = r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of));
        assert_eq!(d, Decision::Keep);
        assert_eq!(r.tracked().map(|k| k.0), Some(100));
    }

    #[test]
    fn switching_to_the_foreground_program_takes_two_consecutive_selections() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        run_frames(&mut r, 100, 0, 90, 3);
        let now = run_frames(&mut r, 300, 0, 90, 3);
        r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of)); // locks onto 100 (foreground)
        assert_eq!(r.select(&ctx(now, Some(300), &m, &excl, &known, &exe_of)), Decision::Keep, "first sighting");
        assert_eq!(r.select(&ctx(now, Some(300), &m, &excl, &known, &exe_of)), Decision::Switch((300, 1)));
    }

    #[test]
    fn tracked_program_is_dropped_after_it_goes_silent() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        let now = run_frames(&mut r, 100, 0, 90, 3);
        r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of));
        assert!(r.tracked().is_some());
        assert_eq!(r.select(&ctx(now + 5_000 * MS, Some(100), &m, &excl, &known, &exe_of)), Decision::Keep, "5 s alt-tab is tolerated");
        assert_eq!(r.select(&ctx(now + 9_000 * MS, None, &m, &excl, &known, &exe_of)), Decision::Drop);
        assert!(r.tracked().is_none());
    }

    #[test]
    fn frame_times_only_come_from_the_tracked_chain() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        let now = run_frames(&mut r, 100, 0, 100, 2);
        r.select(&ctx(now, Some(100), &m, &excl, &known, &exe_of));
        assert!(r.ingest(&FrameEvent { pid: 100, chain: 1, t_ns: now + 10 * MS, api: Api::Dxgi }).is_some());
        let (_, ft) = r.ingest(&FrameEvent { pid: 100, chain: 1, t_ns: now + 20 * MS, api: Api::Dxgi }).unwrap();
        assert!((ft - 10.0).abs() < 0.01);
        assert!(r.ingest(&FrameEvent { pid: 300, chain: 1, t_ns: now, api: Api::Dxgi }).is_none());
    }

    #[test]
    fn manual_mode_only_measures_the_named_program() {
        let (m, excl, known) = setup();
        let exe_of = |p: u32| m.get(&p).map(|s| s.to_string());
        let mut r = Router::default();
        run_frames(&mut r, 100, 0, 144, 3);
        let now = run_frames(&mut r, 300, 0, 60, 3);
        let c = SelectCtx { now_ns: now, foreground_pid: Some(100), exe_of: &exe_of, excluded: &excl, manual: Some("other.exe"), known: &known };
        assert_eq!(r.select(&c), Decision::Switch((300, 1)));
    }

    #[test]
    fn two_swap_chains_in_one_process_are_kept_apart() {
        let mut r = Router::default();
        for i in 0..100u64 {
            r.ingest(&FrameEvent { pid: 100, chain: 1, t_ns: i * 10 * MS, api: Api::Dxgi });
            r.ingest(&FrameEvent { pid: 100, chain: 2, t_ns: i * 10 * MS + 3 * MS, api: Api::Dxgi });
        }
        assert_eq!(r.rate_of((100, 1), 990 * MS), 100);
        assert_eq!(r.rate_of((100, 2), 990 * MS), 100);
    }
}
