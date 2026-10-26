//! Frame capture: where FPS comes from.
//!
//! A [`FrameSource`] reports one [`FrameEvent`] per presented frame. On Windows
//! the real source listens to the operating system's own graphics events (ETW) –
//! the same mechanism Intel PresentMon uses. **Nothing is injected into games and
//! no game memory is read**, which is why it is compatible with anti-cheat
//! software; the trade-off is that only DirectX 9/10/11/12 (DXGI) presents are
//! visible.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    Dxgi,
    D3d9,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameEvent {
    pub pid: u32,
    /// Identifies the swap chain, so two windows in one process aren't merged.
    pub chain: u64,
    /// Monotonic nanoseconds ([`crate::clock::now_ns`] timeline).
    pub t_ns: u64,
    pub api: Api,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureState {
    Stopped,
    Running,
    /// Windows refused to start the trace: needs administrator rights or the
    /// "Performance Log Users" group.
    NeedsPermission,
    Unsupported(String),
    Error(String),
}

pub trait FrameSource: Send {
    fn name(&self) -> &'static str;
    fn start(&mut self) -> CaptureState;
    /// Move all pending frame events into `out` without blocking.
    fn drain(&mut self, out: &mut Vec<FrameEvent>);
    fn state(&self) -> CaptureState;
    fn stop(&mut self);
    /// Total events received since start (for diagnostics).
    fn events_seen(&self) -> u64;
}

/// Used where FPS capture does not exist (non-Windows builds).
pub struct UnsupportedSource {
    reason: String,
}

impl UnsupportedSource {
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}

impl FrameSource for UnsupportedSource {
    fn name(&self) -> &'static str {
        "unsupported"
    }
    fn start(&mut self) -> CaptureState {
        self.state()
    }
    fn drain(&mut self, _out: &mut Vec<FrameEvent>) {}
    fn state(&self) -> CaptureState {
        CaptureState::Unsupported(self.reason.clone())
    }
    fn stop(&mut self) {}
    fn events_seen(&self) -> u64 {
        0
    }
}

/// The default frame source for this platform.
pub fn system_source() -> Box<dyn FrameSource> {
    #[cfg(windows)]
    {
        Box::new(crate::windows::etw::EtwSource::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(UnsupportedSource::new("FPS measurement is only available on Windows."))
    }
}
