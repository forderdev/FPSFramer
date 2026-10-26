//! Process list, foreground window and loaded graphics modules on Windows.

use crate::proc::{ForegroundInfo, ProcInfo, ProcessSource};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW, MODULEENTRY32W,
    PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};

pub struct WindowsProcs {
    /// pid → exe cache so the foreground lookup doesn't re-snapshot every time.
    cache: Vec<ProcInfo>,
}

impl WindowsProcs {
    pub fn new() -> Self {
        Self { cache: Vec::new() }
    }
}

impl Default for WindowsProcs {
    fn default() -> Self {
        Self::new()
    }
}

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

fn snapshot_processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    // SAFETY: standard ToolHelp enumeration; the handle is closed below.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                out.push(ProcInfo { pid: e.th32ProcessID, exe: wide_to_string(&e.szExeFile).to_lowercase() });
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

fn exe_of(pid: u32) -> Option<String> {
    // SAFETY: opens a limited-rights handle, reads the image name, closes it.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_lowercase())
    }
}

impl ProcessSource for WindowsProcs {
    fn list(&mut self) -> Vec<ProcInfo> {
        self.cache = snapshot_processes();
        self.cache.clone()
    }

    fn foreground(&mut self) -> Option<ForegroundInfo> {
        // SAFETY: plain Win32 window queries.
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let exe = self
                .cache
                .iter()
                .find(|p| p.pid == pid)
                .map(|p| p.exe.clone())
                .or_else(|| exe_of(pid))?;
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFOEXW::default();
            mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let mut monitor = None;
            let mut fullscreen = false;
            if GetMonitorInfoW(mon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
                monitor = Some(wide_to_string(&mi.szDevice));
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r).is_ok() {
                    let m = mi.monitorInfo.rcMonitor;
                    fullscreen = r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom;
                }
            }
            Some(ForegroundInfo { pid, exe, fullscreen, monitor })
        }
    }

    fn graphics_modules(&mut self, pid: u32) -> Vec<String> {
        const INTERESTING: [&str; 9] = [
            "d3d9.dll", "d3d10.dll", "d3d10_1.dll", "d3d10core.dll", "d3d11.dll", "d3d12.dll", "dxgi.dll", "vulkan-1.dll", "opengl32.dll",
        ];
        let mut out = Vec::new();
        // SAFETY: module enumeration of another process; failure (e.g. protected process) just yields nothing.
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) else { return out };
            let mut e = MODULEENTRY32W { dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
            if Module32FirstW(snap, &mut e).is_ok() {
                loop {
                    let name = wide_to_string(&e.szModule).to_lowercase();
                    if INTERESTING.contains(&name.as_str()) && !out.contains(&name) {
                        out.push(name);
                    }
                    if Module32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    fn exclusive_fullscreen(&mut self) -> Option<bool> {
        super::system::exclusive_fullscreen()
    }
}
