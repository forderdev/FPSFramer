//! Monotonic nanosecond clock shared by frame capture and the sampler.
//!
//! On Windows this is the QueryPerformanceCounter timeline, which is the same
//! timeline ETW uses when a session is created with `ClientContext = 1`, so
//! frame timestamps and "now" are directly comparable.

#[cfg(windows)]
pub fn now_ns() -> u64 {
    use windows::Win32::System::Performance::QueryPerformanceCounter;
    let mut t = 0i64;
    // SAFETY: plain out-parameter write.
    unsafe {
        let _ = QueryPerformanceCounter(&mut t);
    }
    qpc_to_ns(t as u64)
}

#[cfg(windows)]
pub fn qpc_frequency() -> u64 {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    static FREQ: OnceLock<u64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: plain out-parameter write.
        unsafe {
            let _ = QueryPerformanceFrequency(&mut f);
        }
        (f as u64).max(1)
    })
}

/// Convert raw QPC ticks to nanoseconds without overflowing on long uptimes.
#[cfg(windows)]
pub fn qpc_to_ns(ticks: u64) -> u64 {
    ((ticks as u128 * 1_000_000_000u128) / qpc_frequency() as u128) as u64
}

#[cfg(not(windows))]
pub fn now_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1_000_000_000
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_is_monotonic() {
        let a = super::now_ns();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = super::now_ns();
        assert!(b > a);
    }
}
