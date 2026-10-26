import { ArrowLeft, ArrowRight, Check, CheckCircle2, Cpu, Eye, Keyboard, Lock, Monitor, PictureInPicture2, Power, ShieldCheck, Sparkles, Target, TriangleAlert } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { HotkeyField } from '../components/HotkeyField';
import { LayoutPreview } from '../components/shared';
import { Badge, Banner, Button, Kbd, Modal, Segmented, Setting, Switch } from '../components/ui';
import { rpc } from '../lib/bridge';
import { navigate } from '../lib/router';
import { act, errorMessage, useApp } from '../lib/store';
import type { AlertRule, Hotkeys, Profile } from '../lib/types';

type StepId = 'welcome' | 'pc' | 'gpu' | 'goals' | 'permission' | 'style' | 'overlay' | 'keys' | 'startup' | 'done';

const STEP_LABEL: Record<StepId, string> = {
  welcome: 'Welcome',
  pc: 'Your PC',
  gpu: 'Graphics card',
  goals: 'Your goals',
  permission: 'FPS permission',
  style: 'Overlay style',
  overlay: 'Overlay and detail',
  keys: 'Shortcuts',
  startup: 'Startup',
  done: 'All set',
};

const STYLE_BLURB: Record<string, string> = {
  'preset.minimal': 'Just FPS and temperatures. Easy on the eyes.',
  'preset.gaming': 'FPS, frame time and the main hardware numbers. A good everyday choice.',
  'preset.detailed': 'Everything useful at a glance, including every CPU core.',
  'preset.advanced': 'Graphs and extra sensors, for tinkering and testing.',
};

/** Only the two shortcuts most people want on day one. The rest live in Settings. */
const HOTKEY_LABELS: { key: keyof Hotkeys; label: string }[] = [
  { key: 'toggleOverlay', label: 'Show or hide the overlay' },
  { key: 'nextScene', label: 'Next overlay style' },
];

const MEASURES: [string, string][] = [
  ['fps', 'FPS'],
  ['cpu.load', 'CPU usage'],
  ['cpu.temp', 'CPU temperature'],
  ['gpu.load', 'GPU usage'],
  ['gpu.temp', 'GPU temperature'],
  ['gpu.fan', 'GPU fan'],
  ['gpu.power', 'GPU power'],
  ['ram.percent', 'Memory'],
  ['disk.read', 'Storage'],
  ['net.down', 'Network'],
];

function refreshRateOf(displays: string[] | undefined): number | null {
  let best: number | null = null;
  for (const d of displays ?? []) {
    const m = /@\s*(\d+)\s*Hz/i.exec(d);
    if (m) best = Math.max(best ?? 0, Number(m[1]));
  }
  return best && best >= 30 && best <= 1000 ? best : null;
}

export function Wizard() {
  const { settings, hardware, catalogById, layouts, profiles, status, platform, demo } = useApp();
  const [stepId, setStepId] = useState<StepId>('welcome');
  const [busy, setBusy] = useState(false);
  const [preset, setPreset] = useState('preset.gaming');
  const [mode, setMode] = useState<'simple' | 'advanced'>('simple');
  const [overlayOn, setOverlayOn] = useState(true);
  const [startup, setStartup] = useState(false);
  const [tray, setTray] = useState(settings.closeToTray);
  const [hotkeys, setHotkeys] = useState<Hotkeys>(settings.hotkeys);
  const [targetFromHz, setTargetFromHz] = useState(true);
  const [batteryAlert, setBatteryAlert] = useState(true);
  const [gpu, setGpu] = useState<number>(settings.primaryGpu ?? -1);

  const hz = refreshRateOf(hardware?.displays);
  const available = (id: string) => !!catalogById[id]?.availability.available;
  const hasBattery = available('battery.percent');
  const manyGpus = (hardware?.gpus.length ?? 0) > 1;
  const choices = layouts.filter((l) => ['preset.minimal', 'preset.gaming', 'preset.detailed', 'preset.advanced'].includes(l.id));
  const chosen = layouts.find((l) => l.id === preset);
  const cap = status?.capture;
  const windows = platform === 'windows';

  // Steps that have nothing to ask are left out. The current step is tracked by id, so a late hardware report
  // can add a step ahead of the user without moving them.
  const steps: StepId[] = ['welcome', 'pc'];
  if (manyGpus) steps.push('gpu');
  if (hz || hasBattery) steps.push('goals');
  steps.push('permission', 'style', 'overlay', 'keys', 'startup', 'done');
  const step = Math.max(0, steps.indexOf(stepId));
  const last = step === steps.length - 1;

  const finish = async () => {
    setBusy(true);
    try {
      // Save the target first: `def` is a copy from before this click, so saving it after the layout change
      // would put the old overlay layout back.
      const def = profiles.find((p) => p.kind === 'default');
      if (def && targetFromHz && hz && def.targetFps === null) await rpc<Profile>('profiles.save', { profile: { ...def, targetFps: hz } });
      await rpc('overlay.setLayout', { layoutId: preset });
      if (hasBattery && batteryAlert) {
        const tpl = (await rpc<AlertRule[]>('rules.templates')).find((t) => t.id === 'tpl.battery');
        if (tpl) await rpc('rules.save', { rule: { ...tpl, id: '' } });
      }
      await rpc('app.completeSetup', {
        settings: {
          uiMode: mode,
          overlayEnabled: overlayOn,
          launchAtStartup: startup && windows && !demo,
          closeToTray: tray,
          hotkeys,
          ...(gpu >= 0 ? { primaryGpu: gpu } : {}),
        },
      });
      useApp.setState({ firstRun: false });
      navigate('/');
    } catch (e) {
      useApp.getState().toast({ level: 'error', message: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  /** Closing the dialog early keeps the defaults but doesn't show the guide again. */
  const skip = async () => {
    await act('app.completeSetup', {});
    useApp.setState({ firstRun: false });
  };

  return (
    <Modal
      title="Welcome to FPSFramer"
      onClose={() => void skip()}
      className="wizard"
      footer={
        <>
          <span className="muted" style={{ marginRight: 'auto', fontSize: '0.875rem' }} aria-live="polite">
            Step {step + 1} of {steps.length}: {STEP_LABEL[stepId]}
          </span>
          {step === 0 && <Button variant="ghost" onClick={() => void skip()}>Skip setup</Button>}
          {step > 0 && (
            <Button onClick={() => setStepId(steps[step - 1])} disabled={busy}>
              <ArrowLeft size={15} /> Back
            </Button>
          )}
          {!last ? (
            <Button variant="primary" onClick={() => setStepId(steps[step + 1])} data-autofocus>
              {step === 0 ? 'Get started' : 'Next'} <ArrowRight size={15} />
            </Button>
          ) : (
            <Button variant="primary" onClick={() => void finish()} disabled={busy}>
              <Check size={15} /> Finish and open FPSFramer
            </Button>
          )}
        </>
      }
    >
      <ol className="wz-progress" aria-label="Setup progress">
        {steps.map((s, i) => (
          <li key={s} aria-current={i === step ? 'step' : undefined} className={i <= step ? 'on' : ''}>
            <span className="sr-only">{STEP_LABEL[s]}</span>
          </li>
        ))}
      </ol>

      {stepId === 'welcome' && (
        <StepShell icon={<Sparkles size={22} />} title="See how your games really run" lead="FPSFramer shows your frame rate and temperatures on screen while you play, and explains afterwards what happened. Setup takes about a minute.">
          {demo && <Badge tone="warn">Demo mode: values are simulated</Badge>}
          <Point icon={<Lock size={18} />} title="Private and safe">
            Everything stays on your PC. No account, no telemetry. FPSFramer never injects into a game.
          </Point>
          <Point icon={<CheckCircle2 size={18} />} title="Honest numbers">
            If something can’t be measured on your PC, FPSFramer says so and why. It never shows made-up values.
          </Point>
        </StepShell>
      )}

      {stepId === 'pc' && (
        <StepShell icon={<Cpu size={22} />} title="Your PC" lead="Here is what FPSFramer found.">
          <dl className="wz-facts">
            <Fact label="Processor" value={hardware?.cpu || 'Not detected'} />
            <Fact label="Graphics" value={hardware?.gpus.length ? hardware.gpus.join(', ') : 'No graphics card detected'} />
            <Fact label="Memory" value={hardware?.ramGb ? `${hardware.ramGb.toFixed(hardware.ramGb % 1 ? 1 : 0)} GB` : 'Not detected'} />
            <Fact label="Display" value={hardware?.displays.length ? hardware.displays.join(' · ') : 'Not detected'} />
          </dl>
          <Measurable catalogById={catalogById} />
        </StepShell>
      )}

      {stepId === 'gpu' && (
        <StepShell icon={<Monitor size={22} />} title="Which graphics card runs your games?" lead="Usually it is the faster, dedicated one. Automatic works for most PCs.">
          <div className="stack" style={{ gap: '0.5rem' }} role="radiogroup" aria-label="Main graphics card">
            {[{ v: -1, label: 'Choose automatically' }, ...(hardware?.gpus ?? []).map((g, i) => ({ v: i, label: g }))].map((o) => (
              <button key={o.v} type="button" role="radio" aria-checked={gpu === o.v} aria-pressed={gpu === o.v} className="choice" onClick={() => setGpu(o.v)}>
                <b className="grow">{o.label}</b>
                {gpu === o.v && <Check size={16} aria-hidden style={{ color: 'var(--accent-text)' }} />}
              </button>
            ))}
          </div>
        </StepShell>
      )}

      {stepId === 'goals' && (
        <StepShell icon={<Target size={22} />} title="Your goals" lead="Optional extras, based on what we found.">
          <div className="card tight">
            {hz && (
              <Setting title={`Your screen runs at ${hz} Hz`} desc={`Use ${hz} FPS as your goal, so FPSFramer can tell you how often you reach it.`}>
                <Switch checked={targetFromHz} onChange={setTargetFromHz} label={`Use ${hz} FPS as my target`} />
              </Setting>
            )}
            {hasBattery && (
              <Setting title="This looks like a laptop" desc="Warn me when the battery gets low (below 20%).">
                <Switch checked={batteryAlert} onChange={setBatteryAlert} label="Low battery alert" />
              </Setting>
            )}
          </div>
        </StepShell>
      )}

      {stepId === 'permission' && (
        <StepShell icon={<ShieldCheck size={22} />} title="Permission to measure FPS" lead="Windows only lets administrators measure frame rate. Temperatures, usage and memory work without it.">
          {!windows || demo ? (
            <Banner tone="info" title="Nothing to do here">
              {demo ? 'Demo mode simulates frame data.' : 'FPS measurement is only available on Windows.'}
            </Banner>
          ) : cap?.state === 'needsPermission' ? (
            <Banner
              tone="warn"
              title="Permission needed"
              actions={
                <Button
                  variant="primary"
                  onClick={async () => {
                    const r = await act<{ granted: boolean; note: string }>('capture.grantPermission');
                    if (r) useApp.getState().toast({ level: 'success', message: r.note });
                  }}
                >
                  Allow FPS measurement
                </Button>
              }
            >
              You’ll see one Windows prompt, then sign out and back in. You can skip this and do it later from Help.
            </Banner>
          ) : (
            <Banner tone="ok" title="FPS measurement is ready">
              {cap?.state === 'tracking' ? `Measuring ${cap.name ?? 'a game'} right now.` : 'Start a game and FPSFramer finds it automatically.'}
            </Banner>
          )}
        </StepShell>
      )}

      {stepId === 'style' && (
        <StepShell icon={<PictureInPicture2 size={22} />} title="Pick an overlay style" lead="This is what appears over your game. You can change it or design your own at any time.">
          <Segmented
            label="Overlay style"
            value={preset}
            options={choices.map((l) => ({ value: l.id, label: l.name }))}
            onChange={setPreset}
          />
          <div className="stack" style={{ gap: '0.5rem' }}>
            {chosen && <LayoutPreview layout={chosen} scale={0.9} minHeight={140} />}
            <p className="muted" style={{ margin: 0 }}>{STYLE_BLURB[preset] ?? chosen?.description}</p>
          </div>
        </StepShell>
      )}

      {stepId === 'overlay' && (
        <StepShell icon={<Eye size={22} />} title="Overlay and detail" lead="Two choices that shape how FPSFramer looks day to day.">
          <div className="card tight">
            <Setting title="Show the overlay while gaming" desc="You can turn it on or off any time with a hotkey.">
              <Switch checked={overlayOn} onChange={setOverlayOn} label="Show the overlay" />
            </Setting>
            <Setting title="Detail in the app" desc="Simple hides rarely used options. You can switch any time.">
              <Segmented
                label="Interface detail"
                value={mode}
                options={[
                  { value: 'simple', label: 'Simple' },
                  { value: 'advanced', label: 'Advanced' },
                ]}
                onChange={setMode}
              />
            </Setting>
          </div>
        </StepShell>
      )}

      {stepId === 'keys' && (
        <StepShell icon={<Keyboard size={22} />} title="Shortcuts" lead="They work even while a game is in front. Click one to change it. More shortcuts are in Settings.">
          <div className="card tight">
            {HOTKEY_LABELS.map((h) => (
              <Setting key={h.key} title={h.label}>
                <HotkeyField label={h.label} value={hotkeys[h.key]} onChange={(v) => setHotkeys({ ...hotkeys, [h.key]: v })} />
              </Setting>
            ))}
          </div>
        </StepShell>
      )}

      {stepId === 'startup' && (
        <StepShell icon={<Power size={22} />} title="Startup" lead="Choose how FPSFramer behaves in the background.">
          <div className="card tight">
            <Setting title="Start with Windows" desc="Ready in the tray before you launch a game.">
              <Switch checked={startup} onChange={setStartup} label="Start with Windows" disabled={!windows || demo} />
            </Setting>
            <Setting title="Keep running in the tray" desc="When you close the window, recording and the overlay carry on.">
              <Switch checked={tray} onChange={setTray} label="Keep running in the tray" />
            </Setting>
          </div>
        </StepShell>
      )}

      {stepId === 'done' && (
        <StepShell icon={<CheckCircle2 size={22} />} title="You’re all set" lead="Start a game. FPSFramer finds it and records it by itself. Afterwards, open Sessions to see how it ran.">
          <p className="muted" style={{ margin: 0 }}>
            Press <Kbd>{hotkeys.toggleOverlay ?? 'your hotkey'}</Kbd> to show or hide the overlay. If something looks wrong, Help checks your setup.
          </p>
          <Banner tone="info" title="About anti-cheat">
            No tool can promise every anti-cheat accepts an overlay. In competitive online games, check the game’s rules and switch the overlay off with your hotkey if unsure.
          </Banner>
        </StepShell>
      )}
    </Modal>
  );
}

function StepShell({ icon, title, lead, children }: { icon: ReactNode; title: string; lead: string; children: ReactNode }) {
  return (
    <div className="wz">
      <div className="wz-head">
        <span className="wz-icon" aria-hidden>
          {icon}
        </span>
        <h3 className="wz-title">{title}</h3>
        <p className="muted wz-lead">{lead}</p>
      </div>
      {children}
    </div>
  );
}

function Point({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <div className="card tight">
      <div className="row" style={{ gap: 8, fontWeight: 650 }}>
        <span style={{ color: 'var(--accent-text)', display: 'inline-flex' }} aria-hidden>
          {icon}
        </span>
        {title}
      </div>
      <div className="muted" style={{ marginTop: 4 }}>{children}</div>
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="wz-fact">
      <dt className="faint">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/** One line about what can be measured here. Only the readings that are missing are listed. */
function Measurable({ catalogById }: { catalogById: ReturnType<typeof useApp.getState>['catalogById'] }) {
  const rows = MEASURES.map(([id, label]) => {
    const d = catalogById[id];
    // FPS only becomes available once a game is running; count it as ready instead of missing.
    const ready = !!d?.availability.available || (id === 'fps' && !!d && d.availability.reason?.code === 'noActiveGame');
    return { id, label, ready };
  });
  const missing = rows.filter((r) => !r.ready);
  if (missing.length === 0) {
    return (
      <Banner tone="ok" title="Everything can be measured on this PC" />
    );
  }
  return (
    <div className="card tight stack" style={{ gap: '0.5rem' }}>
      <div style={{ fontWeight: 650 }}>
        {rows.length - missing.length} of {rows.length} readings are available
      </div>
      <div className="row wrap">
        {missing.map((r) => (
          <Badge key={r.id} tone="warn">
            <TriangleAlert size={12} aria-hidden /> {r.label}
          </Badge>
        ))}
      </div>
      <p className="faint" style={{ margin: 0, fontSize: '0.8125rem' }}>
        Help explains each missing reading and how to add it. CPU temperature, for example, needs the free HWiNFO or LibreHardwareMonitor.
      </p>
    </div>
  );
}
