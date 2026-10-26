# Architecture

FPSFramer is a Cargo workspace plus a web UI. The important rule: **the engine decides *what* should
happen; the native shell only decides *how* it is shown.** That keeps almost everything testable on any
operating system.

```
┌───────────────────────────── desktop app (app/src-tauri) ─────────────────────────────┐
│  main window ─┐                                                                         │
│  overlay windows (one per OverlaySpec) ─┤  Tauri commands: rpc, overlay_size, …        │
│  tray · global hotkeys · notifications ─┘  event "engine-event" ◄── engine events      │
└───────────────▲─────────────────────────────────────────────┬───────────────────────────┘
                │ JSON-RPC (Engine::call)                      │ Event bus (crossbeam channels)
┌───────────────┴──────────────────────────────────────────────▼───────────────────────────┐
│ crates/engine                                                                              │
│  sampler thread: providers → readings → router (which process is "the game"?) → frame     │
│  statistics → alerts → session recorder → snapshot event          saver thread: sessions   │
│  rpc.rs: settings, layouts, profiles, sessions, diagnostics, backup, screenshots …         │
└───────────────▲──────────────────────────────────────────────────────────────────────────┘
                │
┌───────────────┴────────────────────────┐   ┌──────────────────────────────────────────────┐
│ crates/platform                         │   │ crates/core (no OS code)                       │
│  SensorProvider impls (Windows/Linux/   │   │  FrameTracker, Histogram, spike & drop finder  │
│  demo), FrameSource (ETW / demo),       │   │  bottleneck + findings, AlertEngine, layouts,  │
│  ProcessSource, screenshots, overlay    │   │  profiles, settings, storage, session format   │
│  window helpers                         │   └──────────────────────────────────────────────┘
└─────────────────────────────────────────┘
```

## Honest sensors

A reading is either **present in the `Readings` map or absent** — a missing sensor is never a zero.
Every sensor has a `SensorDesc` whose `availability` carries a machine-readable reason
(`noHardware`, `needsCompanion`, `needsPermission`, `noActiveGame`, `notSupported`, `warming`,
`providerError`), a plain-language message and, where one exists, a fix. The overlay, the graphs and the
reports all key off that, so "N/A — why" is shown instead of `0`. The engine test
`real_linux_sensors_flow_through_the_engine` checks this against real `/proc` data.

## FPS measurement

* **Source:** an ETW real-time session (`FPSFramer-Capture`) subscribed to the DXGI
  (`{CA11C036-0102-4A2D-A6AD-F03CFED5D3C9}`, event 42) and Direct3D 9
  (`{783ACA0A-790E-4D7F-8451-AA850511C6B9}`, event 1) providers — the same public mechanism Intel
  PresentMon uses. QPC timestamps; `DXGI_PRESENT_TEST` calls are ignored; frame time is the delta between
  consecutive presents per (process, swap chain).
* **Delivery rhythm:** Windows hands trace events to a consumer a buffer at a time (about once a second unless
  asked otherwise). The session therefore uses one buffer pool (so events arrive in timestamp order) and a small
  helper thread flushes it every 100 ms; the engine additionally sorts each batch by timestamp. The live FPS
  window ends at the newest delivered frame while frames keep arriving, and only at "now" once nothing has
  arrived for longer than a grace period learned from how late batches normally are — so a once-a-second
  delivery can't make the number saw-tooth, while a real freeze still shows as FPS falling to 0 (tests:
  `batched_delivery_does_not_make_fps_sawtooth`, `a_freeze_still_shows_up_when_frames_arrive_in_batches`).
* **Statistics** (`core::frames`): a fine-bin histogram (0.05 ms bins to 100 ms, 1 ms bins to 5 s) gives
  exact-enough percentiles in constant memory. "1 % low" is the average FPS of the slowest 1 % of frames
  (the percentile-frame method is a setting). 1 % lows need ≥ 100 frames and 0.1 % lows ≥ 1000 before they
  are shown. Gaps over 5 s are treated as pauses and ignored.
* **Router** (`engine::router`): picks the game among presenting processes by present rate (≥ 10/s),
  foreground preference, stickiness and a two-selection switch rule; drops it after 8 s of silence; never
  picks desktop/browser/shell processes; supports a manual process. A running program that has its own
  profile counts as "game active" even when no frames can be captured (so the overlay and hardware
  recording still work for Vulkan/OpenGL games).
* **Permissions:** starting the session needs admin or *Performance Log Users*; otherwise the capture
  state is `needsPermission` and the UI offers the one-time fix.

## Overlay windows

The engine computes `OverlaySpec`s (which overlays exist right now, where, how big, click-through,
edit mode …). The shell makes windows match: borderless, transparent, always-on-top, no taskbar entry, no
focus (`WS_EX_NOACTIVATE`), optional `WDA_EXCLUDEFROMCAPTURE`. Each overlay is a web page
(`overlay.html?id=…`) drawn by the **same renderer as the editor preview** (`ui/src/overlay/LayoutRenderer.tsx`),
so the preview is exact. The page reports its content size; the shell fits the window, positions it by
anchor + offset on the chosen monitor (DPI-aware) and shows it without activating it. Dragging in edit
mode is saved back as the nearest of nine anchors plus an offset. A small loop re-asserts topmost order
(optional).

Windows are created only while needed (a game is running, a preview or edit mode is active), so an idle
FPSFramer holds no overlay web views.

## Data and persistence

* Settings, profiles, layouts and alert rules are separate JSON files under the config folder;
  sessions (gzip + an index), logs and backups under the data folder. Writes are atomic; a corrupt file is
  kept as `.corrupt-<time>` and the last `.bak` is restored with a notice in the UI.
* Imported layouts / profiles / backups pass through validators that cap sizes, allow only `data:` images
  and reject path-traversal ids.
* Sessions are deleted to a trash first, so every delete can be undone.

## UI

React 19 + Vite, a small zustand store fed by one `engine-event` stream, a hash router with a "leave
without saving?" guard. All colours/sizes come from CSS tokens (dark, light, high contrast; rem-based
scale). Charts are canvas with a keyboard-navigable cursor and a table fallback. The UI never talks to the
OS: in the desktop app it calls Tauri commands, in a browser it talks to `fpsframer-devserver` — the only
place that knows the difference is `ui/src/lib/bridge.ts`.

## Threads and locking

The sampler thread owns timing; one `Mutex<Core>` protects shared state and is never held while calling
into windowing code. The shell subscribes twice to the event bus — one thread feeds web pages, another does
native work — so slow window operations can never delay live numbers.
