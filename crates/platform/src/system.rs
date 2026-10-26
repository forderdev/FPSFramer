//! Small operating-system helpers: elevation, opening links and folders.

/// Links the app is allowed to open. Anything else is refused.
pub fn url_is_allowed(url: &str) -> bool {
    let u = url.trim().to_lowercase();
    u.starts_with("https://") || u.starts_with("ms-settings:")
}

pub fn open_url(url: &str) -> Result<(), String> {
    if !url_is_allowed(url) {
        return Err("This kind of link can't be opened from FPSFramer.".into());
    }
    open_target(url)
}

/// Open a folder or file in the system file manager / default app.
pub fn open_path(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} does not exist yet.", path.display()));
    }
    open_target(&path.to_string_lossy())
}

fn open_target(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::windows::system::shell_open(target)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open {target}: {e}"))
    }
}

/// Whether this process has administrator rights.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        crate::windows::system::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Start another copy of this program with administrator rights (shows the Windows UAC prompt).
pub fn relaunch_elevated() -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::windows::system::relaunch_elevated()
    }
    #[cfg(not(windows))]
    {
        Err("Only available on Windows.".into())
    }
}

/// One-time permission so FPS capture works without running as administrator
/// (adds the user to "Performance Log Users"; takes effect after signing out and in).
pub fn grant_capture_permission() -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::windows::system::grant_capture_permission()
    }
    #[cfg(not(windows))]
    {
        Err("Only available on Windows.".into())
    }
}

/// `Some(true)` when Windows reports an exclusive-fullscreen Direct3D application.
pub fn exclusive_fullscreen() -> Option<bool> {
    #[cfg(windows)]
    {
        crate::windows::system::exclusive_fullscreen()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Known anti-cheat services/processes currently present (informational only).
pub fn detect_anticheat(process_names: &[String]) -> Vec<&'static str> {
    const KNOWN: &[(&str, &str)] = &[
        ("easyanticheat", "Easy Anti-Cheat"),
        ("easyanticheat_eos", "Easy Anti-Cheat"),
        ("beservice", "BattlEye"),
        ("vgc", "Riot Vanguard"),
        ("vgtray", "Riot Vanguard"),
        ("faceitclient", "FACEIT"),
        ("faceit", "FACEIT"),
        ("esportal", "Esportal"),
        ("xigncode", "XIGNCODE3"),
        ("gameguard", "nProtect GameGuard"),
        ("npggnt", "nProtect GameGuard"),
        ("aliceanticheat", "AntiCheat Expert"),
    ];
    let mut out: Vec<&'static str> = Vec::new();
    for p in process_names {
        let l = p.to_lowercase();
        let l = l.trim_end_matches(".exe");
        for (k, label) in KNOWN {
            if (l == *k || l.starts_with(k))
                && !out.contains(label) {
                    out.push(label);
                }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_safe_links_may_be_opened() {
        assert!(url_is_allowed("https://example.com"));
        assert!(url_is_allowed("ms-settings:display-advancedgraphics"));
        assert!(!url_is_allowed("file:///C:/Windows/System32/cmd.exe"));
        assert!(!url_is_allowed("javascript:alert(1)"));
        assert!(!url_is_allowed("calc.exe"));
        assert!(open_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn anticheat_detection() {
        let procs = vec!["EasyAntiCheat.exe".to_string(), "chrome.exe".into(), "vgc.exe".into(), "BEService.exe".into()];
        let found = detect_anticheat(&procs);
        assert!(found.contains(&"Easy Anti-Cheat") && found.contains(&"Riot Vanguard") && found.contains(&"BattlEye"));
        assert_eq!(found.len(), 3);
    }
}
