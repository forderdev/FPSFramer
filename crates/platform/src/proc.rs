//! Running processes and the foreground window – used to recognise games.

use crate::capture::Api;
#[cfg(not(windows))]
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    /// Lower-case executable file name, e.g. `cs2.exe`.
    pub exe: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForegroundInfo {
    pub pid: u32,
    pub exe: String,
    /// The window covers its whole monitor (full-screen or borderless).
    pub fullscreen: bool,
    /// Name of the monitor it is on (`\\.\DISPLAY1`).
    pub monitor: Option<String>,
}

pub trait ProcessSource: Send {
    fn list(&mut self) -> Vec<ProcInfo>;
    fn foreground(&mut self) -> Option<ForegroundInfo>;
    /// Graphics-related DLL names loaded by `pid` (lower-case), if they can be listed.
    fn graphics_modules(&mut self, pid: u32) -> Vec<String>;
    /// Windows' own view: is an exclusive full-screen Direct3D application running?
    fn exclusive_fullscreen(&mut self) -> Option<bool> {
        None
    }
}

/// Programs that present frames but are never "the game".
const NOT_GAMES: &[&str] = &[
    "dwm.exe", "explorer.exe", "fpsframer.exe", "msedgewebview2.exe", "searchhost.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "applicationframehost.exe", "textinputhost.exe", "widgets.exe", "taskmgr.exe",
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe", "iexplore.exe",
    "discord.exe", "slack.exe", "teams.exe", "ms-teams.exe", "zoom.exe", "spotify.exe", "telegram.exe", "whatsapp.exe",
    "steam.exe", "steamwebhelper.exe", "epicgameslauncher.exe", "epicwebhelper.exe", "galaxyclient.exe", "eadesktop.exe",
    "origin.exe", "battle.net.exe", "ubisoftconnect.exe", "upc.exe", "riotclientservices.exe", "riotclientux.exe",
    "nvidia share.exe", "nvcontainer.exe", "nvidia app.exe", "nvcplui.exe", "radeonsoftware.exe", "amdrssrcext.exe",
    "obs64.exe", "obs32.exe", "streamlabs desktop.exe", "vlc.exe", "mpc-hc64.exe", "wallpaper32.exe", "wallpaper64.exe",
    "code.exe", "devenv.exe", "notepad.exe", "winword.exe", "excel.exe", "powerpnt.exe", "outlook.exe",
    "hwinfo64.exe", "hwinfo32.exe", "librehardwaremonitor.exe", "msiafterburner.exe", "rtss.exe",
    // Chromium/Electron based desktop apps draw with Direct3D too, and look like a game to a frame counter.
    "claude.exe", "chatgpt.exe", "cursor.exe", "windsurf.exe", "notion.exe", "figma.exe", "obsidian.exe", "signal.exe",
    "postman.exe", "onedrive.exe", "electron.exe", "arc.exe", "comet.exe", "whatsapp.root.exe", "ms-teams.exe",
    "gamebar.exe", "gamebarftserver.exe", "xboxpcapp.exe", "systemsettings.exe", "lockapp.exe", "phoneexperiencehost.exe",
    "mspaint.exe", "snippingtool.exe", "screenclippinghost.exe",
];

pub fn is_never_a_game(exe: &str) -> bool {
    let e = exe.to_lowercase();
    NOT_GAMES.contains(&e.as_str())
}

/// Human description of the graphics API from loaded libraries and the capture event type.
pub fn describe_api(api: Api, modules: &[String]) -> String {
    let has = |m: &str| modules.iter().any(|x| x == m);
    let (d12, d11) = (has("d3d12.dll"), has("d3d11.dll"));
    match api {
        Api::D3d9 => "DirectX 9".into(),
        Api::Dxgi => {
            if d12 && d11 {
                "DirectX 11/12".into()
            } else if d12 {
                "DirectX 12".into()
            } else if d11 {
                "DirectX 11".into()
            } else if has("d3d10.dll") || has("d3d10_1.dll") || has("d3d10core.dll") {
                "DirectX 10".into()
            } else if has("d3d9.dll") {
                "DirectX 9".into()
            } else {
                "DirectX (DXGI)".into()
            }
        }
        Api::Other => "Unknown".into(),
    }
}

/// When a program loads Vulkan/OpenGL, say so (FPS capture does not cover those APIs yet).
pub fn unsupported_api(modules: &[String]) -> Option<&'static str> {
    let has = |m: &str| modules.iter().any(|x| x == m);
    let dx = has("d3d11.dll") || has("d3d12.dll") || has("d3d9.dll") || has("d3d10.dll");
    if has("vulkan-1.dll") && !dx {
        Some("Vulkan")
    } else if has("opengl32.dll") && !dx {
        Some("OpenGL")
    } else {
        None
    }
}

#[cfg(not(windows))]
struct LinuxProcs;

#[cfg(not(windows))]
impl ProcessSource for LinuxProcs {
    fn list(&mut self) -> Vec<ProcInfo> {
        let mut v = Vec::new();
        for e in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            let exe = fs::read_to_string(e.path().join("comm")).map(|s| s.trim().to_lowercase()).unwrap_or_default();
            if !exe.is_empty() {
                v.push(ProcInfo { pid, exe });
            }
        }
        v
    }
    fn foreground(&mut self) -> Option<ForegroundInfo> {
        None
    }
    fn graphics_modules(&mut self, _pid: u32) -> Vec<String> {
        Vec::new()
    }
}

pub fn system() -> Box<dyn ProcessSource> {
    #[cfg(windows)]
    {
        Box::new(crate::windows::proc::WindowsProcs::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(LinuxProcs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_and_launchers_are_never_games() {
        assert!(is_never_a_game("Chrome.exe"));
        assert!(is_never_a_game("dwm.exe"));
        assert!(is_never_a_game("Claude.exe"), "an Electron desktop app is not a game");
        assert!(!is_never_a_game("cs2.exe"));
    }

    #[test]
    fn api_description() {
        let m = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(describe_api(Api::Dxgi, &m(&["d3d12.dll", "dxgi.dll"])), "DirectX 12");
        assert_eq!(describe_api(Api::Dxgi, &m(&["d3d11.dll"])), "DirectX 11");
        assert_eq!(describe_api(Api::Dxgi, &m(&["d3d11.dll", "d3d12.dll"])), "DirectX 11/12");
        assert_eq!(describe_api(Api::Dxgi, &[]), "DirectX (DXGI)");
        assert_eq!(describe_api(Api::D3d9, &[]), "DirectX 9");
    }

    #[test]
    fn vulkan_and_opengl_are_called_out() {
        let m = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(unsupported_api(&m(&["vulkan-1.dll"])), Some("Vulkan"));
        assert_eq!(unsupported_api(&m(&["opengl32.dll"])), Some("OpenGL"));
        assert_eq!(unsupported_api(&m(&["vulkan-1.dll", "d3d11.dll"])), None, "mixed apps present through DXGI");
    }

    #[cfg(not(windows))]
    #[test]
    fn can_list_processes_here() {
        let mut p = system();
        assert!(!p.list().is_empty());
    }
}
