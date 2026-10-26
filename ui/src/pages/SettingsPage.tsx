import { Archive, Eye, Gauge, Keyboard, Monitor, Palette, RotateCcw, Settings as SettingsIcon, Shield, X } from 'lucide-react';
import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { HotkeyField } from '../components/HotkeyField';
import { Badge, Button, Card, CardHead, ColorField, Disclosure, Field, NumberInput, PageHead, SearchBox, Segmented, Select, Setting, Slider, Switch, Tabs, TextInput, confirmAction } from '../components/ui';
import { rpc } from '../lib/bridge';
import { openTextFile, saveTextFile } from '../lib/files';
import { formatBytes, formatDateTime } from '../lib/format';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { Hotkeys } from '../lib/types';
import { displayScale, fromDisplayValue, toDisplayValue } from '../lib/units';

type TabId = 'general' | 'appearance' | 'monitoring' | 'hotkeys' | 'overlay' | 'backup' | 'about';
const TABS: { id: TabId; label: string; icon: ReactNode }[] = [
  { id: 'general', label: 'General', icon: <SettingsIcon size={15} /> },
  { id: 'appearance', label: 'Appearance', icon: <Palette size={15} /> },
  { id: 'monitoring', label: 'Monitoring', icon: <Gauge size={15} /> },
  { id: 'hotkeys', label: 'Hotkeys', icon: <Keyboard size={15} /> },
  { id: 'overlay', label: 'Overlay', icon: <Monitor size={15} /> },
  { id: 'backup', label: 'Backup & reset', icon: <Archive size={15} /> },
  { id: 'about', label: 'About & privacy', icon: <Shield size={15} /> },
];

export default function SettingsPage({ tab }: { tab?: string }) {
  const current: TabId = TABS.some((t) => t.id === tab) ? (tab as TabId) : 'general';
  return (
    <div className="page">
      <PageHead title="Settings" sub="Everything is saved the moment you change it." />
      <Tabs value={current} tabs={TABS} onChange={(t) => navigate(`/settings/${t}`)} />
      {current === 'general' && <General />}
      {current === 'appearance' && <Appearance />}
      {current === 'monitoring' && <Monitoring />}
      {current === 'hotkeys' && <HotkeySettings />}
      {current === 'overlay' && <OverlaySettings />}
      {current === 'backup' && <BackupSettings />}
      {current === 'about' && <About />}
    </div>
  );
}

function useSettings() {
  const settings = useApp((s) => s.settings);
  const update = useApp((s) => s.updateSettings);
  return { s: settings, set: (patch: Record<string, unknown>) => void update(patch) };
}

// ───────────────────────────── general ─────────────────────────────

function General() {
  const { s, set } = useSettings();
  const platform = useApp((st) => st.platform);
  const demo = useApp((st) => st.demo);
  const advanced = s.uiMode === 'advanced';
  return (
    <div className="stack">
      <Card>
        <CardHead title="Startup" />
        <Setting title="Start FPSFramer when I sign in to Windows" desc="So your overlay and recordings are ready before you launch a game.">
          <Switch checked={s.launchAtStartup} onChange={(v) => set({ launchAtStartup: v })} label="Start with Windows" disabled={demo || platform !== 'windows'} />
        </Setting>
        <Setting title="Start minimised to the tray" desc="No window pops up when FPSFramer starts. Open it from the tray icon.">
          <Switch checked={s.startMinimized} onChange={(v) => set({ startMinimized: v })} label="Start minimised" />
        </Setting>
        <Setting title="Keep running in the tray when I close the window" desc="The overlay and recording continue in the background. Turn off to quit when the window closes.">
          <Switch checked={s.closeToTray} onChange={(v) => set({ closeToTray: v })} label="Close to tray" />
        </Setting>
        {(demo || platform !== 'windows') && <p className="faint" style={{ margin: '0.5rem 0 0', fontSize: '0.8125rem' }}>Start-with-Windows is only available in the Windows app.</p>}
      </Card>

      <Card>
        <CardHead title="Recording" sub="FPSFramer records each game session so you can look back at how it ran." />
        <Setting title="Record games automatically" desc="Starts when a game is detected, stops when it closes.">
          <Switch checked={s.recording.autoRecord} onChange={(v) => set({ recording: { autoRecord: v } })} label="Record automatically" />
        </Setting>
        <Setting title="Ignore short sessions" desc="Sessions shorter than this are discarded.">
          <NumberInput label="Minimum session length" value={s.recording.minDurationS} min={0} max={600} suffix="seconds" onChange={(v) => set({ recording: { minDurationS: Math.round(v) } })} />
        </Setting>
        <Setting title="Sessions to keep" desc="The oldest unstarred sessions are removed when you go past this number. Starred sessions are always kept.">
          <NumberInput label="Maximum sessions" value={s.recording.maxSessions} min={5} max={2000} onChange={(v) => set({ recording: { maxSessions: Math.round(v) } })} />
        </Setting>
        {advanced && (
          <Setting title="Record every available measurement" desc="Uses more disk space (a few hundred KB per hour) but lets you graph any sensor afterwards.">
            <Switch checked={s.recording.recordAllSensors} onChange={(v) => set({ recording: { recordAllSensors: v } })} label="Record all sensors" />
          </Setting>
        )}
      </Card>

      <Disclosure title="More options" sub="Notifications, screenshots, setup guide">
        <div className="stack">
          <div className="section-title">Notifications</div>
        <Setting title="Show Windows notifications" desc="For alerts and for the summary after a session.">
          <Switch checked={s.notifications.toast} onChange={(v) => set({ notifications: { toast: v } })} label="Show notifications" />
        </Setting>
        <Setting title="Play a sound for important alerts" desc="Only for alerts you turned sound on for.">
          <Switch checked={s.notifications.sound} onChange={(v) => set({ notifications: { sound: v } })} label="Alert sounds" />
        </Setting>
        <Setting title="Tell me when a session summary is ready" desc="A short note after a game closes, with a link to the report.">
          <Switch checked={s.notifications.sessionSummary} onChange={(v) => set({ notifications: { sessionSummary: v } })} label="Session summary notifications" />
        </Setting>

          <div className="section-title">Screenshots</div>
          <ScreenshotFolder />

          <div className="section-title">Setup</div>
        <Setting title="Run the setup guide again" desc="Walks through the hardware check, permissions and choosing an overlay style.">
          <Button
            onClick={() => {
              useApp.setState({ firstRun: true });
              navigate('/');
            }}
          >
            <RotateCcw size={15} /> Open setup guide
          </Button>
        </Setting>
        </div>
      </Disclosure>
    </div>
  );
}

function ScreenshotFolder() {
  const { s, set } = useSettings();
  const [text, setText] = useState(s.screenshots.folder ?? '');
  useEffect(() => setText(s.screenshots.folder ?? ''), [s.screenshots.folder]);
  return (
    <Field label="Folder" help="Leave empty to use Pictures\FPSFramer. The folder is created when you take the first screenshot.">
      <div className="row wrap">
        <div style={{ flex: 1, minWidth: '16rem' }}>
          <TextInput label="Screenshot folder" value={text} onChange={setText} placeholder="Pictures\FPSFramer" />
        </div>
        <Button disabled={(s.screenshots.folder ?? '') === text.trim()} onClick={() => set({ screenshots: { folder: text.trim() || null } })}>
          Save folder
        </Button>
        <Button variant="ghost" onClick={() => void act('screenshots.open')}>
          Open it
        </Button>
      </div>
    </Field>
  );
}

// ───────────────────────────── appearance ─────────────────────────────

function Appearance() {
  const { s, set } = useSettings();
  return (
    <div className="stack">
      <Card>
        <CardHead title="Look" />
        <Setting title="Theme" desc="“High contrast” uses stronger colours and borders for easier reading.">
          <Segmented
            label="Theme"
            value={s.theme}
            options={[
              { value: 'dark', label: 'Dark' },
              { value: 'light', label: 'Light' },
              { value: 'system', label: 'Like Windows' },
              { value: 'high-contrast', label: 'High contrast' },
            ]}
            onChange={(v) => set({ theme: v })}
          />
        </Setting>
        <Setting title="Accent colour" desc="Used for buttons and highlights.">
          <ColorField label="Accent colour" value={s.accent} onChange={(v) => set({ accent: v })} swatches />
        </Setting>
        <Setting title="Text and control size" desc="Makes everything in this window larger or smaller. The overlay has its own size.">
          <div style={{ width: '14rem' }}>
            <Slider label="Interface size" value={s.uiScale} min={0.85} max={1.5} step={0.05} onChange={(v) => set({ uiScale: Math.round(v * 100) / 100 })} format={(v) => `${Math.round(v * 100)}%`} />
          </div>
        </Setting>
        <Setting title="Reduce motion" desc="Turns off animations and sliding effects.">
          <Switch checked={s.reduceMotion} onChange={(v) => set({ reduceMotion: v })} label="Reduce motion" />
        </Setting>
      </Card>

      <Card>
        <CardHead title="Detail and units" />
        <Setting title="Interface" desc="Simple shows the most useful options. Advanced reveals every setting, extra sensors and technical details.">
          <Segmented
            label="Interface detail"
            value={s.uiMode}
            options={[
              { value: 'simple', label: 'Simple' },
              { value: 'advanced', label: 'Advanced' },
            ]}
            onChange={(v) => set({ uiMode: v })}
          />
        </Setting>
        <Setting title="Temperature">
          <Segmented
            label="Temperature unit"
            value={s.units.temperature}
            options={[
              { value: 'celsius', label: '°C' },
              { value: 'fahrenheit', label: '°F' },
            ]}
            onChange={(v) => set({ units: { temperature: v } })}
          />
        </Setting>
        <Setting title="Network speed" desc="Internet plans are usually quoted in megabits per second (Mbps); downloads in megabytes (MB/s).">
          <Segmented
            label="Network unit"
            value={s.units.network}
            options={[
              { value: 'megabits', label: 'Mbps' },
              { value: 'megabytes', label: 'MB/s' },
            ]}
            onChange={(v) => set({ units: { network: v } })}
          />
        </Setting>
      </Card>
    </div>
  );
}

// ───────────────────────────── monitoring ─────────────────────────────

function Monitoring() {
  const { s, set } = useSettings();
  const hardware = useApp((st) => st.hardware);
  const status = useApp((st) => st.status);
  const advanced = s.uiMode === 'advanced';
  const [ignoredText, setIgnoredText] = useState('');

  return (
    <div className="stack">
      <Card>
        <CardHead title="Which program counts as the game?" />
        <Setting title="Detection" desc="Automatic picks the program that is drawing frames, preferring the one in front. Manual lets you name the program yourself.">
          <Segmented
            label="Game detection"
            value={s.capture.mode}
            options={[
              { value: 'auto', label: 'Automatic' },
              { value: 'manual', label: 'Choose myself' },
            ]}
            onChange={(v) => set({ capture: { mode: v } })}
          />
        </Setting>
        {s.capture.mode === 'manual' && (
          <Setting title="Program to measure" desc="For example “game.exe”. Leave empty to measure nothing.">
            <ManualProcess />
          </Setting>
        )}
        <div className="setting" style={{ display: 'block' }}>
          <div className="setting-title">Never treat these as games</div>
          <div className="setting-desc">Programs that draw frames but aren’t games (browsers, video players…). Common ones are already excluded.</div>
          <div className="row wrap" style={{ marginTop: 8 }}>
            {s.capture.ignored.map((e) => (
              <span key={e} className="chip on" style={{ cursor: 'default' }}>
                {e}
                <button type="button" className="x" aria-label={`Stop ignoring ${e}`} onClick={() => set({ capture: { ignored: s.capture.ignored.filter((x) => x !== e) } })}>
                  <X size={13} aria-hidden />
                </button>
              </span>
            ))}
            <div style={{ width: '12rem' }}>
              <TextInput
                label="Add a program to ignore"
                small
                placeholder="program.exe"
                value={ignoredText}
                onChange={setIgnoredText}
                onEnter={() => {
                  const e = ignoredText.trim().toLowerCase();
                  if (e && !s.capture.ignored.includes(e)) set({ capture: { ignored: [...s.capture.ignored, e] } });
                  setIgnoredText('');
                }}
              />
            </div>
          </div>
        </div>
      </Card>

      <Disclosure title="Extra sensors" sub="CPU temperature, fans, voltages">
        <p className="muted" style={{ margin: '0 0 0.5rem' }}>Windows hides these from ordinary programs. A free companion tool running in the background can supply them.</p>
        <Setting title="Use HWiNFO" desc="Needs HWiNFO running with “Shared Memory Support” turned on in its settings.">
          <Switch checked={s.companions.hwinfo} onChange={(v) => set({ companions: { hwinfo: v } })} label="Use HWiNFO" />
        </Setting>
        <Setting title="Use LibreHardwareMonitor" desc="Needs LibreHardwareMonitor (or OpenHardwareMonitor) running as administrator. FPSFramer reads what it publishes through Windows’ WMI interface.">
          <Switch checked={s.companions.libreHardwareMonitor} onChange={(v) => set({ companions: { libreHardwareMonitor: v } })} label="Use LibreHardwareMonitor" />
        </Setting>
        <div className="row wrap" style={{ marginTop: 8 }}>
          {(status?.providers ?? []).map((p) => (
            <Badge key={p.id} tone={p.health === 'ok' ? 'ok' : p.health === 'degraded' ? 'warn' : 'bad'}>
              {p.label}: {p.health === 'ok' ? `${p.sensors} sensors` : p.message ?? p.health}
            </Badge>
          ))}
        </div>
        <p style={{ margin: '0.75rem 0 0' }}>
          <a
            href="#/help/sensors"
            onClick={(e) => {
              e.preventDefault();
              navigate('/help/sensors');
            }}
          >
            How do I set up a companion tool?
          </a>
        </p>
      </Disclosure>

      <Card>
        <CardHead title="Measuring" />
        {(hardware?.gpus.length ?? 0) > 1 && (
          <Setting title="Main graphics card" desc="Your PC has more than one. Choose the one your games use (usually the faster one).">
            <Select
              label="Main graphics card"
              value={s.primaryGpu ?? -1}
              options={[{ value: -1, label: 'Choose automatically' }, ...(hardware?.gpus ?? []).map((g, i) => ({ value: i, label: g }))]}
              onChange={(v) => set({ primaryGpu: v < 0 ? null : v })}
            />
          </Setting>
        )}
        <Setting title="Refresh rate of the readings" desc="How often hardware readings update. Faster is smoother but uses slightly more CPU.">
          <Select
            label="Update interval"
            value={s.updateIntervalMs}
            options={[
              { value: 250, label: '4 per second' },
              { value: 500, label: '2 per second' },
              { value: 1000, label: '1 per second' },
              { value: 2000, label: 'Every 2 seconds' },
            ]}
            onChange={(v) => set({ updateIntervalMs: v })}
          />
        </Setting>
        <Setting title="Statistics cover" desc="The time span used for min / average / max and the 1% lows.">
          <Select
            label="Statistics window"
            value={s.statsWindow}
            options={[
              { value: 'session', label: 'The whole game session' },
              { value: 'last30s', label: 'Last 30 seconds' },
              { value: 'last60s', label: 'Last minute' },
              { value: 'last5m', label: 'Last 5 minutes' },
            ]}
            onChange={(v) => set({ statsWindow: v })}
          />
        </Setting>
        {advanced && (
          <>
            <Setting title="FPS averaging" desc="The live FPS number is averaged over this long. Shorter reacts faster but jumps around more.">
              <Select
                label="FPS averaging window"
                value={s.fpsWindowMs}
                options={[
                  { value: 250, label: '0.25 s' },
                  { value: 500, label: '0.5 s' },
                  { value: 1000, label: '1 s' },
                  { value: 2000, label: '2 s' },
                ]}
                onChange={(v) => set({ fpsWindowMs: v })}
              />
            </Setting>
            <Setting title="How “1% low” is calculated" desc="Different tools use different methods, so numbers may not match exactly. Average-of-slowest is the most common.">
              <Segmented
                label="1% low method"
                value={s.lowsMethod}
                options={[
                  { value: 'average', label: 'Average of slowest 1%' },
                  { value: 'percentile', label: '99th-percentile frame' },
                ]}
                onChange={(v) => set({ lowsMethod: v })}
              />
            </Setting>
          </>
        )}
      </Card>

      {advanced && <WarningLevels />}
    </div>
  );
}

function ManualProcess() {
  const { s, set } = useSettings();
  const [text, setText] = useState(s.capture.manualProcess ?? '');
  useEffect(() => setText(s.capture.manualProcess ?? ''), [s.capture.manualProcess]);
  const commit = () => {
    const v = text.trim().toLowerCase();
    if (v !== (s.capture.manualProcess ?? '')) set({ capture: { manualProcess: v || null } });
  };
  return <input className="input" style={{ width: '14rem' }} aria-label="Program to measure" value={text} placeholder="game.exe" onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === 'Enter' && commit()} />;
}

/** Edit the levels at which a reading turns yellow ("elevated") or red ("critical"). */
function WarningLevels() {
  const { s, set } = useSettings();
  const catalog = useApp((st) => st.catalog);
  const [q, setQ] = useState('');
  const rows = useMemo(() => catalog.filter((d) => d.statusRule && d.availability.available && (!q.trim() || d.name.toLowerCase().includes(q.toLowerCase()))).slice(0, 40), [catalog, q]);
  return (
    <Disclosure title="Warning colours" sub="When readings turn yellow or red">
      <p className="muted" style={{ margin: '0 0 0.75rem' }}>Readings turn yellow, then red, when they pass these levels. Change them to match your hardware.</p>
      <SearchBox value={q} onChange={setQ} label="Search measurements" placeholder="Search measurements" />
      <div className="stack" style={{ marginTop: '0.75rem', gap: 0 }}>
        {rows.map((d) => {
          const rule = d.statusRule!;
          const ov = s.sensorOverrides[d.id];
          const el = ov?.elevated ?? rule.elevated;
          const cr = ov?.critical ?? rule.critical;
          const label = displayScale(d.unit, s, 0).label;
          const setLevel = (k: 'elevated' | 'critical', v: number) => set({ sensorOverrides: { [d.id]: { elevated: ov?.elevated ?? null, critical: ov?.critical ?? null, [k]: fromDisplayValue(d.unit, v, s) } } });
          return (
            <div key={d.id} className="setting">
              <div className="grow">
                <div className="setting-title">{d.name}</div>
                <div className="setting-desc">{rule.direction === 'higherIsWorse' ? 'Warns when it goes above' : 'Warns when it drops below'}</div>
              </div>
              <div className="ctrl">
                <NumberInput label={`${d.name} yellow level`} small value={Math.round(toDisplayValue(d.unit, el, s) * 100) / 100} onChange={(v) => setLevel('elevated', v)} suffix={`${label} yellow`} />
                <NumberInput label={`${d.name} red level`} small value={Math.round(toDisplayValue(d.unit, cr, s) * 100) / 100} onChange={(v) => setLevel('critical', v)} suffix={`${label} red`} />
                {ov && (
                  <Button size="sm" variant="ghost" onClick={() => set({ sensorOverrides: { [d.id]: { elevated: null, critical: null } } })}>
                    Reset
                  </Button>
                )}
              </div>
            </div>
          );
        })}
        {rows.length === 0 && <p className="muted">Nothing matches.</p>}
      </div>
    </Disclosure>
  );
}

// ───────────────────────────── hotkeys ─────────────────────────────

/** The first MAIN_HOTKEYS rows are shown up front; the rest sit under "More shortcuts". */
const MAIN_HOTKEYS = 4;
const HOTKEY_ROWS: { key: keyof Hotkeys; title: string; desc: string }[] = [
  { key: 'toggleOverlay', title: 'Show / hide the overlay', desc: 'Works while you are in a game.' },
  { key: 'nextScene', title: 'Next style set', desc: 'Cycle through the overlay styles of the current game.' },
  { key: 'screenshot', title: 'Take a screenshot', desc: '' },
  { key: 'toggleRecording', title: 'Start / stop recording', desc: 'Records a session by hand, for example on the desktop.' },
  { key: 'prevScene', title: 'Previous style set', desc: '' },
  { key: 'toggleEdit', title: 'Move the overlay', desc: 'Lets you drag the overlay on screen, then lock it again.' },
  { key: 'resetStats', title: 'Reset min / max / lows', desc: 'Starts the statistics over without ending the session.' },
  { key: 'openApp', title: 'Open FPSFramer', desc: 'Brings this window to the front.' },
];

const DEFAULT_KEYS: Hotkeys = {
  toggleOverlay: 'Ctrl+Shift+F9',
  nextScene: 'Ctrl+Shift+F10',
  prevScene: 'Ctrl+Shift+F8',
  toggleEdit: 'Ctrl+Shift+F7',
  toggleRecording: 'Ctrl+Shift+F6',
  screenshot: 'Ctrl+Shift+F12',
  resetStats: 'Ctrl+Shift+F5',
  openApp: null,
};

function HotkeySettings() {
  const { s, set } = useSettings();
  return (
    <div className="stack">
      <p className="muted" style={{ margin: 0, maxWidth: '44rem' }}>
        Shortcuts work even while a game is in front. They need a modifier (Ctrl, Alt or Shift) so they never steal normal game keys. If one does nothing, another program may already use it.
      </p>
      <Card>
        {HOTKEY_ROWS.slice(0, MAIN_HOTKEYS).map((r) => (
          <Setting key={r.key} title={r.title} desc={r.desc || undefined}>
            <HotkeyField label={r.title} value={s.hotkeys[r.key]} onChange={(v) => set({ hotkeys: { [r.key]: v } })} />
          </Setting>
        ))}
      </Card>
      <Disclosure title="More shortcuts" sub={`${HOTKEY_ROWS.length - MAIN_HOTKEYS} more`}>
        {HOTKEY_ROWS.slice(MAIN_HOTKEYS).map((r) => (
          <Setting key={r.key} title={r.title} desc={r.desc || undefined}>
            <HotkeyField label={r.title} value={s.hotkeys[r.key]} onChange={(v) => set({ hotkeys: { [r.key]: v } })} />
          </Setting>
        ))}
      </Disclosure>
      <div>
        <div>
          <Button
            onClick={async () => {
              if (await confirmAction({ title: 'Restore the standard shortcuts?', body: 'Your current shortcuts will be replaced.', confirmLabel: 'Restore' })) set({ hotkeys: DEFAULT_KEYS });
            }}
          >
            <RotateCcw size={15} /> Restore standard shortcuts
          </Button>
        </div>
      </div>
    </div>
  );
}

// ───────────────────────────── overlay ─────────────────────────────

function OverlaySettings() {
  const { s, set } = useSettings();
  const capture = useApp((st) => st.live?.capture);
  return (
    <div className="stack">
      <Card>
        <CardHead title="Overlay" sub="What to show, and where, is set on the Overlay page." />
        <Setting title="Show the overlay" desc="Master switch. You can also toggle it with your hotkey.">
          <Switch checked={s.overlayEnabled} onChange={(v) => set({ overlayEnabled: v })} label="Show the overlay" />
        </Setting>
        <Setting title="Keep the overlay above the game" desc="Windows can push other windows over the overlay. When this is on, FPSFramer politely brings it back to the front every moment or two.">
          <Switch checked={s.overlay.reassertTopmost} onChange={(v) => set({ overlay: { reassertTopmost: v } })} label="Keep on top" />
        </Setting>
        <Setting title="Hide the overlay from screenshots and recordings" desc="Asks Windows to leave the overlay out of screen captures, so streams and videos stay clean. Works on Windows 10 (2004) and newer; FPSFramer’s own screenshots then won’t include it either.">
          <Switch checked={s.overlay.excludeFromCapture} onChange={(v) => set({ overlay: { excludeFromCapture: v } })} label="Hide from captures" />
        </Setting>
        <div className="row wrap" style={{ marginTop: 8 }}>
          <Button onClick={() => void act('overlay.preview', { seconds: 10 })}>
            <Eye size={15} /> Show it for 10 seconds
          </Button>
          <Button variant="ghost" onClick={() => navigate('/overlay')}>
            Go to the Overlay page
          </Button>
        </div>
      </Card>

      <Disclosure title="What works with the overlay">
        <div className="stack" style={{ gap: '0.5rem' }}>
          <p className="muted" style={{ margin: 0 }}>
            FPSFramer draws its overlay in a separate transparent window that sits above the game. It never injects into the game, which keeps it safe with anti-cheat systems, but it also means:
          </p>
          <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
            <li>
              <b>Borderless or windowed</b> games always work. This is the recommended mode.
            </li>
            <li>
              <b>Exclusive fullscreen</b> games take over the display, so Windows doesn’t show other windows above them. Switch the game to “Borderless” or “Windowed fullscreen”. Most modern DirectX 11/12 games use a flip model that works either way.
            </li>
            <li>
              <b>FPS readings</b> work for DirectX 9, 10, 11 and 12. Vulkan and OpenGL games show hardware readings in the overlay but no FPS yet.
            </li>
            <li>
              <b>Hardware readings</b> (usage, temperatures, memory…) work with any game.
            </li>
          </ul>
          {capture && capture.state === 'tracking' && capture.api && <Badge tone="ok">Right now: {capture.name ?? capture.exe} is using {capture.api}</Badge>}
        </div>
      </Disclosure>
    </div>
  );
}

// ───────────────────────────── backup ─────────────────────────────

interface BackupInfo {
  name: string;
  size: number;
  modified: number;
}

function BackupSettings() {
  const toast = useApp((st) => st.toast);
  const [list, setList] = useState<BackupInfo[] | null>(null);
  const load = () =>
    rpc<BackupInfo[]>('backup.list')
      .then(setList)
      .catch(() => setList([]));
  useEffect(() => {
    void load();
  }, []);
  const { s, set } = useSettings();

  const create = async () => {
    if ((await act('backup.create', {}, 'Backup created')) !== undefined) void load();
  };
  const exportAll = async () => {
    const r = await act<{ filename: string; json: string }>('backup.export');
    if (r && (await saveTextFile(r.filename, r.json))) toast({ level: 'success', message: 'Backup file saved. It contains your settings, profiles, layouts and alerts.' });
  };
  const importFile = async () => {
    const text = await openTextFile('.json');
    if (text === null) return;
    const ok = await confirmAction({ title: 'Restore from this backup file?', body: 'Your current settings, profiles, layouts and alerts will be replaced. A safety copy of the current ones is made first. Recorded sessions are not affected.', confirmLabel: 'Restore', danger: true });
    if (ok && (await act('backup.import', { json: text }, 'Backup restored')) !== undefined) void load();
  };
  const restore = async (b: BackupInfo) => {
    const ok = await confirmAction({ title: 'Go back to this backup?', body: `Your settings, profiles, layouts and alerts return to how they were on ${formatDateTime(b.modified)}. A safety copy of the current ones is made first.`, confirmLabel: 'Go back', danger: true });
    if (ok && (await act('backup.restore', { name: b.name }, 'Backup restored')) !== undefined) void load();
  };
  const reset = async () => {
    const ok = await confirmAction({ title: 'Reset all settings?', body: 'Settings return to their defaults. Your profiles, layouts, alerts and sessions are kept, and a backup is made first.', confirmLabel: 'Reset settings', danger: true });
    if (ok) await act('settings.reset', {}, 'Settings reset');
  };

  return (
    <div className="stack">
      <Card>
        <CardHead title="Back up and restore" sub="Settings, profiles, layouts and alerts in one file. Recorded sessions are stored separately and aren’t included." />
        <div className="row wrap">
          <Button variant="primary" onClick={() => void exportAll()}>
            Save a backup file…
          </Button>
          <Button onClick={() => void importFile()}>Restore from a file…</Button>
          <Button onClick={() => void create()}>Make a quick backup</Button>
          <Button variant="ghost" onClick={() => void act('system.openFolder', { kind: 'backups' })}>
            Open backup folder
          </Button>
        </div>
        <Setting title="Back up automatically" desc="Keeps a dated copy each time FPSFramer starts, plus a copy before risky changes (the last few are kept).">
          <Switch checked={s.autoBackup} onChange={(v) => set({ autoBackup: v })} label="Automatic backups" />
        </Setting>
      </Card>

      <Disclosure title="Saved backups" sub={list ? `${list.length}` : undefined}>
        {list === null ? (
          <div className="skeleton" style={{ height: 80 }} />
        ) : list.length === 0 ? (
          <p className="muted" style={{ margin: 0 }}>No backups yet.</p>
        ) : (
          <div className="stack" style={{ gap: 0 }}>
            {list.slice(0, 12).map((b) => (
              <div key={b.name} className="setting">
                <div className="grow">
                  <div className="setting-title">{formatDateTime(b.modified)}</div>
                  <div className="setting-desc">
                    {b.name.replace(/^fpsframer-|\.json$/g, '')} · {formatBytes(b.size)}
                  </div>
                </div>
                <div className="ctrl">
                  <Button size="sm" onClick={() => void restore(b)}>
                    Go back to this
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Disclosure>

      <Disclosure title="Start over">
        <Setting title="Reset settings to defaults" desc="Handy if something is acting strangely. Nothing is deleted without a backup first.">
          <Button variant="danger" onClick={() => void reset()}>
            <RotateCcw size={15} /> Reset settings
          </Button>
        </Setting>
      </Disclosure>
    </div>
  );
}

// ───────────────────────────── about ─────────────────────────────

function About() {
  const { version, platform, paths, demo } = useApp();
  return (
    <div className="stack">
      <Card>
        <div className="row" style={{ gap: '1rem' }}>
          <img src="/logo.svg" width={56} height={56} alt="" />
          <div>
            <div style={{ fontSize: '1.25rem', fontWeight: 700 }}>FPSFramer</div>
            <div className="muted">
              Version {version} · {demo ? 'demo mode' : platform}
            </div>
          </div>
        </div>
      </Card>

      <Card>
        <CardHead title="Your privacy" />
        <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
          <li>Everything FPSFramer measures stays on your PC. It has no accounts, no telemetry and no analytics.</li>
          <li>It doesn’t connect to the internet by itself. It only opens a web page when you click a link in Help.</li>
          <li>It doesn’t read or change the memory of your games and never injects code into them.</li>
          <li>Companion tools (HWiNFO, LibreHardwareMonitor) are read locally, on your own PC.</li>
        </ul>
      </Card>

      <Disclosure title="Where things are stored">
        <div className="stack" style={{ gap: '0.5rem' }}>
          <div>
            <div className="faint" style={{ fontSize: '0.8125rem' }}>Settings, profiles, layouts and alerts</div>
            <code className="num">{paths.config}</code>
          </div>
          <div>
            <div className="faint" style={{ fontSize: '0.8125rem' }}>Recorded sessions and logs</div>
            <code className="num">{paths.data}</code>
          </div>
          <div className="row wrap">
            <Button onClick={() => void act('system.openFolder', { kind: 'config' })}>Open settings folder</Button>
            <Button onClick={() => void act('system.openFolder', { kind: 'data' })}>Open data folder</Button>
            <Button onClick={() => void act('system.openFolder', { kind: 'logs' })}>Open logs</Button>
          </div>
        </div>
      </Disclosure>

      <Disclosure title="Open-source components">
        <p className="muted" style={{ margin: 0 }}>
          Built with Tauri, React and Rust. Fonts: Inter and JetBrains Mono (SIL Open Font License). Icons: Lucide (ISC). Hardware readings use Windows performance counters, ETW, NVIDIA’s NVML where present, and optional companion tools. FPSFramer is independent software and is not affiliated with any other FPS-monitoring product.
        </p>
      </Disclosure>
    </div>
  );
}
