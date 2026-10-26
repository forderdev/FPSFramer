import { Check, CircleSlash2, HelpCircle } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { formatReading, statusOf, unavailableText } from '../lib/format';
import { useSeries } from '../lib/series';
import { useApp } from '../lib/store';
import type { Category, Layout, SensorDesc } from '../lib/types';
import { LayoutRenderer, type RenderCtx } from '../overlay/LayoutRenderer';
import { sampleReadings } from '../overlay/sample';
import { Sparkline } from './Charts';
import { Badge, Button, Chip, Empty, Modal, SearchBox, StatusPill, Tip } from './ui';

// ───────────────────────────── render context ─────────────────────────────

/** Context for drawing layouts inside the app. Uses sample values when no game is running. */
export function usePreviewCtx(forceSample?: boolean, editor = false): RenderCtx {
  const { catalogById, live, settings } = useApp();
  const real = !!live && live.capture.state === 'tracking';
  const sample = forceSample ?? !real;
  return useMemo(
    () => ({
      catalog: catalogById,
      values: sample ? sampleReadings(catalogById) : (live?.values ?? {}),
      stats: sample ? {} : (live?.stats ?? {}),
      settings,
      alerts: sample ? [] : (live?.alerts ?? []),
      targetFps: live?.profile.targetFps ?? null,
      sample,
      editor,
    }),
    [catalogById, live, settings, sample, editor],
  );
}

export function LayoutPreview({ layout, scale = 1, minHeight, label = true }: { layout: Layout; scale?: number; minHeight?: number; label?: boolean }) {
  const ctx = usePreviewCtx();
  const box = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const [nat, setNat] = useState({ w: 0, h: 0 });
  const [avail, setAvail] = useState(0);
  useLayoutEffect(() => {
    const r = root.current;
    const b = box.current;
    if (!r || !b) return;
    const measure = () => {
      setNat({ w: r.offsetWidth, h: r.offsetHeight });
      setAvail(b.clientWidth - 24);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(r);
    ro.observe(b);
    return () => ro.disconnect();
  }, [layout]);
  // Shrink wide layouts so they always fit their card.
  const s = nat.w > 0 && avail > 0 ? Math.min(scale, avail / nat.w) : scale;
  return (
    <div ref={box} className="preview game-bg" style={{ minHeight, padding: 12, alignItems: 'start', justifyItems: 'start' }}>
      <div style={{ width: nat.w * s, height: nat.h * s, overflow: 'hidden' }}>
        <div style={{ transform: `scale(${s})`, transformOrigin: '0 0', width: nat.w || undefined }}>
          <LayoutRenderer layout={layout} ctx={ctx} rootRef={root} />
        </div>
      </div>
      {label && ctx.sample && <span className="tag">Sample values</span>}
    </div>
  );
}

// ───────────────────────────── values ─────────────────────────────

/** A formatted live reading with a clear "not available" state and the reason on hover. */
export function Reading({ id, decimals, hideUnit }: { id: string; decimals?: number; hideUnit?: boolean }) {
  const desc = useApp((s) => s.catalogById[id]);
  const settings = useApp((s) => s.settings);
  const raw = useApp((s) => s.live?.values[id]);
  const f = raw !== undefined ? formatReading(desc, raw, settings, { decimals, hideUnit }) : null;
  if (!f) {
    return (
      <Tip text={unavailableText(desc)}>
        <span className="faint" tabIndex={0}>
          {desc && !desc.availability.available ? 'N/A' : '...'}
        </span>
      </Tip>
    );
  }
  return (
    <span className="num">
      {f.text}
      {f.unit && <span className="faint" style={{ fontSize: '0.8em', marginLeft: 2 }}>{f.unit}</span>}
    </span>
  );
}

interface TileProps {
  title: string;
  icon: ReactNode;
  id: string;
  /** Extra readings shown under the main value. */
  extra?: { id: string; label: string; decimals?: number }[];
  hint?: string;
  onClick?: () => void;
  spark?: boolean;
}

export function MetricTile({ title, icon, id, extra = [], hint, onClick, spark = true }: TileProps) {
  const desc = useApp((s) => s.catalogById[id]);
  const settings = useApp((s) => s.settings);
  const live = useApp((s) => s.live);
  const raw = live?.values[id];
  const f = raw !== undefined ? formatReading(desc, raw, settings, {}) : null;
  const status = statusOf(desc, typeof raw === 'number' ? raw : undefined, live?.profile.targetFps);
  const s = useSeries(spark ? [id] : [], 60)[id];
  const unavailable = desc && !desc.availability.available;
  const Wrapper = onClick ? 'button' : 'div';
  return (
    <Wrapper className="tile" onClick={onClick} type={onClick ? 'button' : undefined} title={onClick ? 'Open live graphs' : undefined}>
      <div className="tile-head">
        {icon}
        <span>{title}</span>
        <span className="spacer" />
        {f && desc?.statusRule && <StatusPill status={status} />}
      </div>
      {f ? (
        <div className={`tile-value val-${status}`}>
          {f.text}
          <span className="tile-unit">{f.unit}</span>
        </div>
      ) : (
        <div className="tile-value na" title={unavailableText(desc)}>
          {unavailable ? 'Not available' : '...'}
        </div>
      )}
      <div className="tile-sub">
        {extra.map((e) => {
          const d = useApp.getState().catalogById[e.id];
          const v = live?.values[e.id];
          const ff = v !== undefined ? formatReading(d, v, settings, { decimals: e.decimals }) : null;
          return (
            <span key={e.id}>
              {e.label} <b>{ff ? `${ff.text}${ff.unit ? ' ' + ff.unit : ''}` : d && !d.availability.available ? 'N/A' : '...'}</b>
            </span>
          );
        })}
        {!f && unavailable && <span style={{ flexBasis: '100%' }}>{desc?.availability.reason?.message.slice(0, 96)}…</span>}
        {hint && f && <span>{hint}</span>}
      </div>
      {onClick && <span className="sr-only">Opens the live graphs.</span>}
      {spark && <Sparkline values={s.v} height={30} color={status === 'critical' ? 'var(--bad)' : status === 'elevated' ? 'var(--warn)' : 'var(--accent)'} min={desc?.unit === 'percent' ? 0 : undefined} max={desc?.unit === 'percent' ? 100 : undefined} />}
    </Wrapper>
  );
}

// ───────────────────────────── sensor picker ─────────────────────────────

const CATEGORY_LABEL: Record<string, string> = {
  all: 'All',
  fps: 'FPS',
  cpu: 'CPU',
  gpu: 'Graphics card',
  memory: 'Memory',
  storage: 'Storage',
  network: 'Network',
  heat: 'Temperatures',
  power: 'Power',
  fan: 'Fans',
  voltage: 'Voltages',
  system: 'System',
  other: 'Other',
};

function groupOf(c: Category): string {
  return c === 'thermal' ? 'heat' : c;
}

export function matchesSensor(d: SensorDesc, q: string): boolean {
  if (!q.trim()) return true;
  const hay = `${d.name} ${d.short} ${d.id} ${d.group ?? ''} ${d.description} ${d.keywords.join(' ')} ${d.source}`.toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((t) => hay.includes(t));
}

interface PickerProps {
  title?: string;
  multi?: boolean;
  /** Only offer numeric sensors. */
  numericOnly?: boolean;
  exclude?: string[];
  selected?: string[];
  onPick: (ids: string[]) => void;
  onClose: () => void;
}

export function SensorPicker({ title = 'Choose what to measure', multi = false, numericOnly = false, exclude = [], selected = [], onPick, onClose }: PickerProps) {
  const catalog = useApp((s) => s.catalog);
  const [q, setQ] = useState('');
  const [cat, setCat] = useState('all');
  const [sel, setSel] = useState<string[]>(selected);
  const list = useMemo(() => catalog.filter((d) => !exclude.includes(d.id) && (!numericOnly || d.unit !== 'text')), [catalog, exclude, numericOnly]);
  const cats = useMemo(() => ['all', ...Array.from(new Set(list.map((d) => groupOf(d.category))))], [list]);
  const matches = useMemo(() => list.filter((d) => (cat === 'all' || groupOf(d.category) === cat) && matchesSensor(d, q)), [list, cat, q]);
  const shown = matches.slice(0, 120);
  const toggle = (id: string) => {
    if (!multi) return onPick([id]);
    setSel((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
  };
  return (
    <Modal
      title={title}
      onClose={onClose}
      wide
      footer={
        multi ? (
          <>
            <span className="muted" style={{ marginRight: 'auto' }}>
              {sel.length} selected
            </span>
            <Button onClick={onClose}>Cancel</Button>
            <Button variant="primary" onClick={() => onPick(sel)} disabled={sel.length === 0}>
              Add {sel.length > 0 ? sel.length : ''}
            </Button>
          </>
        ) : undefined
      }
    >
      <div className="stack">
        <div className="row">
          <SearchBox value={q} onChange={setQ} placeholder="Try “temperature”, “fan” or “memory”" label="Search measurements" />
        </div>
        <div className="row wrap" role="group" aria-label="Filter by type">
          {cats.map((c) => (
            <Chip key={c} on={cat === c} onClick={() => setCat(c)}>
              {CATEGORY_LABEL[c] ?? c}
            </Chip>
          ))}
        </div>
        {shown.length === 0 ? (
          <Empty icon={<CircleSlash2 size={32} />} title="Nothing matches that search">
            Try a shorter word, or choose “All”.
          </Empty>
        ) : (
          <div role="listbox" aria-multiselectable={multi} aria-label="Measurements" style={{ maxHeight: '50vh', overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 4 }}>
            {shown.map((d) => {
              const on = sel.includes(d.id);
              const ok = d.availability.available;
              return (
                <button
                  key={d.id}
                  type="button"
                  role="option"
                  aria-selected={on}
                  onClick={() => toggle(d.id)}
                  className="choice"
                  aria-pressed={on}
                  style={{ padding: '0.6rem 0.8rem', opacity: ok ? 1 : 0.75 }}
                >
                  {multi && <span style={{ width: 18, flex: 'none', color: 'var(--accent-text)' }}>{on ? <Check size={18} aria-hidden /> : null}</span>}
                  <span style={{ flex: 1, minWidth: 0 }}>
                    <span style={{ fontWeight: 650 }}>{d.name}</span>
                    {d.group && <span className="faint"> · {d.group}</span>}
                    <span className="muted" style={{ display: 'block', fontSize: '0.8125rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {d.description || d.id}
                    </span>
                  </span>
                  {!ok && (
                    <Tip text={unavailableText(d)}>
                      <Badge tone="warn">
                        <HelpCircle size={12} aria-hidden /> Not available here
                      </Badge>
                    </Tip>
                  )}
                  {d.source && d.source !== 'builtin' && /hwinfo|libre|companion/i.test(d.source) && <Badge tone="info">{d.source}</Badge>}
                </button>
              );
            })}
            {matches.length > shown.length && <p className="faint" style={{ padding: '0.5rem' }}>Showing the first {shown.length} of {matches.length}. Type a few letters to narrow the list.</p>}
          </div>
        )}
      </div>
    </Modal>
  );
}
