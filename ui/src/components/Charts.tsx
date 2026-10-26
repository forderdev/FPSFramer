import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Table2 } from 'lucide-react';
import { Button } from './ui';

// ───────────────────────────── helpers ─────────────────────────────

export function cssVar(name: string, fallback: string): string {
  if (typeof document === 'undefined') return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

export const SERIES_DASH: number[][] = [[], [7, 4], [2, 4], [10, 3, 2, 3]];

export function seriesColor(i: number): string {
  return cssVar(`--chart-${(i % 4) + 1}`, ['#3ddcff', '#ffb454', '#b79bff', '#7ee787'][i % 4]);
}

function niceStep(range: number, ticks: number): number {
  const raw = range / ticks;
  const mag = Math.pow(10, Math.floor(Math.log10(raw || 1)));
  const n = raw / mag;
  return (n < 1.5 ? 1 : n < 3 ? 2 : n < 7 ? 5 : 10) * mag;
}

// ───────────────────────────── sparkline ─────────────────────────────

export function Sparkline({ values, width = 120, height = 32, color = 'var(--accent)', min, max, fill = true }: { values: (number | null)[]; width?: number; height?: number; color?: string; min?: number; max?: number; fill?: boolean }) {
  const pts = values.filter((v): v is number => v !== null);
  if (pts.length < 2) return <svg width={width} height={height} aria-hidden />;
  const lo = min ?? Math.min(...pts);
  const hi = max ?? Math.max(...pts);
  const span = hi - lo || 1;
  const n = values.length;
  const x = (i: number) => (i / (n - 1)) * width;
  const y = (v: number) => height - 2 - ((v - lo) / span) * (height - 4);
  let d = '';
  let started = false;
  values.forEach((v, i) => {
    if (v === null) {
      started = false;
      return;
    }
    d += `${started ? 'L' : 'M'}${x(i).toFixed(1)} ${y(v).toFixed(1)}`;
    started = true;
  });
  return (
    <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden>
      {fill && <path d={`${d} L${width} ${height} L0 ${height} Z`} fill={color} opacity={0.12} />}
      <path d={d} fill="none" stroke={color} strokeWidth={1.8} strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

// ───────────────────────────── gauge ─────────────────────────────

export function Gauge({ percent, size = 56, color = 'var(--accent)', track = 'rgb(255 255 255 / 0.15)', stroke = 6, children }: { percent: number; size?: number; color?: string; track?: string; stroke?: number; children?: React.ReactNode }) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const arc = c * 0.75;
  const val = Math.max(0, Math.min(100, percent)) / 100;
  return (
    <div style={{ position: 'relative', width: size, height: size, display: 'inline-grid', placeItems: 'center' }}>
      <svg width={size} height={size} style={{ transform: 'rotate(135deg)' }} aria-hidden>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={track} strokeWidth={stroke} strokeDasharray={`${arc} ${c}`} strokeLinecap="round" />
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={stroke} strokeDasharray={`${arc * val} ${c}`} strokeLinecap="round" />
      </svg>
      <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center' }}>{children}</div>
    </div>
  );
}

// ───────────────────────────── line chart ─────────────────────────────

export interface ChartSeries {
  id: string;
  label: string;
  color: string;
  dash?: number[];
  data: (number | null)[];
  format: (v: number) => string;
}

export interface ChartMarker {
  x: number;
  label: string;
  color: string;
}

export interface RefLine {
  y: number;
  label: string;
  color?: string;
}

interface LineChartProps {
  series: ChartSeries[];
  xs: number[];
  xFormat: (x: number) => string;
  height?: number;
  yMin?: number;
  yMax?: number;
  yFormat?: (v: number) => string;
  refLines?: RefLine[];
  markers?: ChartMarker[];
  onBrush?: (x0: number, x1: number) => void;
  ariaLabel: string;
  fill?: boolean;
}

const PAD = { l: 52, r: 12, t: 10, b: 24 };

export function LineChart({ series, xs, xFormat, height = 220, yMin, yMax, yFormat, refLines = [], markers = [], onBrush, ariaLabel, fill = true }: LineChartProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  const [hover, setHover] = useState<number | null>(null);
  const [drag, setDrag] = useState<{ a: number; b: number } | null>(null);
  const [table, setTable] = useState(false);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(Math.max(200, el.clientWidth)));
    ro.observe(el);
    setW(Math.max(200, el.clientWidth));
    return () => ro.disconnect();
  }, []);

  const scale = useMemo(() => {
    let lo = Infinity;
    let hi = -Infinity;
    for (const s of series) for (const v of s.data) if (v !== null) (lo = Math.min(lo, v)), (hi = Math.max(hi, v));
    for (const r of refLines) (lo = Math.min(lo, r.y)), (hi = Math.max(hi, r.y));
    if (!Number.isFinite(lo)) (lo = 0), (hi = 1);
    let a = yMin ?? lo;
    let b = yMax ?? hi + (hi - lo || 1) * 0.08;
    if (yMin === undefined && lo >= 0 && lo < (hi - lo) * 0.6) a = 0;
    if (b - a < 1e-6) b = a + 1;
    const step = niceStep(b - a, 4);
    if (yMax === undefined) b = Math.ceil(b / step) * step;
    if (yMin === undefined) a = Math.floor(a / step) * step;
    return { a, b, step };
  }, [series, yMin, yMax, refLines]);

  const x0 = xs.length ? xs[0] : 0;
  const x1 = xs.length ? xs[xs.length - 1] : 1;
  const px = useCallback((x: number) => PAD.l + ((x - x0) / (x1 - x0 || 1)) * (w - PAD.l - PAD.r), [x0, x1, w]);
  const py = useCallback((v: number) => PAD.t + (1 - (v - scale.a) / (scale.b - scale.a)) * (height - PAD.t - PAD.b), [scale, height]);
  const fmtY = yFormat ?? ((v: number) => String(Math.round(v * 10) / 10));

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(height * dpr);
    const g = c.getContext('2d');
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, height);
    const grid = cssVar('--border', '#252d3f');
    const text = cssVar('--text-3', '#8592a8');
    g.font = '11px Inter Variable, Segoe UI, sans-serif';
    g.textBaseline = 'middle';
    // horizontal grid + y labels
    g.lineWidth = 1;
    for (let v = scale.a; v <= scale.b + 1e-9; v += scale.step) {
      const y = Math.round(py(v)) + 0.5;
      g.strokeStyle = grid;
      g.beginPath();
      g.moveTo(PAD.l, y);
      g.lineTo(w - PAD.r, y);
      g.stroke();
      g.fillStyle = text;
      g.textAlign = 'right';
      g.fillText(fmtY(v), PAD.l - 8, y);
    }
    // x labels
    g.textAlign = 'center';
    g.textBaseline = 'top';
    g.fillStyle = text;
    const nx = Math.max(2, Math.floor((w - PAD.l - PAD.r) / 110));
    for (let i = 0; i <= nx; i++) {
      const x = x0 + ((x1 - x0) * i) / nx;
      g.fillText(xFormat(x), px(x), height - PAD.b + 6);
    }
    // reference lines
    for (const r of refLines) {
      const y = py(r.y);
      g.strokeStyle = r.color ?? cssVar('--warn', '#fbbf24');
      g.setLineDash([4, 4]);
      g.beginPath();
      g.moveTo(PAD.l, y);
      g.lineTo(w - PAD.r, y);
      g.stroke();
      g.setLineDash([]);
      g.fillStyle = r.color ?? cssVar('--warn', '#fbbf24');
      g.textAlign = 'right';
      g.textBaseline = 'bottom';
      g.fillText(r.label, w - PAD.r - 4, y - 2);
    }
    // markers
    for (const m of markers) {
      if (m.x < x0 || m.x > x1) continue;
      g.strokeStyle = m.color;
      g.globalAlpha = 0.55;
      g.beginPath();
      g.moveTo(px(m.x), PAD.t);
      g.lineTo(px(m.x), height - PAD.b);
      g.stroke();
      g.globalAlpha = 1;
    }
    // series
    series.forEach((s, si) => {
      g.strokeStyle = s.color;
      g.lineWidth = 2;
      g.lineJoin = 'round';
      g.setLineDash(s.dash ?? SERIES_DASH[si % SERIES_DASH.length]);
      g.beginPath();
      let pen = false;
      s.data.forEach((v, i) => {
        if (v === null || xs[i] === undefined) {
          pen = false;
          return;
        }
        const X = px(xs[i]);
        const Y = py(v);
        if (!pen) g.moveTo(X, Y);
        else g.lineTo(X, Y);
        pen = true;
      });
      g.stroke();
      g.setLineDash([]);
      if (fill && si === 0) {
        g.save();
        g.globalAlpha = 0.1;
        g.fillStyle = s.color;
        g.beginPath();
        let started = false;
        let lastX = 0;
        s.data.forEach((v, i) => {
          if (v === null || xs[i] === undefined) return;
          const X = px(xs[i]);
          if (!started) {
            g.moveTo(X, py(scale.a));
            started = true;
          }
          g.lineTo(X, py(v));
          lastX = X;
        });
        if (started) {
          g.lineTo(lastX, py(scale.a));
          g.closePath();
          g.fill();
        }
        g.restore();
      }
    });
    // hover cursor
    if (hover !== null && xs[hover] !== undefined) {
      const X = px(xs[hover]);
      g.strokeStyle = cssVar('--text-2', '#adb8cc');
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(X, PAD.t);
      g.lineTo(X, height - PAD.b);
      g.stroke();
      series.forEach((s) => {
        const v = s.data[hover];
        if (v === null || v === undefined) return;
        g.fillStyle = s.color;
        g.beginPath();
        g.arc(X, py(v), 4, 0, Math.PI * 2);
        g.fill();
        g.strokeStyle = cssVar('--surface', '#141925');
        g.lineWidth = 2;
        g.stroke();
      });
    }
    // brush
    if (drag) {
      g.fillStyle = cssVar('--accent', '#3ddcff');
      g.globalAlpha = 0.15;
      g.fillRect(Math.min(drag.a, drag.b), PAD.t, Math.abs(drag.b - drag.a), height - PAD.t - PAD.b);
      g.globalAlpha = 1;
    }
  }, [series, xs, w, height, scale, hover, drag, markers, refLines, px, py, x0, x1, xFormat, fmtY, fill]);

  const indexAt = (clientX: number): number | null => {
    const r = canvas.current?.getBoundingClientRect();
    if (!r || xs.length === 0) return null;
    const x = clientX - r.left;
    const target = x0 + ((x - PAD.l) / (w - PAD.l - PAD.r)) * (x1 - x0);
    let lo = 0;
    let hi = xs.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (xs[mid] < target) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(xs[lo - 1] - target) < Math.abs(xs[lo] - target)) lo--;
    return lo;
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (!xs.length) return;
    const cur = hover ?? xs.length - 1;
    if (e.key === 'ArrowLeft') setHover(Math.max(0, cur - (e.shiftKey ? 10 : 1)));
    else if (e.key === 'ArrowRight') setHover(Math.min(xs.length - 1, cur + (e.shiftKey ? 10 : 1)));
    else if (e.key === 'Home') setHover(0);
    else if (e.key === 'End') setHover(xs.length - 1);
    else if (e.key === 'Escape') setHover(null);
    else return;
    e.preventDefault();
  };

  const tipLeft = hover !== null && xs[hover] !== undefined ? px(xs[hover]) : 0;
  const readout =
    hover !== null && xs[hover] !== undefined
      ? `${xFormat(xs[hover])}: ${series.map((s) => `${s.label} ${s.data[hover] != null ? s.format(s.data[hover] as number) : 'no data'}`).join(', ')}`
      : '';

  if (table) return <ChartTable series={series} xs={xs} xFormat={xFormat} onBack={() => setTable(false)} />;

  return (
    <div className="chart" ref={wrap}>
      <div className="chart-legend">
        {series.map((s, i) => (
          <span key={s.id}>
            <i style={{ borderColor: s.color, borderTopStyle: (s.dash ?? SERIES_DASH[i % 4]).length ? 'dashed' : 'solid' }} />
            {s.label}
          </span>
        ))}
        <span className="spacer" />
        <Button variant="ghost" size="sm" onClick={() => setTable(true)} aria-label="Show chart data as a table">
          <Table2 size={14} /> Table
        </Button>
      </div>
      <div style={{ position: 'relative' }}>
        <canvas
          ref={canvas}
          style={{ height, cursor: onBrush ? 'crosshair' : 'default', touchAction: 'none' }}
          role="img"
          tabIndex={0}
          aria-label={`${ariaLabel}. Use the left and right arrow keys to read values.`}
          onKeyDown={onKey}
          onBlur={() => setHover(null)}
          onMouseMove={(e) => {
            setHover(indexAt(e.clientX));
            if (drag) {
              const r = canvas.current!.getBoundingClientRect();
              setDrag({ a: drag.a, b: e.clientX - r.left });
            }
          }}
          onMouseLeave={() => {
            setHover(null);
            setDrag(null);
          }}
          onMouseDown={(e) => {
            if (!onBrush) return;
            const r = canvas.current!.getBoundingClientRect();
            setDrag({ a: e.clientX - r.left, b: e.clientX - r.left });
          }}
          onMouseUp={() => {
            if (drag && Math.abs(drag.b - drag.a) > 8) {
              const toX = (p: number) => x0 + ((p - PAD.l) / (w - PAD.l - PAD.r)) * (x1 - x0);
              onBrush?.(Math.min(toX(drag.a), toX(drag.b)), Math.max(toX(drag.a), toX(drag.b)));
            }
            setDrag(null);
          }}
        />
        {hover !== null && xs[hover] !== undefined && (
          <div className="chart-tip" style={{ left: Math.min(Math.max(tipLeft + 12, 0), w - 190), top: 8 }}>
            <div className="faint" style={{ marginBottom: 2 }}>
              {xFormat(xs[hover])}
            </div>
            {series.map((s) => (
              <div key={s.id} className="row" style={{ gap: '0.4rem' }}>
                <span style={{ width: 8, height: 8, borderRadius: 2, background: s.color, display: 'inline-block' }} />
                <span className="muted">{s.label}</span>
                <b style={{ marginLeft: 'auto', paddingLeft: '0.8rem' }}>{s.data[hover] != null ? s.format(s.data[hover] as number) : 'N/A'}</b>
              </div>
            ))}
          </div>
        )}
        <div className="sr-only" aria-live="polite">
          {readout}
        </div>
      </div>
    </div>
  );
}

function ChartTable({ series, xs, xFormat, onBack }: { series: ChartSeries[]; xs: number[]; xFormat: (x: number) => string; onBack: () => void }) {
  const step = Math.max(1, Math.ceil(xs.length / 60));
  const rows = xs.map((x, i) => ({ x, i })).filter((_, k) => k % step === 0);
  return (
    <div>
      <Button variant="ghost" size="sm" onClick={onBack}>
        ← Back to chart
      </Button>
      <div style={{ maxHeight: 260, overflow: 'auto', marginTop: 8 }}>
        <table className="table">
          <thead>
            <tr>
              <th>Time</th>
              {series.map((s) => (
                <th key={s.id} className="r">
                  {s.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map(({ x, i }) => (
              <tr key={i}>
                <td>{xFormat(x)}</td>
                {series.map((s) => (
                  <td key={s.id} className="r">
                    {s.data[i] != null ? s.format(s.data[i] as number) : 'N/A'}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {step > 1 && <p className="faint" style={{ fontSize: '0.8125rem', marginTop: 6 }}>Showing every {step}th sample.</p>}
    </div>
  );
}

// ───────────────────────────── frame-time histogram ─────────────────────────────

const FINE_BINS = 2000;

function binToMs(i: number): number {
  return i < FINE_BINS ? (i + 0.5) / 20 : 100 + (i - FINE_BINS) + 0.5;
}

export interface HistSeries {
  label: string;
  color: string;
  bins: [number, number][];
}

/** Distribution of frame times: a tall narrow peak means even pacing, a long tail means stutter. */
export function FrameHistogram({ series, height = 160, maxMs }: { series: HistSeries[]; height?: number; maxMs?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(Math.max(200, el.clientWidth)));
    ro.observe(el);
    setW(Math.max(200, el.clientWidth));
    return () => ro.disconnect();
  }, []);

  const data = useMemo(() => {
    // percentile helper for the x-range
    let hiMs = 0;
    for (const s of series) {
      const total = s.bins.reduce((a, b) => a + b[1], 0);
      let acc = 0;
      for (const [b, c] of s.bins) {
        acc += c;
        if (acc >= total * 0.9995) {
          hiMs = Math.max(hiMs, binToMs(b));
          break;
        }
      }
    }
    const max = maxMs ?? Math.min(200, Math.max(25, Math.ceil(hiMs * 1.15)));
    const nb = 90;
    const out = series.map((s) => {
      const bars = new Array<number>(nb).fill(0);
      let total = 0;
      for (const [b, c] of s.bins) {
        const ms = binToMs(b);
        total += c;
        const k = Math.min(nb - 1, Math.floor((ms / max) * nb));
        bars[k] += c;
      }
      return { ...s, bars: bars.map((v) => (total ? v / total : 0)) };
    });
    return { max, nb, out };
  }, [series, maxMs]);

  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(height * dpr);
    const g = c.getContext('2d');
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, height);
    const padL = 8;
    const padB = 22;
    const plotW = w - padL - 8;
    const plotH = height - padB - 6;
    const peak = Math.max(0.001, ...data.out.flatMap((s) => s.bars));
    const bw = plotW / data.nb;
    g.font = '11px Inter Variable, Segoe UI, sans-serif';
    g.fillStyle = cssVar('--text-3', '#8592a8');
    g.textAlign = 'center';
    g.textBaseline = 'top';
    const ticks = Math.max(2, Math.floor(plotW / 90));
    for (let i = 0; i <= ticks; i++) {
      const ms = (data.max * i) / ticks;
      g.fillText(`${ms.toFixed(0)} ms`, padL + (ms / data.max) * plotW, height - padB + 6);
    }
    data.out.forEach((s, si) => {
      g.fillStyle = s.color;
      g.globalAlpha = data.out.length > 1 ? 0.55 : 0.85;
      s.bars.forEach((v, k) => {
        const h = Math.sqrt(v / peak) * plotH;
        if (h > 0.5) g.fillRect(padL + k * bw + 0.5 + si, 6 + plotH - h, Math.max(1, bw - 1), h);
      });
      g.globalAlpha = 1;
    });
    g.strokeStyle = cssVar('--border-strong', '#38425a');
    g.beginPath();
    g.moveTo(padL, 6 + plotH + 0.5);
    g.lineTo(padL + plotW, 6 + plotH + 0.5);
    g.stroke();
  }, [data, w, height]);

  return (
    <div className="chart" ref={wrap}>
      {series.length > 1 && (
        <div className="chart-legend">
          {series.map((s) => (
            <span key={s.label}>
              <i style={{ borderColor: s.color }} />
              {s.label}
            </span>
          ))}
        </div>
      )}
      <canvas ref={ref} style={{ height }} role="img" aria-label="Distribution of frame times. A narrow peak on the left means smooth, even frames; a long tail to the right means stutter." />
    </div>
  );
}
