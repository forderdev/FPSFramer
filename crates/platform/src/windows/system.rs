//! Elevation, shell-open and full-screen state on Windows.

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    CheckTokenMembership, GetTokenInformation, TokenElevation, TokenUser, PSID, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, ShellExecuteExW, ShellExecuteW, QUNS_RUNNING_D3D_FULL_SCREEN, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn shell_open(target: &str) -> Result<(), String> {
    let t = wide(target);
    // SAFETY: NUL-terminated wide strings that outlive the call.
    let r = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(t.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    // ShellExecute returns a value > 32 on success.
    if (r.0 as isize) > 32 {
        Ok(())
    } else {
        Err(format!("Windows could not open {target}."))
    }
}

pub fn is_elevated() -> bool {
    // SAFETY: standard token query on the current process.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

/// Is the current user (in this sign-in) a member of "Performance Log Users"?
/// Members may start the real-time trace sessions used for FPS measurement.
pub fn in_performance_log_users() -> bool {
    // SAFETY: converts a well-known SID string, checks membership, frees the SID.
    unsafe {
        let mut sid = PSID::default();
        if ConvertStringSidToSidW(w!("S-1-5-32-559"), &mut sid).is_err() {
            return false;
        }
        let mut member = windows::core::BOOL(0);
        let ok = CheckTokenMembership(None, sid, &mut member).is_ok();
        let _ = LocalFree(Some(HLOCAL(sid.0)));
        ok && member.as_bool()
    }
}

fn current_user_sid_string() -> Option<String> {
    // SAFETY: two-call TokenUser query, then converts the SID to text.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len.max(1) as usize + 8];
        let ok = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr() as *mut _), buf.len() as u32, &mut len).is_ok();
        let _ = CloseHandle(token);
        if !ok {
            return None;
        }
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(tu.User.Sid, &mut s).ok()?;
        let out = s.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(s.0 as *mut _)));
        out
    }
}

/// Run `exe args` with the "runas" verb (UAC prompt) and wait for it. Returns the exit code.
fn run_elevated_wait(exe: &str, args: &str) -> Result<u32, String> {
    let exe_w = wide(exe);
    let args_w = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(args_w.as_ptr()),
        nShow: 0, // SW_HIDE
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to live for the duration of the call.
    unsafe {
        ShellExecuteExW(&mut info).map_err(|_| "The administrator prompt was cancelled or blocked.".to_string())?;
        if info.hProcess.is_invalid() {
            return Ok(0);
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        Ok(code)
    }
}

pub fn grant_capture_permission() -> Result<(), String> {
    let sid = current_user_sid_string().ok_or("Could not determine your Windows account.")?;
    // SIDs only contain digits, 'S' and '-', but verify before putting it on a command line.
    if !sid.chars().all(|c| c.is_ascii_digit() || c == 'S' || c == '-') {
        return Err("Unexpected account identifier.".into());
    }
    let script = format!(
        "try {{ Add-LocalGroupMember -SID S-1-5-32-559 -Member '{sid}' -ErrorAction Stop; exit 0 }} catch {{ if ($_.FullyQualifiedErrorId -like '*MemberExists*') {{ exit 0 }} else {{ exit 1 }} }}"
    );
    let args = format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"{script}\"");
    match run_elevated_wait("powershell.exe", &args)? {
        0 => Ok(()),
        _ => Err("Windows did not allow the change.".into()),
    }
}

pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_w = wide(&exe.to_string_lossy());
    let args_w = wide("--elevated");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(args_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: as above.
    unsafe { ShellExecuteExW(&mut info).map_err(|_| "The administrator prompt was cancelled or blocked.".to_string()) }
}

pub fn exclusive_fullscreen() -> Option<bool> {
    // SAFETY: simple query.
    unsafe { SHQueryUserNotificationState().ok().map(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN) }
}
