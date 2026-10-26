//! Win32 tweaks for the transparent overlay windows.
//!
//! The UI toolkit creates the windows; these helpers add the few behaviours it has no
//! cross-platform switch for. Every function takes the raw `HWND` value so the caller does not
//! need to share a `windows` crate version with this one.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
    SET_WINDOW_POS_FLAGS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

fn h(hwnd: isize) -> HWND {
    HWND(hwnd as *mut core::ffi::c_void)
}

/// Never take keyboard focus from the game and stay out of Alt-Tab.
pub fn make_unobtrusive(hwnd: isize) {
    // SAFETY: plain style read-modify-write on a window owned by this process.
    unsafe {
        let ex = GetWindowLongPtrW(h(hwnd), GWL_EXSTYLE) as u32;
        // The toolkit rewrites the extended style when other flags change and can leave WS_EX_APPWINDOW set,
        // which puts the overlay on the taskbar and in Alt-Tab; call this again after such changes.
        let wanted = (ex | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
        if wanted != ex {
            SetWindowLongPtrW(h(hwnd), GWL_EXSTYLE, wanted as isize);
            let _ = SetWindowPos(
                h(hwnd),
                None,
                0,
                0,
                0,
                0,
                SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOZORDER.0 | SWP_NOACTIVATE.0 | SWP_FRAMECHANGED.0),
            );
        }
    }
}

/// Put the window back above other topmost windows without activating it.
pub fn assert_topmost(hwnd: isize) {
    // SAFETY: SetWindowPos with SWP_NOACTIVATE on a window of this process.
    unsafe {
        let _ = SetWindowPos(h(hwnd), Some(HWND_TOPMOST), 0, 0, 0, 0, SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0));
    }
}

/// Ask Windows to leave the window out of screenshots, recordings and streams.
/// Needs Windows 10 version 2004 or newer; older versions return an error.
pub fn exclude_from_capture(hwnd: isize, exclude: bool) -> Result<(), String> {
    // SAFETY: documented API; the window belongs to this process.
    unsafe { SetWindowDisplayAffinity(h(hwnd), if exclude { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE }) }
        .map_err(|e| format!("Windows refused to hide the overlay from captures ({e}). This needs Windows 10 version 2004 or newer."))
}
