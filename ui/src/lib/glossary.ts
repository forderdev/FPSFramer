// Plain-language explanations of technical terms. Used for tooltips and the Help → Glossary page.

export interface Term {
  term: string;
  short: string;
  long: string;
}

export const GLOSSARY: Term[] = [
  { term: 'FPS', short: 'Frames per second: how many pictures the game draws each second.', long: 'Higher FPS looks smoother. 30 FPS is playable, 60 FPS feels smooth, 120 to 144 FPS feels very responsive in fast games. Your monitor can only show as many frames as its refresh rate (for example 60 Hz = 60 frames per second).' },
  { term: 'Frame time', short: 'How long one frame takes to draw, in milliseconds (ms).', long: 'It is the same information as FPS, seen from the other side: 16.7 ms per frame is 60 FPS, 6.9 ms is 144 FPS. Frame time is better at revealing stutter, because one very long frame shows up as a spike even if the average FPS looks fine.' },
  { term: '1% Low', short: 'The average FPS of your slowest 1% of frames.', long: 'It describes how bad the dips are. If your average is 100 FPS but your 1% Low is 30 FPS, the game will feel choppy at times even though the average looks great. The closer the 1% Low is to the average, the smoother the game feels. It needs about 100 frames of data to be reliable.' },
  { term: '0.1% Low', short: 'The average FPS of your very slowest 0.1% of frames.', long: 'It catches rare but nasty hitches. It needs about 1,000 frames of data, so it appears after the game has run for a little while.' },
  { term: 'Stutter', short: 'Uneven delivery of frames that makes motion look jerky.', long: 'Stutter shows up as spikes in the frame-time graph. Common causes: shader compilation, loading data from a slow disk, running out of RAM or video memory, background programs, or overheating.' },
  { term: 'CPU', short: 'The processor, the main "brain" of the PC.', long: 'Games use the CPU for game logic, physics and preparing what the graphics card draws. Many games rely heavily on one or two cores, so a single busy core can limit FPS even when total CPU usage looks low.' },
  { term: 'GPU', short: 'The graphics card. It draws the picture.', long: 'In most games the GPU is the busiest part. Usage near 100% while playing is normal and means the graphics card is working at its limit.' },
  { term: 'VRAM', short: 'The graphics card\'s own memory.', long: 'Textures and models live here. If VRAM fills up the game must swap data in and out, which often causes stutter. Lowering texture quality or resolution frees VRAM.' },
  { term: 'RAM', short: 'The PC\'s main memory.', long: 'Programs and the game keep their working data here. When RAM is nearly full Windows starts using the disk as slow extra memory, which can cause hitching.' },
  { term: 'Utilization', short: 'How busy a component is, from 0% to 100%.', long: 'It is the percentage of time the part spends working. 100% does not mean something is wrong. It means the part is fully used.' },
  { term: 'Bottleneck', short: 'The part that limits performance.', long: 'In any game one part (the CPU or the GPU, usually) is the slowest link. Upgrading something else will not raise FPS much until that part is improved or the game settings are changed to ask less of it. FPSFramer gives an estimate based on usage readings. It is a hint, not a guarantee.' },
  { term: 'Thermal throttling', short: 'A part slowing itself down because it is too hot.', long: 'CPUs and graphics cards protect themselves by lowering their speed when they overheat. FPS drops and the game may stutter. Better airflow, cleaning dust and adjusting fan curves usually help.' },
  { term: 'Clock speed', short: 'How fast a chip is running, in MHz or GHz.', long: 'It rises under load and falls when idle to save power. A clock that falls sharply while temperatures are high is a sign of throttling.' },
  { term: 'V-Sync', short: 'A setting that locks the game to your monitor\'s refresh rate.', long: 'It prevents screen tearing but caps FPS (for example at 60 on a 60 Hz screen) and can add input delay. If your FPS is stuck at exactly 60, V-Sync or a frame limiter is probably on.' },
  { term: 'Refresh rate', short: 'How many times per second your monitor updates (Hz).', long: 'A 144 Hz monitor can show up to 144 frames each second. FPS above the refresh rate is not fully visible, though it can still reduce input delay.' },
  { term: 'Borderless / windowed', short: 'Display modes where the game shares the desktop.', long: 'In these modes other windows, including the FPSFramer overlay, can appear above the game. "Exclusive fullscreen" takes over the display completely and hides overlays like this one.' },
  { term: 'Overlay', short: 'Numbers drawn on top of the game while you play.', long: 'FPSFramer shows its overlay as a separate transparent window above the game. It does not inject anything into the game, which keeps it compatible with anti-cheat software.' },
  { term: 'Scene', short: 'A named set of overlays you can switch between.', long: 'For example "Minimal" while playing and "Detailed" when you are testing. Switch scenes with a hotkey, even in-game.' },
  { term: 'Profile', short: 'Settings that belong to one game.', long: 'A profile remembers the overlay scenes, FPS target and alerts for a specific game. FPSFramer applies it automatically when it recognises the game.' },
  { term: 'Session', short: 'One recorded play period.', long: 'Sessions are recorded automatically while a game runs. Afterwards you get a summary, graphs and plain-language findings.' },
  { term: 'Companion tool', short: 'A free program that adds sensors Windows hides.', long: 'Windows does not let apps read CPU temperature, fan speeds or voltages without a hardware driver. HWiNFO and LibreHardwareMonitor include such drivers; FPSFramer can import their readings when they are running.' },
  { term: 'DirectX / Vulkan / OpenGL', short: 'The technologies games use to talk to the graphics card.', long: 'FPSFramer can measure FPS in DirectX 9, 10, 11 and 12 games. Vulkan and OpenGL games are not supported for FPS measurement yet, although the overlay and hardware readings still work.' },
];

const BY_TERM = new Map(GLOSSARY.map((t) => [t.term.toLowerCase(), t]));

export function termHelp(term: string): string | undefined {
  return BY_TERM.get(term.toLowerCase())?.short;
}
