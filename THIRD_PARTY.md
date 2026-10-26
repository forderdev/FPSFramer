# Third-party software

FPSFramer's own code, name, logo and wording are original and MIT-licensed (see `LICENSE`). It builds on
open-source components; none of their code was copied into this repository, they are dependencies.

| Component | Used for | Licence |
| --- | --- | --- |
| [Tauri](https://tauri.app) and its plugins (dialog, notification, autostart, global-shortcut, single-instance) | desktop shell | MIT / Apache-2.0 |
| [`windows`](https://github.com/microsoft/windows-rs), `winreg`, `wmi` | Windows API access | MIT / Apache-2.0 |
| [React](https://react.dev), [zustand](https://github.com/pmndrs/zustand) | user interface | MIT |
| [Lucide](https://lucide.dev) icons (`lucide-react`) | icons | ISC |
| [Inter](https://rsms.me/inter/) (`@fontsource-variable/inter`) | interface font | SIL Open Font License 1.1 |
| [JetBrains Mono](https://www.jetbrains.com/lp/mono/) (`@fontsource/jetbrains-mono`) | numbers / code font | SIL Open Font License 1.1 |
| [Vite](https://vitejs.dev), [Vitest](https://vitest.dev), TypeScript | build and tests | MIT / Apache-2.0 |
| [Playwright](https://playwright.dev), [axe-core](https://github.com/dequelabs/axe-core) | end-to-end and accessibility tests (development only) | Apache-2.0 / MPL-2.0 |
| `serde`, `serde_json`, `parking_lot`, `crossbeam-channel`, `flate2`, `png`, `libloading`, `dirs`, `log`, `thiserror` | core libraries | MIT / Apache-2.0 |

Runtime helpers that FPSFramer *talks to but does not bundle*: NVIDIA's `nvml.dll` (part of the NVIDIA driver),
and, if the user runs them, HWiNFO (shared memory) and LibreHardwareMonitor (WMI). Windows' ETW and performance
counters are part of the operating system. The idea of observing frame presentation through ETW is public
(Microsoft documents the providers, and Intel's PresentMon popularised it); FPSFramer contains its own
implementation.

Transitive dependencies carry their own licences. Before publishing a release, generate the full list, for example
with [`cargo about`](https://github.com/EmbarkStudios/cargo-about) for Rust and `npx license-checker --production`
for the UI, and ship it with the installer.

## Artwork

`ui/public/preview-game.webp` is a frame from *Counter-Strike 2*, © Valve Corporation, used only as a backdrop in the
overlay previews to show what an overlay looks like over a game. FPSFramer is not affiliated with or endorsed by
Valve. It is not covered by this project's MIT licence. Replace it with your own picture before publishing the project
more widely.
