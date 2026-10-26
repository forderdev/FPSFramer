import { AlertTriangle, BookOpen, CheckCircle2, ChevronDown, ClipboardCopy, Info, LifeBuoy, RefreshCw, Shield, Stethoscope, Thermometer, Wrench, XCircle } from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';
import { Badge, Banner, Button, Card, CardHead, Disclosure, Empty, PageHead, SearchBox, Segmented, Tabs } from '../components/ui';
import { GLOSSARY } from '../lib/glossary';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { DiagCheck } from '../lib/types';

type TabId = 'check' | 'sensors' | 'permission' | 'fixes' | 'compat' | 'glossary';
const TABS: { id: TabId; label: string; icon: ReactNode }[] = [
  { id: 'check', label: 'Check my setup', icon: <Stethoscope size={15} /> },
  { id: 'fixes', label: 'Troubleshooting', icon: <Wrench size={15} /> },
  { id: 'sensors', label: 'Missing sensors', icon: <Thermometer size={15} /> },
  { id: 'permission', label: 'FPS permission', icon: <LifeBuoy size={15} /> },
  { id: 'compat', label: 'Games & anti-cheat', icon: <Shield size={15} /> },
  { id: 'glossary', label: 'Glossary', icon: <BookOpen size={15} /> },
];

export default function HelpPage({ tab }: { tab?: string }) {
  const current: TabId = TABS.some((t) => t.id === tab) ? (tab as TabId) : 'check';
  return (
    <div className="page">
      <PageHead title="Help & diagnostics" sub="Find out why something isn’t showing, and how to fix it. Nothing here changes your PC unless you press a button that says so." />
      <Tabs value={current} tabs={TABS} onChange={(t) => navigate(`/help/${t}`)} />
      {current === 'check' && <Diagnostics />}
      {current === 'fixes' && <Troubleshooting />}
      {current === 'sensors' && <SensorsGuide />}
      {current === 'permission' && <PermissionGuide />}
      {current === 'compat' && <Compatibility />}
      {current === 'glossary' && <Glossary />}
    </div>
  );
}

// ───────────────────────────── diagnostics ─────────────────────────────

const STATUS_ICON = {
  ok: <CheckCircle2 size={20} aria-label="Working" style={{ color: 'var(--ok)' }} />,
  info: <Info size={20} aria-label="For your information" style={{ color: 'var(--info)' }} />,
  warn: <AlertTriangle size={20} aria-label="Worth a look" style={{ color: 'var(--warn)' }} />,
  fail: <XCircle size={20} aria-label="Problem" style={{ color: 'var(--bad)' }} />,
} as const;

function CheckRow({ c, onFix }: { c: DiagCheck; onFix: (f: NonNullable<DiagCheck['fix']>) => void }) {
  return (
    <div className="setting" style={{ alignItems: 'flex-start' }}>
      <div style={{ paddingTop: 2 }}>{STATUS_ICON[c.status]}</div>
      <div className="grow">
        <div className="setting-title">{c.title}</div>
        <div className="setting-desc" style={{ color: 'var(--text)' }}>{c.summary}</div>
        {c.details.length > 0 && (
          <details style={{ marginTop: 6 }} open={c.status === 'fail' || c.status === 'warn'}>
            <summary className="muted" style={{ cursor: 'pointer' }}>Details</summary>
            <ul className="muted" style={{ margin: '0.4rem 0 0', paddingLeft: '1.2rem', fontSize: '0.875rem' }}>
              {c.details.map((d, i) => (
                <li key={i}>{d}</li>
              ))}
            </ul>
          </details>
        )}
      </div>
      {c.fix && (
        <Button size="sm" variant={c.status === 'fail' ? 'primary' : 'default'} onClick={() => onFix(c.fix!)}>
          {c.fix.label}
        </Button>
      )}
    </div>
  );
}

function Diagnostics() {
  const toast = useApp((s) => s.toast);
  const advanced = useApp((s) => s.settings.uiMode === 'advanced');
  const [checks, setChecks] = useState<DiagCheck[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [logs, setLogs] = useState<{ t: number; level: string; message: string }[] | null>(null);

  const run = async () => {
    setBusy(true);
    const r = await act<DiagCheck[]>('diagnostics.run');
    setChecks(r ?? []);
    setBusy(false);
  };
  useEffect(() => {
    void run();
  }, []);

  const copyReport = async () => {
    const text = await act<string>('diagnostics.report');
    if (text === undefined) return;
    try {
      await navigator.clipboard.writeText(text);
      toast({ level: 'success', message: 'Report copied. Paste it into a message when asking for help. It contains no personal information.' });
    } catch {
      toast({ level: 'error', message: 'Couldn’t copy automatically. Open the logs folder in Settings → About instead.' });
    }
  };

  const runFix = async (f: NonNullable<DiagCheck['fix']>) => {
    if (f.action === 'openRoute' && f.arg) return navigate(f.arg);
    if (f.action === 'retryCapture') {
      await act('capture.retry', {}, 'Trying again…');
      setTimeout(() => void run(), 1500);
    } else if (f.action === 'grantCapture') {
      navigate('/help/permission');
    }
  };

  // Only what needs a look is shown up front. What passed is one click away.
  const attention = checks?.filter((c) => c.status !== 'ok') ?? [];
  const passed = checks?.filter((c) => c.status === 'ok') ?? [];
  const problems = attention.filter((c) => c.status === 'fail' || c.status === 'warn').length;

  return (
    <div className="stack">
      <Card>
        <CardHead
          title={checks === null ? 'Checking…' : problems === 0 ? 'Everything looks good' : `${problems} ${problems === 1 ? 'thing' : 'things'} worth a look`}
          sub={checks === null ? undefined : attention.length === 0 ? 'Nothing needs your attention.' : 'Here is what to look at.'}
          right={
            <div className="row wrap">
              <Button size="sm" onClick={() => void run()} disabled={busy}>
                <RefreshCw size={14} /> Check again
              </Button>
              <Button size="sm" onClick={() => void copyReport()}>
                <ClipboardCopy size={14} /> Copy report
              </Button>
            </div>
          }
        />
        {checks === null ? (
          <div className="skeleton" style={{ height: 120 }} />
        ) : (
          attention.length > 0 && (
            <div className="stack" style={{ gap: 0 }}>
              {attention.map((c) => (
                <CheckRow key={c.id} c={c} onFix={(f) => void runFix(f)} />
              ))}
            </div>
          )
        )}
      </Card>

      {passed.length > 0 && (
        <Disclosure title="Everything that passed" sub={`${passed.length} ${passed.length === 1 ? 'check' : 'checks'}`}>
          <div className="stack" style={{ gap: 0 }}>
            {passed.map((c) => (
              <CheckRow key={c.id} c={c} onFix={(f) => void runFix(f)} />
            ))}
          </div>
        </Disclosure>
      )}

      {advanced && (
        <Disclosure title="Recent log" sub="For troubleshooting">
          <div className="stack">
            <p className="muted" style={{ margin: 0 }}>Technical messages. Personal folder names are hidden.</p>
            <div>
              <Button size="sm" onClick={async () => setLogs((await act<{ t: number; level: string; message: string }[]>('logs.recent', { n: 120 })) ?? [])}>
                {logs ? 'Refresh' : 'Show the log'}
              </Button>
            </div>
            {logs && (
              <pre className="num" style={{ margin: 0, maxHeight: '18rem', overflow: 'auto', fontSize: '0.75rem', whiteSpace: 'pre-wrap' }}>
                {logs.map((l) => `${new Date(l.t).toLocaleTimeString()} ${l.level.padEnd(5)} ${l.message}`).join('\n') || 'Nothing logged yet.'}
              </pre>
            )}
          </div>
        </Disclosure>
      )}
    </div>
  );
}

// ───────────────────────────── troubleshooting ─────────────────────────────

interface Qa {
  q: string;
  a: ReactNode;
  tags: string;
}

const QA: Qa[] = [
  {
    q: 'The overlay shows nothing, or “FPS: N/A”',
    tags: 'fps overlay empty missing dash',
    a: (
      <>
        <p>FPS only appears while a DirectX game is actively drawing. Check these in order:</p>
        <ol>
          <li>Is a game running and in front? FPSFramer finds it automatically after a second or two.</li>
          <li>
            Did Windows give FPSFramer permission to measure? See <a href="#/help/permission">FPS permission</a>.
          </li>
          <li>Does the game use Vulkan or OpenGL? FPS can’t be measured for those yet. Check “Games & anti-cheat”. Hardware readings still work.</li>
          <li>Is it in exclusive fullscreen? Switch to borderless/windowed so the overlay can show above it.</li>
        </ol>
      </>
    ),
  },
  {
    q: 'I can’t see the overlay in my game',
    tags: 'overlay hidden fullscreen invisible not showing',
    a: (
      <>
        <p>
          The overlay is a separate window above the game. Games in <b>exclusive fullscreen</b> hide it. In the game’s video settings choose <b>Borderless</b> or <b>Windowed fullscreen</b>, then try again.
        </p>
        <p>Also check the overlay isn’t switched off (hotkey on the Hotkeys settings tab), and use “Show it for 10 seconds” on Settings → Overlay to test it on the desktop.</p>
      </>
    ),
  },
  {
    q: 'CPU temperature, fan speed or voltage says “Not available”',
    tags: 'temperature cpu fan voltage missing companion',
    a: (
      <p>
        Windows doesn’t share these with ordinary programs. Run a free companion tool (HWiNFO or LibreHardwareMonitor) alongside FPSFramer and it will fill them in. See <a href="#/help/sensors">Missing sensors</a>. FPSFramer never shows an invented number: if it can’t measure something it says so.
      </p>
    ),
  },
  {
    q: 'My numbers don’t match another monitoring tool',
    tags: 'different numbers compare accuracy 1% low',
    a: (
      <p>
        Small differences are normal. Tools average over different time spans, and “1% low” has several definitions. FPSFramer uses the average of your slowest 1% of frames; you can switch to the 99th-percentile method in Settings → Monitoring (Advanced mode). FPS is measured from real frame presentation times using Windows’ own tracing, the same source many other tools use.
      </p>
    ),
  },
  {
    q: 'The game stutters but FPS looks fine',
    tags: 'stutter hitch spikes smooth',
    a: (
      <p>
        Average FPS hides short hitches. Open the session afterwards and look at <b>Spikes</b> and the frame-time graph. FPSFramer points out when they happened and what else was going on (memory filling up, a hot processor, disk activity). Common fixes are lowering texture quality when video memory is full, closing background programs, and cooling.
      </p>
    ),
  },
  {
    q: 'FPS is stuck at exactly 60 (or 144…)',
    tags: 'capped vsync limiter locked',
    a: (
      <p>
        That’s almost always V-Sync or a frame limiter holding the game to your screen’s refresh rate. It’s not a problem. Turn it off in the game’s settings if you want to see how fast your PC can go.
      </p>
    ),
  },
  {
    q: 'A hotkey does nothing',
    tags: 'hotkey shortcut keys not working',
    a: (
      <p>Another program (often a graphics driver tool or chat app) may already use that combination. Choose another one in Settings → Hotkeys. FPSFramer will tell you if Windows refuses a shortcut.</p>
    ),
  },
  {
    q: 'Does FPSFramer slow my game down?',
    tags: 'performance overhead cpu usage lightweight',
    a: (
      <p>
        Very little. It reads counters once or twice a second and listens to Windows’ frame-timing events; it doesn’t hook into the game. If you want to be sure, close the FPSFramer window. The overlay and recording keep running from the tray with minimal resources. You can slow the update rate in Settings → Monitoring.
      </p>
    ),
  },
  {
    q: 'Where did my sessions and settings go?',
    tags: 'data lost backup restore missing',
    a: (
      <p>
        Everything is stored locally. See Settings → About & privacy for the folders. Settings are backed up automatically; use Settings → Backup & reset to go back to an earlier copy. Deleted sessions can be restored with the Undo button right after you delete them.
      </p>
    ),
  },
];

function Troubleshooting() {
  const [q, setQ] = useState('');
  const shown = QA.filter((x) => !q.trim() || `${x.q} ${x.tags}`.toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="stack">
      <SearchBox value={q} onChange={setQ} placeholder="Describe the problem, like “no FPS” or “overlay”" label="Search troubleshooting" />
      {shown.length === 0 ? (
        <Card>
          <Empty title="Nothing matches">
            Try the <a href="#/help/check">setup check</a>. It finds most problems automatically.
          </Empty>
        </Card>
      ) : (
        <Card style={{ padding: 0 }}>
          {shown.map((x) => (
            <details key={x.q} className="qa">
              <summary>
                <ChevronDown size={16} aria-hidden className="chev" />
                {x.q}
              </summary>
              <div className="qa-body">{x.a}</div>
            </details>
          ))}
        </Card>
      )}
    </div>
  );
}

// ───────────────────────────── sensors guide ─────────────────────────────

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <div className="row" style={{ alignItems: 'flex-start', gap: '0.8rem' }}>
      <span style={{ flex: 'none', width: 28, height: 28, borderRadius: '50%', background: 'var(--accent)', color: 'var(--accent-contrast)', display: 'grid', placeItems: 'center', fontWeight: 700 }} aria-hidden>
        {n}
      </span>
      <div>
        <div style={{ fontWeight: 650 }}>{title}</div>
        <div className="muted">{children}</div>
      </div>
    </div>
  );
}

function SensorsGuide() {
  const { settings, status } = useApp();
  const update = useApp((s) => s.updateSettings);
  const [tool, setTool] = useState<'hwinfo' | 'lhm'>('hwinfo');
  const hw = status?.providers.find((p) => /hwinfo/i.test(p.id));
  const lhm = status?.providers.find((p) => /libre|lhm/i.test(p.id));
  return (
    <div className="stack">
      <p className="muted" style={{ margin: 0, maxWidth: '44rem' }}>
        Windows keeps CPU temperature, fan speeds and voltages behind a hardware driver that normal programs can’t load. A free helper tool can, and FPSFramer reads the values it publishes. Nothing is guessed or estimated.
      </p>

      <Segmented
        label="Helper tool"
        value={tool}
        options={[
          { value: 'hwinfo', label: 'HWiNFO' },
          { value: 'lhm', label: 'LibreHardwareMonitor' },
        ]}
        onChange={setTool}
      />

      {tool === 'hwinfo' ? (
        <Card>
          <CardHead
            title="HWiNFO"
            sub="Most complete. Free for personal use."
            right={hw ? <Badge tone={hw.health === 'ok' ? 'ok' : 'warn'}>{hw.health === 'ok' ? `Connected, ${hw.sensors} sensors` : 'Not connected'}</Badge> : undefined}
          />
          <div className="stack">
            <Step n={1} title="Install and start HWiNFO in “Sensors-only” mode">
              Download it from hwinfo.com and choose “Sensors-only” at startup.
            </Step>
            <Step n={2} title="Turn on Shared Memory Support">
              In HWiNFO open <b>Settings</b> and tick <b>Shared Memory Support</b>. (HWiNFO’s free edition switches it off after 12 hours; start it again if sensors disappear.)
            </Step>
            <Step n={3} title="Tell FPSFramer to use it">
              <Button size="sm" onClick={() => void update({ companions: { hwinfo: !settings.companions.hwinfo } })} aria-pressed={settings.companions.hwinfo}>
                {settings.companions.hwinfo ? 'Stop using HWiNFO' : 'Use HWiNFO'}
              </Button>
            </Step>
            <div>
              <Button variant="ghost" onClick={() => void act('system.openUrl', { url: 'https://www.hwinfo.com/' })}>
                Open hwinfo.com
              </Button>
            </div>
          </div>
        </Card>
      ) : (
        <Card>
          <CardHead
            title="LibreHardwareMonitor"
            sub="Open source."
            right={lhm ? <Badge tone={lhm.health === 'ok' ? 'ok' : 'warn'}>{lhm.health === 'ok' ? `Connected, ${lhm.sensors} sensors` : 'Not connected'}</Badge> : undefined}
          />
          <div className="stack">
            <Step n={1} title="Install and run LibreHardwareMonitor as administrator">
              It needs administrator rights to load its sensor driver (right-click → Run as administrator).
            </Step>
            <Step n={2} title="Leave it open while you play">
              FPSFramer reads the values it publishes through Windows’ standard WMI interface. If sensors don’t appear within a minute, open <b>Check my setup</b>. It shows whether the program was found.
            </Step>
            <Step n={3} title="Tell FPSFramer to use it">
              <Button size="sm" onClick={() => void update({ companions: { libreHardwareMonitor: !settings.companions.libreHardwareMonitor } })} aria-pressed={settings.companions.libreHardwareMonitor}>
                {settings.companions.libreHardwareMonitor ? 'Stop using LibreHardwareMonitor' : 'Use LibreHardwareMonitor'}
              </Button>
            </Step>
            <div>
              <Button variant="ghost" onClick={() => void act('system.openUrl', { url: 'https://github.com/LibreHardwareMonitor/LibreHardwareMonitor' })}>
                Open the project page
              </Button>
            </div>
          </div>
        </Card>
      )}

      <Disclosure title="What works without any helper">
        <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
          <li>FPS, frame time and the 1% / 0.1% lows (DirectX games).</li>
          <li>CPU usage and speed, per core. Memory, disk and network.</li>
          <li>NVIDIA graphics cards: usage, temperature, clocks, memory, power and fan speed.</li>
          <li>AMD and Intel graphics cards: usage and memory. Their temperature, clocks and fan need a helper.</li>
        </ul>
      </Disclosure>
    </div>
  );
}

// ───────────────────────────── permission guide ─────────────────────────────

function PermissionGuide() {
  const { status } = useApp();
  const toast = useApp((s) => s.toast);
  const cap = status?.capture;
  const elevated = status?.elevated;
  const granted = cap?.state === 'tracking' || cap?.state === 'searching' || cap?.state === 'stalled';
  return (
    <div className="stack">
      {cap?.state === 'needsPermission' ? (
        <Banner tone="warn" title="FPSFramer can’t measure FPS yet">
          Windows only allows frame-timing measurement for administrators and members of the “Performance Log Users” group.
        </Banner>
      ) : granted ? (
        <Banner tone="ok" title="FPS measurement is allowed">
          {elevated ? 'FPSFramer is running as administrator.' : 'FPSFramer has the permission it needs.'}
        </Banner>
      ) : null}

      <Card>
        <CardHead title="Allow it once" sub="Recommended. Adds your Windows account to the Performance Log Users group. You’ll see a Windows prompt." />
        <div className="stack">
          <ol className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
            <li>Press the button and approve the Windows prompt.</li>
            <li>
              <b>Sign out of Windows and back in</b> (or restart). Windows only applies group changes at sign-in.
            </li>
            <li>Start FPSFramer again. It will measure FPS from then on, no matter how you start it.</li>
          </ol>
          <div className="row wrap">
            <Button
              variant="primary"
              onClick={async () => {
                const r = await act<{ granted: boolean; note: string }>('capture.grantPermission');
                if (r) toast({ level: 'success', message: r.note });
              }}
            >
              Allow FPS measurement
            </Button>
            <Button
              variant="ghost"
              title="Works immediately, but you need to do it every time FPSFramer starts."
              onClick={async () => {
                const ok = await act('capture.restartElevated');
                if (ok !== undefined) toast({ level: 'info', message: 'Restarting as administrator…' });
              }}
              disabled={elevated}
            >
              {elevated ? 'Already running as administrator' : 'Or restart as administrator'}
            </Button>
          </div>
        </div>
      </Card>

      <Disclosure title="Why does it need permission?">
        <p className="muted" style={{ margin: 0 }}>
          To measure FPS without touching your games, FPSFramer listens to the frame-presentation events that Windows itself records (the same facility Microsoft’s own tools use). Windows protects that facility, so a program needs either administrator rights or membership in the built-in <b>Performance Log Users</b> group. Hardware readings (usage, memory, temperatures) never need this.
        </p>
      </Disclosure>
    </div>
  );
}

// ───────────────────────────── compatibility ─────────────────────────────

function Compatibility() {
  return (
    <div className="stack">
      <Card>
        <CardHead title="What FPSFramer can and can’t do" sub="An honest list, so you know what to expect." />
        <table className="table">
          <thead>
            <tr>
              <th>Game type</th>
              <th>FPS &amp; frame time</th>
              <th>Hardware readings</th>
              <th>Overlay</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>DirectX 9 to 12, borderless or windowed</td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
            </tr>
            <tr>
              <td>Exclusive fullscreen</td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
              <td>
                <Badge tone="warn">Often hidden</Badge>
              </td>
            </tr>
            <tr>
              <td>Vulkan or OpenGL</td>
              <td>
                <Badge tone="bad">Not yet</Badge>
              </td>
              <td>
                <Badge tone="ok">Yes</Badge>
              </td>
              <td>
                <Badge tone="ok">Yes (windowed)</Badge>
              </td>
            </tr>
          </tbody>
        </table>
        <p className="faint" style={{ marginBottom: 0, fontSize: '0.8125rem' }}>
          “Often hidden”: Windows draws exclusive-fullscreen games straight to the display, so no other window can appear above them. Choose borderless or windowed in the game to see the overlay. Individual games and drivers can behave differently.
        </p>
      </Card>

      <Disclosure title="Anti-cheat and online games">
        <div className="stack" style={{ gap: '0.6rem' }}>
          <p className="muted" style={{ margin: 0 }}>
            FPSFramer was designed to stay on the right side of anti-cheat systems:
          </p>
          <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
            <li>It never injects code into a game, hooks its graphics calls, or reads or writes its memory.</li>
            <li>FPS comes from Windows’ own event tracing; the overlay is an ordinary separate window.</li>
            <li>It does not try to hide from, or work around, any anti-cheat software.</li>
          </ul>
          <p className="muted" style={{ margin: 0 }}>
            That approach is the same one used by several widely accepted monitoring tools, but <b>no tool can promise that a particular game’s anti-cheat will accept it</b>. Anti-cheat vendors set their own rules, and some competitive games forbid all overlays. If a game’s rules say to avoid overlays, or you are unsure, turn the overlay off with your hotkey. FPSFramer will still record your session. Use of any third-party tool is at your own discretion.
          </p>
        </div>
      </Disclosure>

      <Disclosure title="What FPSFramer does not do">
        <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
          <li>
            <b>FPS limiting</b>: doing it properly means hooking the game, which conflicts with anti-cheat. Use the game’s own limiter or your graphics driver’s (see Games &amp; profiles).
          </li>
          <li>
            <b>Overclocking or fan control</b>: FPSFramer only reads values; it never changes your hardware.
          </li>
        </ul>
      </Disclosure>
    </div>
  );
}

// ───────────────────────────── glossary ─────────────────────────────

function Glossary() {
  const [q, setQ] = useState('');
  const shown = GLOSSARY.filter((t) => !q.trim() || `${t.term} ${t.short} ${t.long}`.toLowerCase().includes(q.toLowerCase()));
  return (
    <div className="stack">
      <SearchBox value={q} onChange={setQ} placeholder="Look up a word" label="Search the glossary" />
      {shown.length === 0 ? (
        <Card>
          <Empty title="No matching term" />
        </Card>
      ) : (
        <Card style={{ padding: 0 }}>
          {shown.map((t) => (
            <details key={t.term} className="qa">
              <summary>
                <ChevronDown size={16} aria-hidden className="chev" />
                <span>
                  <b>{t.term}</b> <span className="muted" style={{ fontWeight: 400 }}>{t.short}</span>
                </span>
              </summary>
              <div className="qa-body">{t.long}</div>
            </details>
          ))}
        </Card>
      )}
    </div>
  );
}
