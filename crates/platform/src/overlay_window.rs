//! Win32 tweaks for overlay windows. On other systems every call does nothing, so the native
//! shell compiles (and can be exercised) everywhere. `hwnd` is the raw window handle value.

#[cfg(windows)]
use crate::windows::overlay as imp;

#[cfg(windows)]
pub use imp::{assert_topmost, make_unobtrusive};

/// Hide (or stop hiding) the window from screen capture.
pub fn exclude_from_capture(hwnd: isize, exclude: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        imp::exclude_from_capture(hwnd, exclude)
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, exclude);
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn make_unobtrusive(_hwnd: isize) {}
#[cfg(not(windows))]
pub fn assert_topmost(_hwnd: isize) {}
