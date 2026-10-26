import { BellRing, Check, Pencil, Plus, ShieldAlert, Trash2, Volume2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { SensorPicker } from '../components/shared';
import { Badge, Banner, Button, Card, CardHead, Chip, Empty, Field, Modal, NumberInput, PageHead, Segmented, Switch, TextInput, confirmAction } from '../components/ui';
import { rpc } from '../lib/bridge';
import { formatClock, relativeTime } from '../lib/format';
import { clone, newId } from '../lib/labels';
import { act, useApp } from '../lib/store';
import type { AlertLogItem, AlertRule, SensorDesc, Settings } from '../lib/types';
import { displayScale, fromDisplayValue, toDisplayValue } from '../lib/units';

const SEVERITY_LABEL = { info: 'Heads-up', warning: 'Warning', critical: 'Critical' } as const;
const SEVERITY_TONE = { info: 'info', warning: 'warn', critical: 'bad' } as const;

/** Rule thresholds are stored in the engine's base units; the UI shows the user's units. */
function toShown(d: SensorDesc | undefined, v: number, s: Pick<Settings, 'units'>): number {
  return d ? toDisplayValue(d.unit, v, s) : v;
}
function fromShown(d: SensorDesc | undefined, v: number, s: Pick<Settings, 'units'>): number {
  return d ? fromDisplayValue(d.unit, v, s) : v;
}

function round(v: number): number {
  return Math.round(v * 100) / 100;
}

export function describeRule(r: AlertRule, d: SensorDesc | undefined, s: Pick<Settings, 'units'>): string {
  const unit = d ? displayScale(d.unit, s, 0).label : '';
  const shown = round(toShown(d, r.threshold, s));
  const what = d?.name ?? r.sensorId;
  const cmp = r.comparator === 'above' ? 'goes above' : 'drops below';
  const hold = r.forSeconds > 0 ? ` for ${r.forSeconds} s` : ' (as soon as it happens)';
  return `When ${what} ${cmp} ${shown}${unit ? ' ' + unit : ''}${hold}`;
}

export default function AlertsPage() {
  const rules = useApp((s) => s.rules);
  const live = useApp((s) => s.live);
  const log = useApp((s) => s.alertLog);
  const catalog = useApp((s) => s.catalogById);
  const settings = useApp((s) => s.settings);
  const toast = useApp((s) => s.toast);
  const [editing, setEditing] = useState<AlertRule | null>(null);
  const [adding, setAdding] = useState(false);
  const [logFilter, setLogFilter] = useState<'all' | 'fired'>('fired');

  const active = live?.alerts ?? [];
  const shownLog = useMemo(() => log.filter((l) => logFilter === 'all' || l.kind === 'fired').slice(0, 60), [log, logFilter]);

  const toggle = (r: AlertRule, enabled: boolean) => void act('rules.save', { rule: { ...r, enabled } });

  const remove = async (r: AlertRule) => {
    const copy = clone(r);
    const res = await act('rules.delete', { id: r.id });
    if (res === undefined) return;
    toast({ level: 'info', message: `Deleted “${r.name}”` }, async () => {
      await act('rules.save', { rule: copy }, `Restored “${r.name}”`);
    });
  };

  return (
    <div className="page">
      <PageHead
        title="Alerts"
        sub="Get a nudge when something needs attention, for example when your graphics card gets too hot. An alert only fires when the problem lasts a few seconds, so brief spikes don’t nag you."
        actions={
          <Button variant="primary" onClick={() => setAdding(true)}>
            <Plus size={16} /> Add alert
          </Button>
        }
      />

      {active.length > 0 && (
        <Banner tone={active.some((a) => a.severity === 'critical') ? 'bad' : 'warn'} title={`${active.length} ${active.length === 1 ? 'alert is' : 'alerts are'} active right now`}>
          <ul style={{ margin: 0, paddingLeft: '1.1rem' }}>
            {active.map((a) => (
              <li key={a.ruleId}>
                <b>{a.name}</b>: {a.message} <span className="faint">(for {formatClock(a.sinceMs / 1000)})</span>
              </li>
            ))}
          </ul>
        </Banner>
      )}

      <Card style={{ padding: 0, overflow: 'hidden' }}>
        {rules.length === 0 ? (
          <Empty icon={<BellRing size={36} />} title="No alerts yet" action={<Button variant="primary" onClick={() => setAdding(true)}>Add your first alert</Button>}>
            Alerts watch a measurement, like temperature or FPS, and tell you when it crosses a limit you choose.
          </Empty>
        ) : (
          <ul className="list" style={{ listStyle: 'none', margin: 0, padding: 0 }}>
            {rules.map((r) => {
              const d = catalog[r.sensorId];
              const missing = d && !d.availability.available;
              return (
                <li key={r.id} className="setting" style={{ padding: '0.9rem 1.1rem' }}>
                  <div className="grow">
                    <div className="row wrap" style={{ gap: '0.5rem' }}>
                      <b>{r.name}</b>
                      <Badge tone={SEVERITY_TONE[r.severity]}>{SEVERITY_LABEL[r.severity]}</Badge>
                      {r.gameOnly && <Badge>Only while gaming</Badge>}
                      {r.profileIds.length > 0 && <Badge>{r.profileIds.length} profile{r.profileIds.length === 1 ? '' : 's'} only</Badge>}
                      {missing && (
                        <Badge tone="warn" >
                          Can’t be measured on this PC
                        </Badge>
                      )}
                    </div>
                    <div className="setting-desc">{describeRule(r, d, settings)}</div>
                    <div className="faint row" style={{ gap: '0.75rem', fontSize: '0.8125rem', marginTop: 2 }}>
                      {r.notify.overlay && <span>On overlay</span>}
                      {r.notify.toast && <span>Notification</span>}
                      {r.notify.sound && (
                        <span className="row" style={{ gap: 3 }}>
                          <Volume2 size={13} aria-hidden /> Sound
                        </span>
                      )}
                      {!r.notify.overlay && !r.notify.toast && !r.notify.sound && <span>Logged only</span>}
                      <span>Re-alerts at most every {r.cooldownSeconds >= 60 ? `${Math.round(r.cooldownSeconds / 60)} min` : `${r.cooldownSeconds} s`}</span>
                    </div>
                  </div>
                  <div className="ctrl">
                    <Button size="sm" icon variant="ghost" aria-label={`Edit ${r.name}`} title="Edit" onClick={() => setEditing(clone(r))}>
                      <Pencil size={15} />
                    </Button>
                    <Button size="sm" icon variant="ghost" aria-label={`Delete ${r.name}`} title="Delete" onClick={() => void remove(r)}>
                      <Trash2 size={15} />
                    </Button>
                    <Switch checked={r.enabled} onChange={(v) => toggle(r, v)} label={`${r.name} enabled`} />
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </Card>

      <Card>
        <CardHead
          title="Recent activity"
          sub="What fired lately. Cleared and held-back alerts are recorded too."
          right={
            <Segmented
              label="Show"
              value={logFilter}
              options={[
                { value: 'fired', label: 'Alerts' },
                { value: 'all', label: 'Everything' },
              ]}
              onChange={setLogFilter}
            />
          }
        />
        {shownLog.length === 0 ? (
          <p className="muted" style={{ margin: 0 }}>
            Nothing has fired since FPSFramer started.
          </p>
        ) : (
          <div className="stack" style={{ gap: 2 }}>
            {shownLog.map((l, i) => (
              <LogRow key={`${l.t}-${l.ruleId}-${i}`} item={l} />
            ))}
          </div>
        )}
      </Card>

      {adding && (
        <AddAlert
          onClose={() => setAdding(false)}
          onPick={(r) => {
            setAdding(false);
            setEditing(r);
          }}
        />
      )}
      {editing && <RuleEditor rule={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

function LogRow({ item }: { item: AlertLogItem }) {
  const label = item.kind === 'fired' ? 'Fired' : item.kind === 'cleared' ? 'Back to normal' : 'Held back';
  return (
    <div className="row" style={{ gap: '0.75rem', padding: '0.4rem 0', borderBottom: '1px solid var(--line)' }}>
      <span className="faint num" style={{ minWidth: '5.5rem', fontSize: '0.8125rem' }} title={new Date(item.t).toLocaleString()}>
        {relativeTime(item.t)}
      </span>
      <Badge tone={item.kind === 'fired' ? SEVERITY_TONE[item.severity] : undefined}>{label}</Badge>
      <span style={{ fontWeight: 600 }}>{item.name}</span>
      <span className="muted" style={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {item.message}
      </span>
    </div>
  );
}

// ───────────────────────────── add from template ─────────────────────────────

function AddAlert({ onClose, onPick }: { onClose: () => void; onPick: (r: AlertRule) => void }) {
  const catalog = useApp((s) => s.catalogById);
  const settings = useApp((s) => s.settings);
  const rules = useApp((s) => s.rules);
  const [templates, setTemplates] = useState<AlertRule[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    rpc<AlertRule[]>('rules.templates')
      .then(setTemplates)
      .catch(() => setFailed(true));
  }, []);

  const blank = (): AlertRule => ({
    id: newId('rule'),
    name: 'My alert',
    enabled: true,
    sensorId: '',
    comparator: 'above',
    threshold: 80,
    forSeconds: 5,
    cooldownSeconds: 120,
    hysteresis: 0,
    severity: 'warning',
    notify: { overlay: true, toast: true, sound: false },
    profileIds: [],
    gameOnly: false,
    message: '',
  });

  return (
    <Modal title="Add an alert" onClose={onClose} wide>
      <div className="stack">
        <p className="muted" style={{ margin: 0 }}>
          Start from a ready-made alert and adjust it, or build your own.
        </p>
        {failed && <Banner tone="bad">The ready-made alerts couldn’t be loaded. You can still build your own.</Banner>}
        <div className="stack" style={{ gap: 6 }}>
          {(templates ?? []).map((t) => {
            const d = catalog[t.sensorId];
            const have = rules.some((r) => r.sensorId === t.sensorId && r.comparator === t.comparator && Math.abs(r.threshold - t.threshold) < 1e-6);
            return (
              <button key={t.id} type="button" className="choice" style={{ padding: '0.7rem 0.9rem' }} onClick={() => onPick({ ...t, id: newId('rule') })}>
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ fontWeight: 650 }}>{t.name}</span>
                  <span className="muted" style={{ display: 'block', fontSize: '0.8125rem' }}>
                    {describeRule(t, d, settings)}
                  </span>
                </span>
                {d && !d.availability.available && <Badge tone="warn">Can’t be measured here</Badge>}
                {have && (
                  <Badge tone="ok">
                    <Check size={12} aria-hidden /> Already added
                  </Badge>
                )}
              </button>
            );
          })}
        </div>
        <div>
          <Button onClick={() => onPick(blank())}>
            <Plus size={15} /> Build a custom alert
          </Button>
        </div>
      </div>
    </Modal>
  );
}

// ───────────────────────────── editor ─────────────────────────────

function RuleEditor({ rule, onClose }: { rule: AlertRule; onClose: () => void }) {
  const catalog = useApp((s) => s.catalogById);
  const settings = useApp((s) => s.settings);
  const profiles = useApp((s) => s.profiles);
  const exists = useApp((s) => s.rules.some((r) => r.id === rule.id));
  const [r, setR] = useState<AlertRule>(rule);
  const [picking, setPicking] = useState(false);
  const d = catalog[r.sensorId];
  const unit = d ? displayScale(d.unit, settings, 0).label : '';
  const set = <K extends keyof AlertRule>(k: K, v: AlertRule[K]) => setR((x) => ({ ...x, [k]: v }));
  const gameProfiles = profiles.filter((p) => p.kind === 'game');

  const save = async () => {
    const out = { ...r, name: r.name.trim() || 'My alert' };
    const res = await act<AlertRule>('rules.save', { rule: out }, exists ? 'Alert saved' : 'Alert added');
    if (res) onClose();
  };

  const cancel = async () => {
    if (JSON.stringify(r) !== JSON.stringify(rule) && exists) {
      const ok = await confirmAction({ title: 'Discard your changes?', body: 'The alert has not been saved.', confirmLabel: 'Discard', danger: true });
      if (!ok) return;
    }
    onClose();
  };

  return (
    <Modal
      title={exists ? 'Edit alert' : 'New alert'}
      onClose={() => void cancel()}
      wide
      footer={
        <>
          <Button onClick={() => void cancel()}>Cancel</Button>
          <Button variant="primary" onClick={() => void save()} disabled={!r.sensorId}>
            {exists ? 'Save' : 'Add alert'}
          </Button>
        </>
      }
    >
      <div className="stack">
        <Field label="Name">
          <TextInput label="Alert name" value={r.name} onChange={(v) => set('name', v)} maxLength={60} />
        </Field>

        <Field label="What to watch" help={d ? d.description : 'Pick the measurement this alert should keep an eye on.'}>
          <div className="row wrap">
            <Button onClick={() => setPicking(true)}>{d ? d.name : 'Choose a measurement…'}</Button>
            {d && !d.availability.available && <Badge tone="warn">Not available on this PC right now</Badge>}
          </div>
        </Field>

        <div className="row wrap" style={{ gap: '1rem', alignItems: 'flex-end' }}>
          <Field label="Alert when it">
            <Segmented
              label="Direction"
              value={r.comparator}
              options={[
                { value: 'above', label: 'Goes above' },
                { value: 'below', label: 'Drops below' },
              ]}
              onChange={(v) => set('comparator', v)}
            />
          </Field>
          <Field label="This value">
            <NumberInput label="Threshold" value={round(toShown(d, r.threshold, settings))} onChange={(v) => set('threshold', fromShown(d, v, settings))} step={d?.unit === 'celsius' ? 1 : 0.5} suffix={unit} />
          </Field>
          <Field label="For at least" help="It must stay past the limit this long before the alert fires. 0 = react instantly.">
            <NumberInput label="Seconds it must last" value={r.forSeconds} min={0} max={3600} onChange={(v) => set('forSeconds', Math.round(v))} suffix="seconds" />
          </Field>
        </div>

        <div className="row wrap" style={{ gap: '1rem', alignItems: 'flex-end' }}>
          <Field label="Importance">
            <Segmented
              label="Importance"
              value={r.severity}
              options={[
                { value: 'info', label: 'Heads-up' },
                { value: 'warning', label: 'Warning' },
                { value: 'critical', label: 'Critical' },
              ]}
              onChange={(v) => set('severity', v)}
            />
          </Field>
          <Field label="Don’t repeat for" help="After it fires, wait this long before it can fire again.">
            <NumberInput label="Cooldown seconds" value={r.cooldownSeconds} min={0} max={86400} onChange={(v) => set('cooldownSeconds', Math.round(v))} suffix="seconds" />
          </Field>
        </div>

        <Field label="How to tell me">
          <div className="row wrap">
            <Chip on={r.notify.overlay} onClick={() => set('notify', { ...r.notify, overlay: !r.notify.overlay })} title="Show on the overlay while you play (needs an Alerts item in your overlay layout)">
              <ShieldAlert size={13} aria-hidden /> On the overlay
            </Chip>
            <Chip on={r.notify.toast} onClick={() => set('notify', { ...r.notify, toast: !r.notify.toast })} title="A Windows notification">
              <BellRing size={13} aria-hidden /> Notification
            </Chip>
            <Chip on={r.notify.sound} onClick={() => set('notify', { ...r.notify, sound: !r.notify.sound })} title="A short sound">
              <Volume2 size={13} aria-hidden /> Sound
            </Chip>
          </div>
        </Field>

        <Field label="Message (optional)" help="Leave empty for a standard message that includes the current value.">
          <TextInput label="Custom message" value={r.message} onChange={(v) => set('message', v)} maxLength={140} placeholder="e.g. Time to check the fans" />
        </Field>

        <div className="stack" style={{ gap: 0 }}>
          <div className="setting">
            <div className="grow">
              <div className="setting-title">Only while a game is running</div>
              <div className="setting-desc">Skip this alert when you are just using the desktop.</div>
            </div>
            <div className="ctrl">
              <Switch checked={r.gameOnly} onChange={(v) => set('gameOnly', v)} label="Only while a game is running" />
            </div>
          </div>
          {gameProfiles.length > 0 && (
            <div className="setting">
              <div className="grow">
                <div className="setting-title">Limit to certain games</div>
                <div className="setting-desc">{r.profileIds.length === 0 ? 'Applies to every game.' : 'Applies only to the games ticked below.'}</div>
                <div className="row wrap" style={{ marginTop: 8 }}>
                  {gameProfiles.map((p) => (
                    <Chip key={p.id} on={r.profileIds.includes(p.id)} onClick={() => set('profileIds', r.profileIds.includes(p.id) ? r.profileIds.filter((x) => x !== p.id) : [...r.profileIds, p.id])}>
                      {p.name}
                    </Chip>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {picking && (
        <SensorPicker
          title="Which measurement should this alert watch?"
          numericOnly
          onClose={() => setPicking(false)}
          onPick={([id]) => {
            setPicking(false);
            const nd = catalog[id];
            setR((x) => ({ ...x, sensorId: id, name: x.name === 'My alert' && nd ? `${nd.name} alert` : x.name }));
          }}
        />
      )}
    </Modal>
  );
}
