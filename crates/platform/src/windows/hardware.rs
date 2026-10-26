//! "What PC is this?" on Windows.

use fpsframer_core::session::HardwareInfo;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// `["2560×1440 @ 165 Hz", …]` for every attached display.
pub fn displays() -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    // SAFETY: documented EnumDisplayDevices / EnumDisplaySettings loop with correctly sized structs.
    unsafe {
        loop {
            let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            if !EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0).as_bool() {
                break;
            }
            i += 1;
            const ATTACHED: u32 = 0x1; // DISPLAY_DEVICE_ATTACHED_TO_DESKTOP
            if dd.StateFlags.0 & ATTACHED == 0 {
                continue;
            }
            let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
            if EnumDisplaySettingsW(PCWSTR(dd.DeviceName.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm).as_bool() {
                out.push(format!("{}×{} @ {} Hz", dm.dmPelsWidth, dm.dmPelsHeight, dm.dmDisplayFrequency));
            }
        }
    }
    out
}

fn os_name() -> String {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion") else {
        return "Windows".into();
    };
    let build: u32 = k.get_value::<String, _>("CurrentBuild").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ver: String = k.get_value("DisplayVersion").unwrap_or_default();
    let family = if build >= 22000 { "Windows 11" } else { "Windows 10" };
    if ver.is_empty() {
        format!("{family} (build {build})")
    } else {
        format!("{family} {ver} (build {build})")
    }
}

pub fn collect(gpu_names: &[String]) -> HardwareInfo {
    let cpu = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0")
        .ok()
        .and_then(|k| k.get_value::<String, _>("ProcessorNameString").ok())
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| "Unknown CPU".into());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: dwLength set as required.
    let ram = if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() { m.ullTotalPhys as f64 / 1_073_741_824.0 } else { 0.0 };
    let gpus = if gpu_names.is_empty() { super::gpu::adapter_names() } else { gpu_names.to_vec() };
    HardwareInfo {
        cpu: format!("{cpu} ({threads} threads)"),
        gpus,
        ram_gb: (ram * 10.0).round() / 10.0,
        os: os_name(),
        displays: displays(),
        app_version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[allow(dead_code)]
fn _keep(_: &[u16]) -> String {
    wide_to_string(&[])
}
