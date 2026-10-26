# Compatibility and limitations

This page lists what FPSFramer can't do and what to expect. It is shown in the app too
(Help → Games & anti-cheat). Everything about Windows is based on documented Windows behaviour and
**has not yet been confirmed on real Windows machines** — see [TESTING.md](TESTING.md).

## Games and graphics APIs

| | FPS & frame time | Hardware readings | Overlay |
| --- | --- | --- | --- |
| DirectX 11 / 12, borderless or windowed | yes | yes | yes |
| DirectX 9 / 10 | yes | yes | yes |
| Exclusive fullscreen | yes | yes | usually hidden by Windows |
| Vulkan / OpenGL | **no** (not captured) | yes | yes (windowed) |

* FPS comes from Windows' DXGI / D3D9 *present* events. Vulkan and OpenGL games that don't present through
  DXGI produce none. When such a game has its own profile, it is still detected as "a game is running", so
  the overlay and recording continue with hardware data and the app says FPS is unsupported for it.
* An overlay is an ordinary window above the game. Windows does not compose other windows above
  exclusive-fullscreen Direct3D, so use "Borderless" / "Windowed fullscreen".
* Screenshots are taken from the composed desktop; exclusive fullscreen may return black.
* The overlay is placed on the monitor the game window is on; this relies on monitor names reported by the
  windowing layer and falls back to the main screen if they don't match.

## Anti-cheat

* FPSFramer never injects code, hooks graphics calls, or reads/writes game memory. FPS comes from OS event
  tracing; the overlay is a separate window; hardware readings come from OS counters and NVML.
* It does not attempt to hide from, or work around, any anti-cheat.
* That design is the same one several widely accepted tools use, **but nobody can guarantee a particular
  game's anti-cheat will accept it**. Some competitive games forbid all overlays. If in doubt, turn the
  overlay off (hotkey) — recording and hardware monitoring keep working without it.
* Installing a kernel driver would unlock more sensors but is exactly what anti-cheat dislikes, so FPSFramer
  installs none.

## Sensors

| Sensor | Without a helper | Notes |
| --- | --- | --- |
| FPS, frame time, lows | needs ETW permission | DirectX only |
| CPU load (total / per core), clock | yes | Windows performance counters |
| CPU temperature, power, fans, voltages | **no** | needs HWiNFO (shared memory) or LibreHardwareMonitor (WMI) running |
| NVIDIA GPU: load, VRAM, temperature, clocks, power, fan | yes | NVML (`nvml.dll`, from the driver) |
| AMD / Intel GPU: load, VRAM | yes | Windows GPU counters (what Task Manager shows) |
| AMD / Intel GPU: temperature, clocks, power, fan | **no** | companion tool |
| Memory, storage, network, battery | yes | counters / Win32 |

Unavailable sensors are reported with the reason and the fix, never as zero. Values that need a companion
tool are labelled with their source.

## Not included (on purpose)

* **FPS limiting** — a correct limiter hooks the game's present call. The app explains how to cap in the game,
  NVIDIA Control Panel, AMD Software, or with V-Sync / VRR, and opens Windows' graphics settings.
* **Overclocking / fan control / any hardware writes.**
* **Network features** — no cloud sync, uploads, accounts or auto-update check in this version.

## Other known limits

* The first public version is Windows-only; the same code builds on Linux for development/demo but has no FPS capture there.
* `Win+` shortcuts and keys the windowing layer can't register will be reported as "couldn't be set".
* Notification sounds depend on Windows' notification settings (Focus Assist / Do Not Disturb can suppress toasts).
* While recording, one reading per second is kept in memory and written when the session ends; very long
  sessions therefore use proportionally more memory.
