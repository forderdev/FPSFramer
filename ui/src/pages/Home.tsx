import { Camera, Circle, Cpu, Eye, Gamepad2, HardDrive, MemoryStick, Monitor, Network, PictureInPicture2, Plus, Settings2, Square, Zap } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Sparkline } from '../components/Charts';
import { MetricTile, Reading } from '../components/shared';
import { Badge, Banner, Button, Card, CardHead, Chip, InfoTip, Kbd, PageHead, Switch } from '../components/ui';
import { formatClock, formatReading, relativeTime, statusOf } from '../lib/format';
import { termHelp } from '../lib/glossary';
import { clone, newId, VERDICT_LABEL } from '../lib/labels';
import { navigate } from '../lib/router';
import { useFrames } from '../lib/series';
import { act, currentProfile, useApp } from '../lib/store';
import type { Profile, SessionListItem } from '../lib/types';

function useLastSession(): SessionListItem | null {
  const version = useApp((s) => s.sessionsVersion);
  const [item, setItem] = useState<SessionListItem | null>(null);
  useEffect(() => {
    let alive = true;
    void act<SessionListItem[]>('sessions.list').then((l) => alive && setItem(l?.[0] ?? null));
    return () => {
      alive = false;
    };
  }, [version]);
  return item;
}

export function HomePage() {
  const { live, profiles, catalogById } = useApp();
  const last = useLastSession();
  const recording = live?.session;
  const cap = live?.capture;
  const tracking = cap?.state === 'tracking' || cap?.state === 'stalled';
  const profile = currentProfile(profiles, live);

  return (
    <div className="page">
      <PageHead
        title="Home"
        sub="Your PC’s performance at a glance. Everything updates live."
        actions={
          <>
            <Button onClick={() => void act('screenshots.take')} title="Save a picture of your screen">
              <Camera size={16} /> Screenshot
            </Button>
            {recording ? (
              <Button variant="danger" onClick={() => void act('sessions.stopRecording')}>
                <Square size={14} fill="currentColor" /> Stop recording · {formatClock(recording.elapsedS)}
              </Button>
            ) : (
              <Button onClick={() => void act('sessions.startRecording')} title="Record a session by hand. Games are recorded automatically.">
                <Circle size={14} fill="var(--bad)" color="var(--bad)" /> Record a session
              </Button>
            )}
          </>
        }
      />

      <Attention />

      <Card>
        <div className="hero">
          <div className="stack" style={{ gap: '0.3rem' }}>
            <div className="row" style={{ gap: '0.4rem' }}>
              <span className="muted" style={{ fontWeight: 650 }}>
                Frame rate
              </span>
              <InfoTip text={termHelp('FPS')} />
              {profile?.targetFps && (
                <Badge tone="accent">
                  Target {profile.targetFps} FPS
                </Badge>
              )}
            </div>
            <FpsHero />
            <div className="muted" style={{ minHeight: '1.5rem' }}>
              {tracking ? (
                <span className="row wrap" style={{ gap: '0.5rem' }}>
                  <Gamepad2 size={16} aria-hidden /> <b style={{ color: 'var(--text)' }}>{cap?.name}</b>
                  {cap?.api && <Badge>{cap.api}</Badge>}
                  {profile && <Badge tone="info">Profile: {profile.name}</Badge>}
                </span>
              ) : (
                <GameHint />
              )}
            </div>
          </div>
          <div className="stack">
            <div className="grid cols-4" style={{ gap: '0.75rem' }}>
              {[
                { id: 'frametime', label: 'Frame time', term: 'Frame time' },
                { id: 'fps.low1', label: '1% Low', term: '1% Low' },
                { id: 'fps.low01', label: '0.1% Low', term: '0.1% Low' },
                { id: 'fps.avg', label: 'Average', term: 'FPS' },
              ].map((m) => (
                <div key={m.id} className="stack" style={{ gap: 2 }}>
                  <span className="faint row" style={{ fontSize: '0.8125rem', gap: 4 }}>
                    {m.label} <InfoTip text={catalogById[m.id]?.description ?? termHelp(m.term)} />
                  </span>
                  <b className="num" style={{ fontSize: '1.4rem' }}>
                    <Reading id={m.id} />
                  </b>
                </div>
              ))}
            </div>
            <FrameGraph />
          </div>
        </div>
      </Card>

      <div className="grid cols-3">
        <MetricTile title="Processor" icon={<Cpu size={17} />} id="cpu.load" extra={[{ id: 'cpu.temp', label: 'Temp' }, { id: 'cpu.clock', label: 'Speed' }]} onClick={() => navigate('/monitor?set=cpu')} />
        <MetricTile title="Graphics card" icon={<Monitor size={17} />} id="gpu.load" extra={[{ id: 'gpu.temp', label: 'Temp' }, { id: 'gpu.vram.percent', label: 'Video memory' }]} onClick={() => navigate('/monitor?set=gpu')} />
        <MetricTile title="Memory" icon={<MemoryStick size={17} />} id="ram.percent" extra={[{ id: 'ram.used', label: 'Used' }, { id: 'ram.total', label: 'of' }]} onClick={() => navigate('/monitor?set=memory')} />
        <MetricTile title="Storage" icon={<HardDrive size={17} />} id="disk.active" extra={[{ id: 'disk.read', label: 'Read' }, { id: 'disk.write', label: 'Write' }]} onClick={() => navigate('/monitor?set=storage')} />
        <MetricTile title="Network" icon={<Network size={17} />} id="net.down" extra={[{ id: 'net.up', label: 'Upload' }]} onClick={() => navigate('/monitor?set=network')} />
        <MetricTile title="Graphics power" icon={<Zap size={17} />} id="gpu.power" extra={[{ id: 'gpu.fan', label: 'Fan' }, { id: 'cpu.power', label: 'CPU power' }]} onClick={() => navigate('/monitor?set=power')} />
      </div>

      <div className="grid cols-2">
        <OverlayCard />
        <Card>
          <CardHead title="Last session" sub="Recorded automatically while you play" />
          {last ? (
            <div className="stack">
              <div className="row wrap" style={{ gap: '0.6rem' }}>
                <b style={{ fontSize: '1.1rem' }}>{last.meta.name || last.meta.gameName || 'Desktop recording'}</b>
                <span className="muted">{relativeTime(last.meta.startedAt)}</span>
                {last.meta.demo && <Badge tone="warn">Demo</Badge>}
              </div>
              <div className="row wrap" style={{ gap: '1.5rem' }}>
                <Stat label="Average" v={last.fpsAvg} unit="FPS" />
                <Stat label="1% Low" v={last.low1} unit="FPS" />
                <Stat label="Spikes" v={last.spikeCount} />
                {last.verdict && <Badge tone={last.verdict === 'unknown' ? undefined : 'accent'}>{VERDICT_LABEL[last.verdict]}</Badge>}
              </div>
              <div className="row">
                <Button variant="primary" onClick={() => navigate(`/sessions/${last.meta.id}`)}>
                  See what happened
                </Button>
                <Button variant="ghost" onClick={() => navigate('/sessions')}>
                  All sessions
                </Button>
              </div>
            </div>
          ) : (
            <p className="muted">
              Nothing recorded yet. Start a game and FPSFramer records it for you. Afterwards you get a plain-language summary of how it ran, including any stutter, heat or bottlenecks.
            </p>
          )}
        </Card>
      </div>

    </div>
  );
}

function Stat({ label, v, unit }: { label: string; v: number | null; unit?: string }) {
  return (
    <div>
      <div className="faint" style={{ fontSize: '0.8125rem' }}>
        {label}
      </div>
      <b className="num" style={{ fontSize: '1.3rem' }}>
        {v === null ? 'N/A' : Math.round(v)}
      </b>{' '}
      {unit && v !== null && <span className="muted">{unit}</span>}
    </div>
  );
}

function FpsHero() {
  const live = useApp((s) => s.live);
  const desc = useApp((s) => s.catalogById['fps']);
  const settings = useApp((s) => s.settings);
  const raw = live?.values['fps'];
  const f = raw !== undefined ? formatReading(desc, raw, settings, {}) : null;
  const status = statusOf(desc, typeof raw === 'number' ? raw : undefined, live?.profile.targetFps);
  if (f) {
    return (
      <div className={`hero-value val-${status}`} aria-live="off">
        {f.text}
        <span className="tile-unit" style={{ fontSize: '1.4rem', marginLeft: 8 }}>
          FPS
        </span>
      </div>
    );
  }
  const reason = live?.capture;
  return (
    <div className="hero-value na">
      {reason?.state === 'needsPermission' ? 'Needs permission' : reason?.state === 'unsupportedApi' ? 'Not measurable' : 'No game yet'}
    </div>
  );
}

function GameHint() {
  const cap = useApp((s) => s.live?.capture);
  if (cap?.state === 'needsPermission') return <span>FPSFramer needs a one-time permission from Windows to measure FPS. See the box above.</span>;
  if (cap?.state === 'unsupportedApi') return <span>{cap.message}</span>;
  if (cap?.state === 'unsupported' || cap?.state === 'error') return <span>{cap.message}</span>;
  return <span>Start a game and FPSFramer finds it automatically. Use windowed or borderless mode for the overlay.</span>;
}

function FrameGraph() {
  const frames = useFrames(240);
  const live = useApp((s) => s.live);
  const target = live?.profile.targetFps;
  if (frames.length < 8) {
    return (
      <div className="faint" style={{ height: 56, display: 'grid', placeItems: 'center', border: '1px dashed var(--border-strong)', borderRadius: 12, fontSize: '0.8125rem' }}>
        The frame-time graph appears when a game is running
      </div>
    );
  }
  const max = Math.max(20, Math.min(80, Math.max(...frames) * 1.1));
  return (
    <div>
      <div className="row faint" style={{ fontSize: '0.75rem', justifyContent: 'space-between' }}>
        <span>Frame time: lower and flatter is smoother{target ? ` (target ${(1000 / target).toFixed(1)} ms)` : ''}</span>
        <span>last {frames.length} frames</span>
      </div>
      <Sparkline values={frames} height={56} min={0} max={max} color="var(--accent)" />
    </div>
  );
}

/** One box at a time: a critical problem first, then the most useful hint, then a milder warning. Nothing when all is well. */
function Attention() {
  const health = useHealthBanner();
  const hints = useSetupHints();
  const shown = health?.bad ? health.node : (hints[0] ?? health?.node ?? null);
  return shown;
}

function useHealthBanner(): { bad: boolean; node: React.ReactNode } | null {
  const live = useApp((s) => s.live);
  const catalog = useApp((s) => s.catalogById);
  if (!live) return null;
  const alerts = live.alerts;
  const worst = alerts.find((a) => a.severity === 'critical') ?? alerts[0];
  let tone: 'ok' | 'warn' | 'bad' = 'ok';
  let title = '';
  let body = '';
  if (worst) {
    tone = worst.severity === 'critical' ? 'bad' : 'warn';
    title = worst.severity === 'critical' ? 'Needs attention' : 'Heads up';
    body = worst.message + (alerts.length > 1 ? ` (+${alerts.length - 1} more)` : '');
  } else {
    const keys = ['cpu.temp', 'gpu.temp', 'ram.percent', 'gpu.vram.percent', 'cpu.load'];
    for (const k of keys) {
      const v = live.values[k];
      const st = statusOf(catalog[k], typeof v === 'number' ? v : undefined);
      if (st !== 'normal') {
        tone = st === 'critical' ? 'bad' : 'warn';
        title = st === 'critical' ? 'Needs attention' : 'Worth keeping an eye on';
        body = `${catalog[k]?.name ?? k} is ${st === 'critical' ? 'very high' : 'getting high'} right now.`;
        break;
      }
    }
  }
  if (tone === 'ok') return null;
  return {
    bad: tone === 'bad',
    node: (
      <Banner tone={tone} title={title} actions={alerts.length > 0 ? <Button size="sm" onClick={() => navigate('/alerts')}>View alerts</Button> : undefined}>
        {body}
      </Banner>
    ),
  };
}

/** Hints in priority order. Only the first is shown on Home. */
function useSetupHints(): React.ReactNode[] {
  const { live, catalogById, profiles, settings, status } = useApp();
  const update = useApp((s) => s.updateSettings);
  const cap = live?.capture;
  const dismissed = settings.dismissedTips;
  const hints: React.ReactNode[] = [];

  // Windows says the game owns the whole display: no overlay window can be drawn over it.
  if (live?.exclusiveFullscreen && live.gameActive) {
    hints.push(
      <Banner
        key="exclusive"
        tone="warn"
        title="The overlay can’t appear over this game right now"
        actions={
          <Button size="sm" onClick={() => navigate('/help/compat')}>
            Why?
          </Button>
        }
      >
        The game is in <b>exclusive full screen</b>, which Windows reserves for the game alone. In its video settings choose <b>Borderless</b> or <b>Windowed fullscreen</b> and the overlay will show. FPS and recording keep working either way.
      </Banner>,
    );
  }
  if (cap?.state === 'needsPermission') {
    hints.push(
      <Banner
        key="perm"
        tone="warn"
        title="Turn on FPS measurement"
        actions={
          <>
            <Button variant="primary" size="sm" onClick={() => navigate('/help/permission')}>
              Set up now
            </Button>
          </>
        }
      >
        Windows only lets apps measure frame rate with a one-time permission. It takes about 30 seconds. Everything else works without it.
      </Banner>,
    );
  }
  const cpuTemp = catalogById['cpu.temp'];
  if (cpuTemp && !cpuTemp.availability.available && cpuTemp.availability.reason?.code === 'needsCompanion' && !dismissed.includes('tip-cpu-temp')) {
    hints.push(
      <Banner
        key="cputemp"
        tone="info"
        title="Add CPU temperature (optional)"
        actions={
          <>
            <Button size="sm" onClick={() => navigate('/help/sensors')}>
              Show me how
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void update({ dismissedTips: [...dismissed, 'tip-cpu-temp'] })}>
              Not now
            </Button>
          </>
        }
      >
        Windows hides CPU temperature, fan speeds and voltages from apps. If you run the free tool HWiNFO or LibreHardwareMonitor, FPSFramer picks them up automatically.
      </Banner>,
    );
  }
  // A game was recognised that has no profile of its own yet.
  const exe = cap?.state === 'tracking' ? cap.exe : null;
  if (exe && live && !dismissed.includes(`tip-profile-${exe}`)) {
    const prof = profiles.find((p) => p.kind === 'game' && p.executables.includes(exe));
    if (!prof && live.profile.id === 'default') {
      hints.push(
        <Banner
          key="newgame"
          tone="info"
          title={`New game detected: ${cap?.name}`}
          actions={
            <>
              <Button size="sm" variant="primary" onClick={() => void createProfileFor(profiles, exe, cap?.name ?? exe)}>
                <Plus size={14} /> Create a profile
              </Button>
              <Button size="sm" onClick={() => void update({ capture: { ignored: [...new Set([...settings.capture.ignored, exe.toLowerCase()])] } })} title="Stop treating this program as a game (change it any time in Settings → Monitoring)">
                It’s not a game
              </Button>
              <Button size="sm" variant="ghost" onClick={() => void update({ dismissedTips: [...dismissed, `tip-profile-${exe}`] })}>
                No thanks
              </Button>
            </>
          }
        >
          A profile remembers this game’s overlay, FPS target and alerts. Until then it uses “All games”.
        </Banner>,
      );
    }
  }
  if (status && status.providers.some((p) => p.health !== 'ok')) {
    hints.push(
      <Banner key="providers" tone="warn" title="Some sensor sources had a problem" actions={<Button size="sm" onClick={() => navigate('/help/diagnostics')}>See details</Button>}>
        FPSFramer keeps retrying automatically. Other readings are not affected.
      </Banner>,
    );
  }
  return hints;
}

export async function createProfileFor(profiles: Profile[], exe: string, name: string) {
  const def = profiles.find((p) => p.kind === 'default');
  if (!def) return;
  const p = clone(def);
  p.id = '';
  p.kind = 'game';
  p.name = name;
  p.executables = [exe];
  const ids = new Map<string, string>();
  p.scenes.forEach((s) => {
    const nid = newId('sc');
    ids.set(s.id, nid);
    s.id = nid;
    s.overlays.forEach((o) => (o.id = newId('pl')));
  });
  p.activeSceneId = ids.get(def.activeSceneId) ?? p.scenes[0]?.id ?? '';
  const saved = await act<Profile>('profiles.save', { profile: p }, `Profile created for ${name}`);
  if (saved) navigate('/profiles');
}

function OverlayCard() {
  const { settings, profiles, live } = useApp();
  const update = useApp((s) => s.updateSettings);
  const profile = currentProfile(profiles, live);
  const hk = settings.hotkeys;
  return (
    <Card>
      <CardHead
        title="Overlay"
        sub="Numbers shown on top of your game"
        icon={<PictureInPicture2 size={18} color="var(--accent-text)" />}
        right={<Switch label="Show the overlay in games" checked={settings.overlayEnabled} onChange={(v) => void update({ overlayEnabled: v })} />}
      />
      <div className="stack">
        <div>
          <div className="faint" style={{ fontSize: '0.8125rem', marginBottom: 6 }}>
            Style for {profile?.name ?? 'all games'}
          </div>
          <div className="row wrap" role="group" aria-label="Overlay style">
            {profile?.scenes.map((s) => (
              <Chip key={s.id} on={s.id === profile.activeSceneId} onClick={() => void act('profiles.setScene', { profileId: profile.id, sceneId: s.id })}>
                {s.name}
              </Chip>
            ))}
          </div>
        </div>
        <div className="row wrap">
          <Button onClick={() => void act('overlay.preview', { seconds: 10 })} title="Shows the overlay on your desktop for 10 seconds">
            <Eye size={16} /> Preview on desktop
          </Button>
          <Button variant="primary" onClick={() => navigate('/overlay')}>
            <Settings2 size={16} /> Customize
          </Button>
        </div>
        <p className="muted" style={{ fontSize: '0.8125rem' }}>
          {hk.toggleOverlay && (
            <>
              Show/hide: <Kbd>{hk.toggleOverlay}</Kbd>{' '}
            </>
          )}
          {hk.nextScene && (
            <>
              · Next style: <Kbd>{hk.nextScene}</Kbd>
            </>
          )}
        </p>
      </div>
    </Card>
  );
}
