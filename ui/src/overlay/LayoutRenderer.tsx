// The single renderer for overlay layouts. The in-game overlay window, the editor canvas,
// the layout gallery and the preset thumbnails all use this component, so what you see in the
// editor is exactly what appears over the game.

import { AlertTriangle, Info, OctagonAlert } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Gauge, Sparkline } from '../components/Charts';
import { formatClock, formatReading, HIDE_IN_OVERLAY, percentOfRange, statusOf } from '../lib/format';
import { useFrames, useSeries } from '../lib/series';
import type { ActiveAlert, Layout, LayoutCanvas, LayoutEl, MetricValue, Readings, SensorDesc, Settings, Status } from '../lib/types';
import { fontStack, textShadowCss, withAlpha } from './fonts';
import { SAMPLE_ALERT, sampleWave } from './sample';

export interface RenderCtx {
  catalog: Record<string, SensorDesc>;
  values: Readings;
  stats: Record<string, [number, number, number]>;
  settings: Pick<Settings, 'units'>;
  alerts: ActiveAlert[];
  targetFps: number | null;
  /** `sample`: values are made up for previewing; graphs draw synthetic curves. */
  sample: boolean;
  /** Editor: show elements even when their sensor is unavailable, and mark them. */
  editor?: boolean;
}

interface Props {
  layout: Layout;
  ctx: RenderCtx;
  scale?: number;
  opacity?: number;
  selectedIds?: string[];
  /** Editor interaction: pointer-down on an element. */
  onElementPointerDown?: (id: string, e: React.PointerEvent) => void;
  className?: string;
  rootRef?: React.Ref<HTMLDivElement>;
}

export function LayoutRenderer({ layout, ctx, scale = 1, opacity = 1, selectedIds = [], onElementPointerDown, className = '', rootRef }: Props) {
  const c = layout.canvas;
  const free = c.arrangement === 'free';
  const bg = c.background;
  const bw = bg.borderWidth ?? 0;
  const style: CSSProperties = {
    fontFamily: fontStack(c.font.family),
    fontSize: c.font.size,
    fontWeight: c.font.weight,
    color: c.textColor,
    background: withAlpha(bg.color, bg.opacity),
    borderRadius: bg.radius,
    border: bw ? `${bw}px solid ${withAlpha(bg.borderColor, bg.borderOpacity ?? 0.1)}` : undefined,
    padding: free ? 0 : c.padding,
    gap: c.gap,
    width: c.width,
    height: free ? c.height : undefined,
    minWidth: free ? undefined : c.width,
    textShadow: textShadowCss(c.textShadow),
    opacity,
    zoom: scale !== 1 ? scale : undefined,
    position: 'relative',
    boxSizing: 'border-box',
    lineHeight: 1.25,
    fontVariantNumeric: 'tabular-nums',
    overflow: free ? 'hidden' : undefined,
  };
  if (!free) {
    if (c.direction === 'row') Object.assign(style, { display: 'flex', flexDirection: 'row', alignItems: 'center', flexWrap: 'nowrap' });
    else if (c.columns > 1) Object.assign(style, { display: 'grid', gridTemplateColumns: `repeat(${c.columns}, minmax(0, 1fr))`, alignItems: 'center' });
    else Object.assign(style, { display: 'flex', flexDirection: 'column', alignItems: 'stretch' });
    style.width = c.width ?? 'max-content';
  }
  return (
    <div ref={rootRef} className={`ov-root ${className}`} style={style} data-layout={layout.id}>
      {layout.elements.map((el) => {
        if (el.visible === false || !isShown(el, c, ctx)) return null;
        return (
          <ElementBox key={el.id} el={el} canvas={c} free={free} selected={selectedIds.includes(el.id)} onPointerDown={onElementPointerDown}>
            <ElementView el={el} canvas={c} ctx={ctx} free={free} />
          </ElementBox>
        );
      })}
    </div>
  );
}

/** Does this element draw anything right now? (Decided before wrapping so hidden rows leave no gap.) */
export function isShown(el: LayoutEl, canvas: LayoutCanvas, ctx: RenderCtx): boolean {
  switch (el.type) {
    case 'metric':
      return (el.values ?? []).some((v) => !resolve(v, ctx, canvas, el).hidden);
    case 'cores':
      return coreList(el, ctx).length > 0;
    case 'graph':
      return !graphHidden(el, canvas, ctx);
    case 'alerts':
      return alertList(ctx).length > 0;
    default:
      return true;
  }
}

function coreList(el: LayoutEl, ctx: RenderCtx): { id: string; n: number }[] {
  const metric = el.metric ?? 'load';
  const ids = Object.keys(ctx.catalog)
    .map((id) => /^cpu\.core\.(\d+)\.(load|clock|temp)$/.exec(id))
    .filter((m): m is RegExpExecArray => !!m && m[2] === metric)
    .map((m) => ({ id: m[0], n: Number(m[1]) }))
    .sort((a, b) => a.n - b.n);
  if (ids.length) return ids;
  return ctx.editor || ctx.sample ? Array.from({ length: 8 }, (_, n) => ({ id: `cpu.core.${n}.${metric}`, n })) : [];
}

function graphHidden(el: LayoutEl, canvas: LayoutCanvas, ctx: RenderCtx): boolean {
  const ids = (el.series ?? []).map((s) => s.sensorId);
  const allGone =
    ids.length > 0 &&
    ids.every((id) => {
      const d = ctx.catalog[id];
      return d && !d.availability.available && HIDE_IN_OVERLAY.has(d.availability.reason?.code ?? '');
    });
  return allGone && !ctx.sample && !ctx.editor && canvas.unavailable === 'hide' && !el.showWhenUnavailable;
}

function alertList(ctx: RenderCtx): ActiveAlert[] {
  const live = ctx.alerts.filter((a) => a.overlay);
  return live.length ? live : ctx.editor ? [SAMPLE_ALERT] : [];
}

function ElementBox({ el, canvas, free, selected, onPointerDown, children }: { el: LayoutEl; canvas: LayoutCanvas; free: boolean; selected: boolean; onPointerDown?: Props['onElementPointerDown']; children: ReactNode }) {
  const s = el.style ?? {};
  const style: CSSProperties = {
    fontFamily: s.fontFamily ? fontStack(s.fontFamily) : undefined,
    fontSize: s.fontSize,
    fontWeight: s.fontWeight,
    color: s.color,
    textAlign: s.align,
    padding: s.padding,
    borderRadius: s.radius,
    textShadow: s.textShadow ? textShadowCss(s.textShadow) : undefined,
    background: s.background?.color ? withAlpha(s.background.color, s.background.opacity ?? 1) : undefined,
    opacity: el.opacity,
    minWidth: 0,
  };
  if (free) {
    Object.assign(style, { position: 'absolute', left: el.x ?? 0, top: el.y ?? 0, width: el.w, height: el.h });
  } else {
    if (el.span && el.span > 1) style.gridColumn = `span ${Math.min(el.span, canvas.columns || 1)}`;
    // Graphs size themselves (plot height + title row); everything else honours explicit sizes.
    if (el.w && el.type !== 'graph') style.width = el.w;
    if (el.h && el.type !== 'graph') style.height = el.h;
  }
  return (
    <div className="ov-el" data-el={el.id} data-type={el.type} data-selected={selected || undefined} style={style} onPointerDown={onPointerDown ? (e) => onPointerDown(el.id, e) : undefined}>
      {children}
    </div>
  );
}

function ElementView({ el, canvas, ctx, free }: { el: LayoutEl; canvas: LayoutCanvas; ctx: RenderCtx; free: boolean }): ReactNode {
  switch (el.type) {
    case 'metric':
      return <MetricView el={el} canvas={canvas} ctx={ctx} />;
    case 'cores':
      return <CoresView el={el} canvas={canvas} ctx={ctx} />;
    case 'text':
      return <div style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{el.text ?? ''}</div>;
    case 'clock':
      return <ClockView el={el} ctx={ctx} />;
    case 'image':
      return el.src ? (
        <img src={el.src} alt="" draggable={false} style={{ display: 'block', width: '100%', height: '100%', objectFit: el.fit ?? 'contain', pointerEvents: 'none' }} />
      ) : (
        <div style={{ width: el.w ?? 48, height: el.h ?? 48, border: '1px dashed currentColor', opacity: 0.4, display: 'grid', placeItems: 'center', fontSize: 11 }}>image</div>
      );
    case 'shape':
      return <ShapeView el={el} />;
    case 'graph':
      return <GraphView el={el} canvas={canvas} ctx={ctx} free={free} />;
    case 'alerts':
      return <AlertsView el={el} ctx={ctx} />;
    case 'divider':
      return <div style={{ height: 1, width: '100%', background: withAlpha(canvas.labelColor, 0.35), margin: `${(el.size ?? 2) / 2}px 0` }} />;
    case 'spacer':
      return <div style={{ height: el.size ?? 8, width: '100%' }} />;
    default:
      return null;
  }
}

// ───────────────────────────── metrics ─────────────────────────────

function valueColor(canvas: LayoutCanvas, el: LayoutEl, v: MetricValue, status: Status): string | undefined {
  const mode = v.colorMode ?? 'status';
  if (mode === 'fixed' && v.color) return v.color;
  if (mode === 'inherit') return undefined;
  if (status === 'critical') return canvas.colors.critical;
  if (status === 'elevated') return canvas.colors.elevated;
  return canvas.colors.normal ?? el.style?.color;
}

interface ResolvedValue {
  v: MetricValue;
  desc: SensorDesc | undefined;
  raw: number | string | undefined;
  numeric: number | undefined;
  hidden: boolean;
  na: boolean;
}

function resolve(v: MetricValue, ctx: RenderCtx, canvas: LayoutCanvas, el: LayoutEl): ResolvedValue {
  const desc = ctx.catalog[v.sensorId];
  const stat = v.stat && v.stat !== 'live' ? ctx.stats[v.sensorId] : undefined;
  let raw: number | string | undefined = ctx.values[v.sensorId];
  if (stat) raw = v.stat === 'min' ? stat[0] : v.stat === 'max' ? stat[1] : stat[2];
  const unavailable = !!desc && !desc.availability.available;
  const permanently = unavailable && HIDE_IN_OVERLAY.has(desc!.availability.reason?.code ?? '');
  const hidden = !ctx.sample && !ctx.editor && canvas.unavailable === 'hide' && !el.showWhenUnavailable && (permanently || (!desc && raw === undefined && false));
  return { v, desc, raw, numeric: typeof raw === 'number' ? raw : undefined, hidden, na: raw === undefined && unavailable };
}

function MetricView({ el, canvas, ctx }: { el: LayoutEl; canvas: LayoutCanvas; ctx: RenderCtx }): ReactNode {
  const values = (el.values ?? []).map((v) => resolve(v, ctx, canvas, el));
  const shown = values.filter((r) => !r.hidden);
  if (shown.length === 0) return null;
  const first = shown[0];
  const lab = el.label?.mode === 'none' ? null : el.label?.mode === 'text' ? el.label.text : (first.desc?.short ?? first.v.sensorId);
  const labelStyle: CSSProperties = { color: el.style?.labelColor ?? canvas.labelColor, fontWeight: 500, fontSize: '0.85em', whiteSpace: 'nowrap' };
  const display = el.display ?? 'text';
  const stacked = el.arrange === 'stacked';
  const sizeFont = el.style?.fontSize;

  const cells = shown.map((r, i) => <ValueCell key={`${r.v.sensorId}-${i}`} r={r} el={el} canvas={canvas} ctx={ctx} />);

  if (display === 'gauge' && first.numeric !== undefined) {
    const pct = percentOfRange(first.desc, first.numeric);
    const st = statusOf(first.desc, first.numeric, ctx.targetFps);
    const col = valueColor(canvas, el, first.v, st) ?? canvas.accent;
    const size = Math.max(40, (sizeFont ?? canvas.font.size) * 3.6);
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 2 }}>
        <Gauge percent={pct} size={size} color={col} track={withAlpha(canvas.labelColor, 0.3)} stroke={Math.max(4, size / 9)}>
          <span style={{ fontWeight: 700, fontSize: '0.95em' }}>{cells[0]}</span>
        </Gauge>
        {lab && <span style={labelStyle}>{lab}</span>}
      </div>
    );
  }
  if (display === 'bar' && first.numeric !== undefined) {
    const pct = percentOfRange(first.desc, first.numeric);
    const st = statusOf(first.desc, first.numeric, ctx.targetFps);
    const col = valueColor(canvas, el, first.v, st) ?? canvas.accent;
    return (
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
        {lab && <span style={{ ...labelStyle, minWidth: '3.2em' }}>{lab}</span>}
        <div style={{ flex: 1, minWidth: el.barWidth ?? 60, height: Math.max(5, (sizeFont ?? canvas.font.size) * 0.5), borderRadius: 99, background: withAlpha(canvas.labelColor, 0.28), overflow: 'hidden' }}>
          <div style={{ width: `${pct}%`, height: '100%', background: col, borderRadius: 99 }} />
        </div>
        <span style={{ display: 'inline-flex', gap: 8, minWidth: '2.6em', justifyContent: 'flex-end' }}>{cells}</span>
      </div>
    );
  }
  if (display === 'spark') {
    return (
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
        {lab && <span style={labelStyle}>{lab}</span>}
        <div style={{ flex: 1, minWidth: el.barWidth ?? 70 }}>
          <SparkFor id={first.v.sensorId} sample={ctx.sample} color={canvas.accent} desc={first.desc} />
        </div>
        <span style={{ display: 'inline-flex', gap: 8 }}>{cells}</span>
      </div>
    );
  }
  if (stacked) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', lineHeight: 1.05 }}>
        <span style={{ display: 'inline-flex', gap: 8, alignItems: 'baseline' }}>{cells}</span>
        {lab && <span style={{ ...labelStyle, fontSize: '0.35em', letterSpacing: '0.12em', textTransform: 'uppercase', marginTop: 2 }}>{lab}</span>}
      </div>
    );
  }
  return (
    <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, justifyContent: el.style?.align === 'center' ? 'center' : el.style?.align === 'right' ? 'flex-end' : 'flex-start', whiteSpace: 'nowrap' }}>
      {lab && <span style={{ ...labelStyle, flex: 'none', minWidth: shown.length > 1 || (display as string) === 'text' ? undefined : undefined }}>{lab}</span>}
      <span style={{ display: 'inline-flex', gap: 8, alignItems: 'baseline', marginLeft: 'auto' }}>{cells}</span>
    </div>
  );
}

function ValueCell({ r, el, canvas, ctx }: { r: ResolvedValue; el: LayoutEl; canvas: LayoutCanvas; ctx: RenderCtx }) {
  const { v, desc } = r;
  const fmt = r.raw !== undefined ? formatReading(desc, r.raw, ctx.settings, { decimals: v.decimals, hideUnit: v.hideUnit }) : null;
  if (!fmt) {
    const reason = desc?.availability.reason?.message;
    return (
      <span title={reason} style={{ opacity: 0.55, fontWeight: 600 }}>
        {r.na ? 'N/A' : '...'}
      </span>
    );
  }
  const st = statusOf(desc, r.numeric, ctx.targetFps);
  const color = valueColor(canvas, el, v, st);
  const showUnit = fmt.unit && !(desc?.unit === 'fps');
  return (
    <span style={{ color, fontWeight: 700 }}>
      {v.prefix}
      {fmt.text}
      {showUnit && <span style={{ fontSize: '0.72em', fontWeight: 600, marginLeft: 1, opacity: 0.85 }}>{fmt.unit}</span>}
      {v.suffix}
    </span>
  );
}

function SparkFor({ id, sample, color, desc }: { id: string; sample: boolean; color: string; desc?: SensorDesc }) {
  const s = useSeries([id], 60)[id];
  const data = sample || s.v.length < 2 ? sampleWave(40, 60, 30) : s.v;
  return <Sparkline values={data} height={20} color={color} min={desc?.min ?? undefined} max={desc?.max ?? undefined} />;
}

// ───────────────────────────── CPU cores ─────────────────────────────

function CoresView({ el, canvas, ctx }: { el: LayoutEl; canvas: LayoutCanvas; ctx: RenderCtx }): ReactNode {
  const list = coreList(el, ctx);
  if (!list.length) return null;
  const cols = el.columns ?? 8;
  const h = el.barHeight ?? 16;
  return (
    <div style={{ display: 'grid', gridTemplateColumns: `repeat(${Math.min(cols, list.length)}, 1fr)`, gap: 3, width: '100%' }}>
      {list.map(({ id, n }) => {
        const desc = ctx.catalog[id];
        const raw = ctx.values[id];
        const v = typeof raw === 'number' ? raw : ctx.sample ? 20 + ((n * 37) % 70) : undefined;
        const pct = v === undefined ? 0 : percentOfRange(desc ?? { unit: 'percent', min: 0, max: 100 } as SensorDesc, v);
        const st = statusOf(desc, v);
        const col = st === 'critical' ? canvas.colors.critical : st === 'elevated' ? canvas.colors.elevated : canvas.accent;
        return (
          <div key={id} title={`Core ${n + 1}: ${v === undefined ? 'no data' : Math.round(v) + '%'}`} style={{ position: 'relative', height: h, borderRadius: 3, background: withAlpha(canvas.labelColor, 0.25), overflow: 'hidden' }}>
            <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, height: `${pct}%`, background: col }} />
            {el.showNumbers !== false && h >= 12 && <span style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center' }}>
                {/* A dark chip keeps the number readable over the bar fill, the empty track and whatever is behind. */}
                <span style={{ fontSize: Math.min(11, h * 0.6), fontWeight: 700, lineHeight: 1, padding: '1px 3px', borderRadius: 3, background: 'rgba(0,0,0,0.75)', color: '#fff' }}>{v === undefined ? '...' : Math.round(v)}</span>
              </span>}
          </div>
        );
      })}
    </div>
  );
}

// ───────────────────────────── clock ─────────────────────────────

function useNow(ms: number): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

function ClockView({ el, ctx }: { el: LayoutEl; ctx: RenderCtx }) {
  const now = useNow(1000);
  const mode = el.mode ?? 'time';
  if (mode === 'session') {
    const v = ctx.values['session.time'];
    return <span style={{ fontWeight: 700 }}>{typeof v === 'number' ? formatClock(v) : '...'}</span>;
  }
  const time = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: el.seconds ? '2-digit' : undefined, hour12: el.hour12 ?? false });
  const date = now.toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' });
  return <span style={{ fontWeight: 700 }}>{mode === 'time' ? time : mode === 'date' ? date : `${date}  ${time}`}</span>;
}

// ───────────────────────────── shape ─────────────────────────────

function ShapeView({ el }: { el: LayoutEl }) {
  const fillColor = typeof el.fill === 'string' ? el.fill : undefined;
  const fill = fillColor ? withAlpha(fillColor, el.fillOpacity ?? 1) : 'transparent';
  const sw = el.strokeWidth ?? 0;
  if (el.shape === 'line') {
    return <div style={{ width: '100%', height: Math.max(1, sw || 2), background: el.stroke ?? fillColor ?? '#fff', position: 'absolute', top: '50%', transform: 'translateY(-50%)' }} />;
  }
  return <div style={{ width: '100%', height: '100%', background: fill, border: sw ? `${sw}px solid ${el.stroke ?? '#fff'}` : undefined, borderRadius: el.shape === 'ellipse' ? '50%' : (el.radius ?? 0), boxSizing: 'border-box' }} />;
}

// ───────────────────────────── graph ─────────────────────────────

function GraphView({ el, canvas, ctx, free }: { el: LayoutEl; canvas: LayoutCanvas; ctx: RenderCtx; free: boolean }): ReactNode {
  const ref = useRef<HTMLCanvasElement>(null);
  const showHead0 = el.showLabel !== false && (el.title || el.showValue);
  const w = el.w ?? 160;
  // In free layouts the element's box is the whole graph; in lists it is the plot height.
  const h = Math.max(8, (el.h ?? 40) - (free && showHead0 ? 14 : 0));
  const source = el.source ?? 'history';
  const seriesDef = el.series ?? [];
  const ids = seriesDef.map((s) => s.sensorId);
  const nFrames = el.frames ?? 240;
  const hist = useSeries(source === 'history' ? ids : [], el.seconds as number ?? 60);
  const frames = useFrames(nFrames);

  const lines: { data: (number | null)[]; color: string }[] = useMemo(() => {
    if (source === 'frames') {
      const data = ctx.sample || frames.length < 4 ? sampleWave(nFrames > 200 ? 200 : nFrames, 7, 1.2, 3).map((v, i) => (i % 53 === 0 ? v + 9 : v)) : frames;
      return [{ data, color: seriesDef[0]?.color ?? canvas.accent }];
    }
    return seriesDef.map((s, i) => {
      const live = hist[s.sensorId]?.v ?? [];
      const data = ctx.sample || live.filter((x) => x !== null).length < 2 ? sampleWave(60, s.sensorId.includes('temp') ? 65 : 55, s.sensorId.includes('temp') ? 6 : 28, i + 1) : live;
      return { data, color: s.color ?? canvas.accent };
    });
  }, [source, frames, hist, seriesDef, ctx.sample, nFrames, canvas.accent]);

  const last = lines[0]?.data.filter((x): x is number => x !== null).slice(-1)[0];
  const firstId = seriesDef[0]?.sensorId;
  const desc = firstId ? ctx.catalog[firstId] : undefined;
  const liveRaw = firstId ? ctx.values[firstId] : undefined;

  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const g = c.getContext('2d');
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const all = lines.flatMap((l) => l.data).filter((x): x is number => x !== null);
    if (all.length < 2) return;
    let lo = el.min ?? Math.min(...all);
    let hi = el.max;
    if (hi === undefined) {
      if (source === 'frames') {
        const sorted = [...all].sort((a, b) => a - b);
        hi = Math.min(100, Math.max(10, sorted[Math.floor(sorted.length * 0.99)] * 1.5));
        lo = el.min ?? 0;
      } else {
        hi = Math.max(...all);
        const pad = (hi - lo || 1) * 0.1;
        if (el.min === undefined) lo -= pad;
        hi += pad;
      }
    }
    if (hi - lo < 1e-6) hi = lo + 1;
    const py = (v: number) => h - 2 - ((Math.min(Math.max(v, lo), hi) - lo) / (hi - lo)) * (h - 4);
    if (el.grid) {
      g.strokeStyle = withAlpha(canvas.labelColor, 0.22);
      g.lineWidth = 1;
      for (let i = 1; i < 4; i++) {
        const y = Math.round((h * i) / 4) + 0.5;
        g.beginPath();
        g.moveTo(0, y);
        g.lineTo(w, y);
        g.stroke();
      }
    }
    if (el.showTarget && ctx.targetFps && source === 'frames') {
      const y = py(1000 / ctx.targetFps);
      g.strokeStyle = withAlpha(canvas.colors.elevated, 0.7);
      g.setLineDash([3, 3]);
      g.beginPath();
      g.moveTo(0, y);
      g.lineTo(w, y);
      g.stroke();
      g.setLineDash([]);
    }
    lines.forEach((l, li) => {
      const n = l.data.length;
      g.strokeStyle = l.color;
      g.lineWidth = el.thickness ?? 1.6;
      g.lineJoin = 'round';
      g.setLineDash(li === 1 ? [5, 3] : li === 2 ? [2, 3] : []);
      g.beginPath();
      let pen = false;
      l.data.forEach((v, i) => {
        if (v === null) {
          pen = false;
          return;
        }
        const x = (i / (n - 1)) * w;
        if (!pen) g.moveTo(x, py(v));
        else g.lineTo(x, py(v));
        pen = true;
      });
      g.stroke();
      g.setLineDash([]);
      if (el.fill && li === 0) {
        g.lineTo(w, h);
        g.lineTo(0, h);
        g.closePath();
        g.globalAlpha = 0.16;
        g.fillStyle = l.color;
        g.fill();
        g.globalAlpha = 1;
      }
    });
  }, [lines, w, h, el.min, el.max, el.grid, el.fill, el.thickness, el.showTarget, source, canvas, ctx.targetFps]);

  const showHead = el.showLabel !== false && (el.title || el.showValue);
  const live = typeof liveRaw === 'number' ? liveRaw : last;
  const f = live !== undefined ? formatReading(desc, live, ctx.settings, {}) : null;
  return (
    <div style={{ width: w, height: h + (showHead ? 14 : 0), position: 'relative' }}>
      {showHead && (
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.7em', lineHeight: '14px', color: canvas.labelColor, fontWeight: 600 }}>
          <span>{el.showLabel !== false ? el.title : ''}</span>
          {el.showValue && f && (
            <span style={{ color: canvas.textColor }}>
              {f.text}
              {f.unit && ` ${f.unit}`}
            </span>
          )}
        </div>
      )}
      <canvas ref={ref} style={{ width: w, height: h, display: 'block' }} />
    </div>
  );
}

// ───────────────────────────── alerts ─────────────────────────────

function AlertsView({ el, ctx }: { el: LayoutEl; ctx: RenderCtx }): ReactNode {
  const list = alertList(ctx);
  if (!list.length) return null;
  const max = el.maxVisible ?? 3;
  const extra = list.length - max;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 3, width: '100%' }}>
      {list.slice(0, max).map((a) => {
        const col = a.severity === 'critical' ? '#ff6b6b' : a.severity === 'warning' ? '#fbbf24' : '#7cb4ff';
        const Icon = a.severity === 'critical' ? OctagonAlert : a.severity === 'warning' ? AlertTriangle : Info;
        return (
          <div key={a.ruleId} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 8px', borderRadius: 8, background: withAlpha(col, 0.2), border: `1px solid ${withAlpha(col, 0.55)}`, color: '#fff', fontSize: '0.82em', fontWeight: 600, maxWidth: '100%' }}>
            <Icon size={13} color={col} aria-hidden style={{ flex: 'none' }} />
            <span style={{ overflowWrap: 'anywhere' }}>{a.message}</span>
          </div>
        );
      })}
      {extra > 0 && <div style={{ fontSize: '0.75em', opacity: 0.8 }}>+{extra} more</div>}
    </div>
  );
}
