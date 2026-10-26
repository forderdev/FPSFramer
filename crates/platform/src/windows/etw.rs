//! FPS measurement through Event Tracing for Windows (ETW).
//!
//! We open a private real-time trace session, subscribe to the DXGI and Direct3D 9
//! "present" events and report one [`FrameEvent`] per presented frame. This is the
//! same public mechanism Intel PresentMon uses: it observes what the operating
//! system already logs. Nothing is injected into any game and no game memory is
//! read, so it does not interfere with anti-cheat software.
//!
//! Starting a real-time session requires administrator rights or membership of the
//! "Performance Log Users" group; if neither applies we report
//! [`CaptureState::NeedsPermission`] and the UI explains how to fix it.

use crate::capture::{CaptureState, FrameEvent, FrameSource};
use crate::clock::qpc_to_ns;
use crate::etw_parse::{parse_present, D3D9_PROVIDER, DXGI_PROVIDER};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_INVALID_PARAMETER, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW, CONTROLTRACE_HANDLE,
    EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_RECORD, EVENT_TRACE_CONTROL_FLUSH, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, PROCESSTRACE_HANDLE, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_RAW_TIMESTAMP, PROCESS_TRACE_MODE_REAL_TIME, TRACE_LEVEL_VERBOSE, WNODE_FLAG_TRACED_GUID,
};

const SESSION_NAME: &str = "FPSFramer-Capture";
const CHANNEL_CAPACITY: usize = 1 << 16;
/// Windows hands trace events to a real-time consumer a buffer at a time (at least once a second). Asking it
/// to flush this often keeps the live FPS number current; the engine copes with whatever delivery rhythm it gets.
const FLUSH_EVERY: Duration = Duration::from_millis(100);

#[repr(C)]
struct PropsBuf {
    props: EVENT_TRACE_PROPERTIES,
    /// The logger name lives directly after the properties struct.
    name: [u16; 64],
}

fn wide_name() -> [u16; 64] {
    let mut n = [0u16; 64];
    for (i, c) in SESSION_NAME.encode_utf16().enumerate().take(63) {
        n[i] = c;
    }
    n
}

/// `single_pool`: use one buffer pool for all processors so events arrive in timestamp order. Without it, events
/// from different processors can be delivered out of order, which would corrupt frame times (they are differences
/// of consecutive timestamps).
fn make_props(single_pool: bool) -> Box<PropsBuf> {
    // SAFETY: all-zero is a valid initial value for these plain-data structs.
    let mut b: Box<PropsBuf> = Box::new(unsafe { std::mem::zeroed() });
    b.name = wide_name();
    b.props.Wnode.BufferSize = std::mem::size_of::<PropsBuf>() as u32;
    b.props.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    b.props.Wnode.ClientContext = 1; // timestamps are QueryPerformanceCounter ticks
    b.props.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | if single_pool { EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING } else { 0 };
    b.props.FlushTimer = 1;
    b.props.LoggerNameOffset = std::mem::offset_of!(PropsBuf, name) as u32;
    b
}

/// Stop a session by name (also cleans up one left behind by a crashed run). Returns what Windows said.
fn stop_session_by_name() -> WIN32_ERROR {
    let mut props = make_props(false);
    let name = wide_name();
    // SAFETY: handle 0 + instance name selects the session; props buffer is large enough for the name.
    unsafe { ControlTraceW(CONTROLTRACE_HANDLE { Value: 0 }, PCWSTR(name.as_ptr()), &mut props.props, EVENT_TRACE_CONTROL_STOP) }
}

struct Ctx {
    tx: Sender<FrameEvent>,
    seen: Arc<AtomicU64>,
}

/// Called by ETW for every event on the consumer thread. Must stay tiny.
unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record; UserContext is the Ctx we registered and keep alive.
    let rec = &*rec;
    let ctx = &*(rec.UserContext as *const Ctx);
    let h = &rec.EventHeader;
    let provider = h.ProviderId.to_u128();
    if provider != DXGI_PROVIDER && provider != D3D9_PROVIDER {
        return;
    }
    let payload: &[u8] = if rec.UserData.is_null() || rec.UserDataLength == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(rec.UserData as *const u8, rec.UserDataLength as usize)
    };
    if let Some((chain, api)) = parse_present(provider, h.EventDescriptor.Id, h.Flags, payload) {
        if h.ProcessId <= 4 {
            return;
        }
        ctx.seen.fetch_add(1, Ordering::Relaxed);
        let _ = ctx.tx.try_send(FrameEvent { pid: h.ProcessId, chain, t_ns: qpc_to_ns(h.TimeStamp as u64), api });
    }
}

pub struct EtwSource {
    rx: Option<Receiver<FrameEvent>>,
    state: Arc<Mutex<CaptureState>>,
    seen: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl EtwSource {
    pub fn new() -> Self {
        Self { rx: None, state: Arc::new(Mutex::new(CaptureState::Stopped)), seen: Arc::new(AtomicU64::new(0)), thread: None }
    }
}

impl Default for EtwSource {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::field_reassign_with_default)]
fn session_thread(ctx: Box<Ctx>, ready: Sender<CaptureState>) {
    let name = wide_name();
    let mut props = make_props(true);
    let mut handle = CONTROLTRACE_HANDLE::default();

    // SAFETY: standard ETW session setup; every pointer refers to locals that outlive the calls.
    unsafe {
        let mut rc = StartTraceW(&mut handle, PCWSTR(name.as_ptr()), &mut props.props);
        // A session with this name is left over from a crashed run (stopping it is enough) or is still owned by
        // another running program. Try a few times, since a stop can take a moment to take effect.
        for _ in 0..5 {
            if rc != ERROR_ALREADY_EXISTS {
                break;
            }
            if stop_session_by_name() == ERROR_ACCESS_DENIED {
                // The session belongs to an elevated program (often a crashed run of this one) and we are not
                // allowed to stop it. That is a permission problem, not a broken trace session.
                log::warn!("ETW: the trace session \"{SESSION_NAME}\" exists and this process may not stop it.");
                rc = ERROR_ACCESS_DENIED;
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
            props = make_props(true);
            rc = StartTraceW(&mut handle, PCWSTR(name.as_ptr()), &mut props.props);
        }
        if rc == ERROR_ALREADY_EXISTS {
            log::warn!("ETW: the trace session \"{SESSION_NAME}\" still exists after stopping it five times; another program is probably recreating it.");
            let _ = ready.send(CaptureState::Error(
                "Another program is already using FPSFramer’s Windows trace session (code 183). Close any other copy of FPSFramer, including ones running in the background, then try again.".into(),
            ));
            return;
        }
        if rc == ERROR_INVALID_PARAMETER {
            // Be lenient if this Windows build rejects the single buffer pool: ordering is also restored by sorting.
            log::warn!("ETW: the single buffer pool was refused; starting without it.");
            props = make_props(false);
            rc = StartTraceW(&mut handle, PCWSTR(name.as_ptr()), &mut props.props);
        }
        if rc == ERROR_ACCESS_DENIED {
            let _ = ready.send(CaptureState::NeedsPermission);
            return;
        }
        if rc != ERROR_SUCCESS {
            let _ = ready.send(CaptureState::Error(format!("Could not start the Windows trace session (code {}).", rc.0)));
            return;
        }

        let level = TRACE_LEVEL_VERBOSE as u8;
        for (guid, required) in [(GUID::from_u128(DXGI_PROVIDER), true), (GUID::from_u128(D3D9_PROVIDER), false)] {
            let r = EnableTraceEx2(handle, &guid, EVENT_CONTROL_CODE_ENABLE_PROVIDER.0, level, u64::MAX, 0, 0, None);
            if r != ERROR_SUCCESS && required {
                let _ = ControlTraceW(handle, PCWSTR(name.as_ptr()), &mut props.props, EVENT_TRACE_CONTROL_STOP);
                let _ = ready.send(CaptureState::Error(format!("Could not subscribe to graphics events (code {}).", r.0)));
                return;
            }
        }

        let mut logger_name = name;
        let ctx_ptr = Box::into_raw(ctx);
        // The struct contains unions, so it is filled field by field after zero-initialisation.
        let mut logfile = EVENT_TRACE_LOGFILEW::default();
        logfile.LoggerName = PWSTR(logger_name.as_mut_ptr());
        logfile.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
        logfile.Anonymous2.EventRecordCallback = Some(on_event);
        logfile.Context = ctx_ptr as *mut c_void;
        let h: PROCESSTRACE_HANDLE = OpenTraceW(&mut logfile);
        if h.Value == u64::MAX {
            let _ = ControlTraceW(handle, PCWSTR(logger_name.as_ptr()), &mut props.props, EVENT_TRACE_CONTROL_STOP);
            drop(Box::from_raw(ctx_ptr));
            let _ = ready.send(CaptureState::Error("Could not open the Windows trace session for reading.".into()));
            return;
        }
        let _ = ready.send(CaptureState::Running);

        // Push buffered events to us promptly instead of waiting for Windows' own (1 s) flush.
        let stop_flush = Arc::new(AtomicBool::new(false));
        let flusher = {
            let stop = stop_flush.clone();
            std::thread::Builder::new()
                .name("fpsframer-etw-flush".into())
                .spawn(move || {
                    let name = wide_name();
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(FLUSH_EVERY);
                        let mut p = make_props(false);
                        // (Inside the enclosing `unsafe` block.) Name + zero handle select the session; the props buffer is
                        // large enough for the name.
                        let _ = ControlTraceW(CONTROLTRACE_HANDLE { Value: 0 }, PCWSTR(name.as_ptr()), &mut p.props, EVENT_TRACE_CONTROL_FLUSH);
                    }
                })
                .ok()
        };

        // Blocks until the session is stopped.
        let _ = ProcessTrace(&[h], None, None);

        stop_flush.store(true, Ordering::Relaxed);
        if let Some(f) = flusher {
            let _ = f.join();
        }
        let _ = CloseTrace(h);
        let _ = ControlTraceW(handle, PCWSTR(logger_name.as_ptr()), &mut props.props, EVENT_TRACE_CONTROL_STOP);
        drop(Box::from_raw(ctx_ptr));
    }
}

impl FrameSource for EtwSource {
    fn name(&self) -> &'static str {
        "etw"
    }

    fn start(&mut self) -> CaptureState {
        if self.thread.is_some() {
            return self.state();
        }
        let (tx, rx) = bounded::<FrameEvent>(CHANNEL_CAPACITY);
        let (ready_tx, ready_rx) = bounded::<CaptureState>(1);
        let ctx = Box::new(Ctx { tx, seen: self.seen.clone() });
        let t = std::thread::Builder::new().name("fpsframer-etw".into()).spawn(move || session_thread(ctx, ready_tx));
        let t = match t {
            Ok(t) => t,
            Err(e) => {
                *self.state.lock() = CaptureState::Error(format!("Could not start the capture thread: {e}"));
                return self.state();
            }
        };
        let st = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(CaptureState::Error("Windows did not respond when starting FPS measurement.".into()));
        if st == CaptureState::Running {
            self.rx = Some(rx);
            self.thread = Some(t);
        } else {
            let _ = t.join();
        }
        *self.state.lock() = st;
        self.state()
    }

    fn drain(&mut self, out: &mut Vec<FrameEvent>) {
        if let Some(rx) = &self.rx {
            while let Ok(e) = rx.try_recv() {
                out.push(e);
                if out.len() > 200_000 {
                    break;
                }
            }
        }
    }

    fn state(&self) -> CaptureState {
        self.state.lock().clone()
    }

    fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = stop_session_by_name();
            let _ = t.join();
        }
        self.rx = None;
        *self.state.lock() = CaptureState::Stopped;
    }

    fn events_seen(&self) -> u64 {
        self.seen.load(Ordering::Relaxed)
    }
}

impl Drop for EtwSource {
    fn drop(&mut self) {
        self.stop();
    }
}
