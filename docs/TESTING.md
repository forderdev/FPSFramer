# Testing — what has been verified, and what has not

This project was written and tested on **Linux**. Everything that does not need Windows is covered by
automated tests; the Windows-specific code compiles against the real Windows API bindings but **has not been
executed on a Windows machine**. This page says exactly where that line is, so nobody has to guess.

## Automated tests

| Suite | Command | What it covers | Last result |
| --- | --- | --- | --- |
| Rust unit/integration | `cargo test --workspace` | frame statistics (percentiles, lows, windows, pauses), spike / drop / bottleneck analysis, alert engine (sustain, cooldown, hysteresis), layout validation and sanitising of hostile imports, storage (atomic writes, corruption recovery, trash/undo, backups), game router, overlay specs, RPC surface, a full **demo-world end-to-end flow** (game detected → live snapshot → auto session → analysis → saved → exported), and a **real-machine smoke test** that runs the sensor providers for real | core 71, platform 44, engine 34 — all pass |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` (also with `--target x86_64-pc-windows-msvc`) | lint-clean on both targets | clean |
| UI unit | `npm --prefix ui test` | layout editing operations, formatting and unit conversion, status colours, shortcut parsing, session comparison, chart decimation | 42 pass |
| End-to-end | `npm --prefix ui run e2e` | the real engine in demo mode driven through the real UI in Chromium: first-run wizard, every page in dark / light / high-contrast themes (render, no console errors, **axe-core accessibility with zero serious/critical findings**), narrow-window overflow, text scaling, alerts, profiles (create / import / export / conflict), layout editor (add, undo/redo, save, unsaved-changes guard), sessions (record → list → report → CSV export → star / delete / undo → compare), settings (theme, units, hotkey recording and conflicts, backup, reset confirmation), the overlay window page, diagnostics, "unavailable ≠ zero" | 89 pass |
| Windows type-check | `cargo check --target x86_64-pc-windows-msvc --workspace` | all Windows-only code compiles against the real bindings | pass |

## Checked by running the actual desktop shell (Linux/X11 under Xvfb)

* The Tauri app starts, shows the main window with the UI, answers RPC calls and streams events.
* An overlay window is created for the running (simulated) game, fitted to its content, placed at its anchor,
  shows live numbers and a "DEMO DATA" label.
* A **global hotkey** (Ctrl+Shift+F9) toggled the overlay: the setting was saved, the overlay window was destroyed
  and an "Overlay off" toast appeared.
* **Edit mode** (Ctrl+Shift+F7) showed the overlay with a drag frame; moving the window was detected, and after it
  settled the new position was saved as `bottom-right` + offset.

## Verified on one real Windows PC (i5-12400F, RTX 3060 Ti, Windows 11 25H2) — first user run

* The ETW session starts (as administrator), DXGI present events arrive (about a million in the first run), CS2 and
  Rocket League are detected (DirectX 11) and sessions are recorded.
* Performance counters, NVIDIA NVML sensors (temperature, power, fan, video memory), memory, disk and network
  report; CPU temperature is correctly reported as unavailable without HWiNFO / LibreHardwareMonitor.
* The installer builds on a Windows runner, installs and starts; the diagnostics report works.
* The same run exposed bugs, fixed afterwards and **re-tested on that PC in a second session** (debug build, not
  elevated): every global shortcut failed with "already registered" (now 7 registered, the toggle hotkey works and
  keeps focus where it was), hardware showed as "Unknown" (now real CPU / GPU / RAM / displays), one GPU was listed
  twice (now one), and with Chrome / Discord / Claude open the game picked was `cs2.exe`.
* **Overlay visibility, second session:** the overlay was never visible because it was shown with a raw `ShowWindow`
  behind the toolkit's back. After the fix it is visible over a real, running CS2 in
  fullscreen-windowed mode (screenshot), click-through, topmost, not in the taskbar/Alt-Tab (window style bits
  `0x80801B8`), and it does not take focus. Removing / re-adding it with the hotkey leaves exactly one window.
* **FPS numbers:** run elevated, ETW tracked CS2 (DirectX 11) at ~190 FPS with occasional dips. The owner states they
  compared the values with another tool and found them correct; this was not reproduced by the session, and
  a scripted side by side run against PresentMon did not work yet (PresentMon produced no output on that PC).
* **Not re-tested:** exclusive fullscreen, Rocket League, two-monitor / mixed-DPI placement with the game on the
  second monitor.

## Not verified on real Windows

These paths are written against documented Windows behaviour and type-check, but were never run (or, for the items
under "Verified" above, only partly). Treat them as "expected to work, unproven" until someone runs the checklist
below.

* **FPS capture** (`platform/src/windows/etw.rs`): starting the ETW session (including the single-buffer-pool
  flag and its fallback, and the 100 ms flush helper), receiving DXGI/D3D9 present events, QPC timestamp
  handling, `NeedsPermission` detection, stopping/cleaning stale sessions. The *consequences* of batched,
  possibly late delivery on the FPS number are unit-tested with simulated delivery patterns; the real delivery
  pattern of ETW on Windows is not.
* **Sensors:** PDH counters (CPU/disk/network/GPU engines), DXGI adapters, NVML on an NVIDIA machine, HWiNFO shared
  memory layout, LibreHardwareMonitor WMI, battery.
* **Overlay behaviour:** transparency, click-through, never taking focus, staying above borderless games, DPI
  handling on mixed-scale multi-monitor setups, monitor-name matching for "the game's monitor",
  `WDA_EXCLUDEFROMCAPTURE`, topmost re-assertion.
* **Shell:** tray icon and menu, start-with-Windows registration, notification toasts and sound, single-instance
  hand-off, the NSIS installer, the WebView2 bootstrapper, elevation / "Performance Log Users" grant.
* **Screenshots** (GDI) in borderless and exclusive-fullscreen modes.
* Behaviour with specific anti-cheat systems — **not tested with any**, and no claim is made.

The `windows-tests` and `windows-installer` jobs in `.github/workflows/ci.yml` run the Rust tests (including the
real-machine smoke test against the Windows performance counters) on a Windows runner and build the installer;
both passed on their first run.

## Manual checklist for a Windows tester

1. **Install** the NSIS installer as a normal user; launch; the first-run guide appears. Complete it.
2. Home shows *Waiting for a game*; CPU / memory / storage / network tiles show real values; temperatures/fans that
   need a helper say *Not available* with a reason (never 0).
3. **FPS permission:** Help → FPS permission → *Allow FPS measurement*, sign out/in (or *Restart as administrator*).
4. Start a **DirectX 11/12 game in borderless mode**: within ~2 s Home shows the game, FPS, frame time, 1 % low;
   compare FPS with another tool (expect small differences); the overlay appears on the game's monitor and does not
   steal focus or block mouse clicks.
5. Switch the game to **exclusive fullscreen**: note whether the overlay is hidden (expected) and whether FPS
   still updates.
6. Run a **Vulkan or OpenGL game**: FPS should be reported as unsupported with an explanation; hardware numbers
   and the overlay still work.
7. **Hotkeys** (toggle overlay, next style, screenshot, record, edit mode) work while the game has focus.
   Edit mode: drag the overlay, press the hotkey again, restart the app; the position persists.
8. Close the window: the app stays in the **tray**; tray menu items work; *Quit* ends it. Reopen from the tray.
9. Enable *Start with Windows*; sign out/in; the app starts to the tray.
10. Play ~5 minutes: a **session** appears with graphs; spikes/drops make sense; export CSV; compare two sessions.
11. Raise the GPU load until an **alert** fires (or lower the threshold): overlay banner and notification appear
    once, not repeatedly.
12. **Multi-monitor / mixed DPI:** move the game to another monitor with a different scale; the overlay follows and
    is the right size.
13. NVIDIA machine: GPU temperature, clocks, power, fan show. AMD/Intel: those say *needs a helper*.
14. Run HWiNFO (Shared Memory on) or LibreHardwareMonitor (as admin): CPU temperature/fans appear within seconds
    and are labelled with their source; close it and they go back to *Not available*.
15. Turn on *Hide the overlay from screenshots and recordings* and confirm with a screen recorder.
16. Uninstall: the app disappears. Your settings and sessions (folders listed in Settings → About) should still be there – the uninstaller is not configured to delete them. Please confirm.

Please report anything that differs, with *Help → Check my setup → Copy report* attached (it contains no user
names or paths).
