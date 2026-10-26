//! Screen capture with GDI. Captures the composed desktop of one monitor, which includes
//! windowed and borderless games. Exclusive full-screen content is not composed by Windows
//! and comes back black – the caller detects that.

use crate::screenshot::{Shot, ShotError};
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC, GetDIBits,
    GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW, SRCCOPY,
};

struct Mon {
    name: String,
    rect: RECT,
    primary: bool,
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let v = &mut *(data.0 as *mut Vec<Mon>);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(h, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
        let end = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
        v.push(Mon {
            name: String::from_utf16_lossy(&mi.szDevice[..end]),
            rect: mi.monitorInfo.rcMonitor,
            primary: mi.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        });
    }
    BOOL(1)
}

pub fn capture(monitor: Option<&str>) -> Result<Shot, ShotError> {
    let mut mons: Vec<Mon> = Vec::new();
    // SAFETY: the callback only touches the Vec we pass through LPARAM.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut mons as *mut Vec<Mon> as isize));
    }
    let m = monitor
        .and_then(|n| mons.iter().find(|m| m.name == n))
        .or_else(|| mons.iter().find(|m| m.primary))
        .or_else(|| mons.first())
        .ok_or_else(|| ShotError::Failed("no monitor found".into()))?;
    let (x, y) = (m.rect.left, m.rect.top);
    let (w, h) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
    if w <= 0 || h <= 0 {
        return Err(ShotError::Failed("monitor has no area".into()));
    }

    // SAFETY: classic GDI screen grab; every handle created here is released below.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(ShotError::Failed("could not access the screen".into()));
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let blt = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);

        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);

        if blt.is_err() || lines == 0 {
            return Err(ShotError::Failed("Windows refused to copy the screen".into()));
        }
        // BGRA → RGBA, opaque.
        // `as_chunks_mut` would need a newer Rust than the project's minimum.
        #[allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        Ok(Shot { width: w as u32, height: h as u32, rgba: buf })
    }
}
