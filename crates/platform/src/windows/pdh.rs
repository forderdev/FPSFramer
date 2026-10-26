//! Thin wrapper over the Windows Performance Data Helper (PDH) API.
//!
//! Counters are added by their *English* path (`PdhAddEnglishCounterW`) so the app
//! works on every Windows display language.

use windows::core::PCWSTR;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

/// Do not clamp percentages to 100 (utility counters legitimately exceed it with turbo).
const NOCAP100: u32 = 0x0000_8000;
const FMT: PDH_FMT = PDH_FMT(PDH_FMT_DOUBLE.0 | NOCAP100);

const ERROR_SUCCESS: u32 = 0;
const PDH_MORE_DATA: u32 = 0x800007D2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct Pdh {
    query: PDH_HQUERY,
    counters: Vec<PDH_HCOUNTER>,
}

// SAFETY: a PDH query handle is just an opaque token; we only use it from one thread at a time.
unsafe impl Send for Pdh {}

impl Pdh {
    pub fn new() -> Option<Pdh> {
        let mut q = PDH_HQUERY(std::ptr::null_mut());
        // SAFETY: valid out pointer.
        let rc = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut q) };
        (rc == ERROR_SUCCESS).then(|| Pdh { query: q, counters: Vec::new() })
    }

    /// Add a counter by English path, e.g. `\Processor Information(*)\% Processor Utility`.
    /// Returns its index for [`value`](Self::value) / [`array`](Self::array).
    pub fn add(&mut self, path: &str) -> Option<usize> {
        let p = wide(path);
        let mut c = PDH_HCOUNTER(std::ptr::null_mut());
        // SAFETY: `p` is NUL-terminated and outlives the call.
        let rc = unsafe { PdhAddEnglishCounterW(self.query, PCWSTR(p.as_ptr()), 0, &mut c) };
        if rc == ERROR_SUCCESS {
            self.counters.push(c);
            Some(self.counters.len() - 1)
        } else {
            None
        }
    }

    pub fn collect(&mut self) -> bool {
        // SAFETY: query handle is valid until Drop.
        unsafe { PdhCollectQueryData(self.query) == ERROR_SUCCESS }
    }

    /// Single-instance counter value (None until a second sample exists).
    pub fn value(&self, idx: usize) -> Option<f64> {
        let c = *self.counters.get(idx)?;
        let mut v = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: valid handle and out pointer.
        let rc = unsafe { PdhGetFormattedCounterValue(c, FMT, None, &mut v) };
        if rc != ERROR_SUCCESS || v.CStatus > 1 {
            return None; // 0 = valid, 1 = new data; anything else is an error
        }
        // SAFETY: we asked for PDH_FMT_DOUBLE.
        let d = unsafe { v.Anonymous.doubleValue };
        d.is_finite().then_some(d)
    }

    /// Wildcard counter → `(instance name, value)` pairs.
    pub fn array(&self, idx: usize) -> Vec<(String, f64)> {
        let Some(&c) = self.counters.get(idx) else { return Vec::new() };
        let fmt = FMT;
        let mut size = 0u32;
        let mut count = 0u32;
        // SAFETY: first call only asks for the required size.
        let rc = unsafe { PdhGetFormattedCounterArrayW(c, fmt, &mut size, &mut count, None) };
        if rc != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // 8-byte aligned buffer for the item structs.
        let mut buf = vec![0u64; (size as usize).div_ceil(8) + 1];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        // SAFETY: buffer is at least `size` bytes and suitably aligned.
        let rc = unsafe { PdhGetFormattedCounterArrayW(c, fmt, &mut size, &mut count, Some(items)) };
        if rc != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // SAFETY: PDH filled `count` items; names point into the same buffer.
            let item = unsafe { &*items.add(i) };
            if item.FmtValue.CStatus > 1 {
                continue;
            }
            let name = unsafe { item.szName.to_string().unwrap_or_default() };
            let v = unsafe { item.FmtValue.Anonymous.doubleValue };
            if v.is_finite() {
                out.push((name, v));
            }
        }
        out
    }
}

impl Drop for Pdh {
    fn drop(&mut self) {
        // SAFETY: closes the handle opened in `new`.
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}
