// Pure functions that edit overlay layouts. Everything returns a new layout (no mutation) so the
// editor can keep an undo history of plain snapshots.

import type { Layout, LayoutCanvas, LayoutEl, SensorDesc } from './types';
import { newId } from './labels';

export type ElType = LayoutEl['type'];

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

// ───────────────────────────── paths ─────────────────────────────

export function getPath(obj: unknown, path: string): unknown {
  return path.split('.').reduce<unknown>((o, k) => (o && typeof o === 'object' ? (o as Record<string, unknown>)[k] : undefined), obj);
}

/** Immutable set: `setPath(el, 'style.fontSize', 20)`. Setting `undefined` removes the key. */
export function setPath<T>(obj: T, path: string, value: unknown): T {
  const keys = path.split('.');
  const rec = (o: unknown, i: number): unknown => {
    const base: Record<string, unknown> = o && typeof o === 'object' ? { ...(o as Record<string, unknown>) } : {};
    const k = keys[i];
    if (i === keys.length - 1) {
      if (value === undefined) delete base[k];
      else base[k] = value;
    } else {
      base[k] = rec(base[k], i + 1);
    }
    return base;
  };
  return rec(obj, 0) as T;
}

// ───────────────────────────── labels ─────────────────────────────

const TYPE_LABEL: Record<ElType, string> = {
  metric: 'Measurement',
  cores: 'CPU cores',
  text: 'Text',
  clock: 'Clock',
  image: 'Image',
  shape: 'Shape',
  graph: 'Graph',
  alerts: 'Alerts',
  divider: 'Divider line',
  spacer: 'Spacer',
};

export function typeLabel(t: ElType): string {
  return TYPE_LABEL[t];
}

/** A friendly name for the layers list / quick toggles. */
export function elementLabel(el: LayoutEl, catalog: Record<string, SensorDesc>): string {
  if (el.name) return el.name;
  switch (el.type) {
    case 'metric': {
      const lab = el.label?.mode === 'text' ? el.label.text : undefined;
      const first = el.values?.[0];
      const d = first ? catalog[first.sensorId] : undefined;
      const base = lab || d?.name || first?.sensorId || 'Measurement';
      return (el.values?.length ?? 0) > 1 ? `${base} + ${(el.values?.length ?? 1) - 1} more` : base;
    }
    case 'graph':
      return el.title || `Graph${el.source === 'frames' ? ' (frame time)' : ''}`;
    case 'text':
      return el.text ? `“${el.text.slice(0, 24)}”` : 'Text';
    case 'clock':
      return el.mode === 'session' ? 'Session timer' : el.mode === 'date' ? 'Date' : 'Clock';
    default:
      return TYPE_LABEL[el.type];
  }
}

// ───────────────────────────── element factory ─────────────────────────────

export function newElement(type: ElType, opts: { sensorId?: string; catalog?: Record<string, SensorDesc>; free?: boolean } = {}): LayoutEl {
  const base: LayoutEl = { id: newId('el'), type, visible: true };
  switch (type) {
    case 'metric': {
      const id = opts.sensorId ?? 'fps';
      const d = opts.catalog?.[id];
      return { ...base, label: { mode: 'text', text: d?.short ?? id }, values: [{ sensorId: id, colorMode: 'status' }], ...(opts.free ? { w: 150, h: 22 } : {}) };
    }
    case 'cores':
      return { ...base, metric: 'load', columns: 8, barHeight: 16, showNumbers: true, span: 12, ...(opts.free ? { w: 260, h: 18 } : {}) };
    case 'text':
      return { ...base, text: 'Your text', ...(opts.free ? { w: 120, h: 22 } : {}) };
    case 'clock':
      return { ...base, mode: 'time', seconds: false, hour12: false, style: { fontWeight: 700 }, ...(opts.free ? { w: 80, h: 22 } : {}) };
    case 'image':
      return { ...base, fit: 'contain', w: 48, h: 48 };
    case 'shape':
      return { ...base, shape: 'rect', fill: '#3ddcff', fillOpacity: 0.25, stroke: '#3ddcff', strokeWidth: 1, radius: 6, w: 120, h: 40 };
    case 'graph':
      return {
        ...base,
        source: 'history',
        seconds: 60,
        series: [{ sensorId: opts.sensorId ?? 'gpu.load', color: '#3ddcff' }],
        w: 180,
        h: 44,
        grid: true,
        showLabel: true,
        title: opts.catalog?.[opts.sensorId ?? 'gpu.load']?.short ?? 'Graph',
        span: 12,
      };
    case 'alerts':
      return { ...base, maxVisible: 3, span: 12, ...(opts.free ? { w: 220, h: 24 } : {}) };
    case 'divider':
      return { ...base, size: 4, span: 12, ...(opts.free ? { w: 200, h: 6 } : {}) };
    case 'spacer':
      return { ...base, size: 8 };
  }
}

export function frameTimeGraph(): LayoutEl {
  return { id: newId('el'), type: 'graph', visible: true, source: 'frames', frames: 240, series: [{ sensorId: 'frametime', color: '#3ddcff' }], w: 170, h: 34, fill: true, grid: false, showLabel: true, title: 'Frame time', span: 12 };
}

// ───────────────────────────── edits ─────────────────────────────

function withEls(l: Layout, els: LayoutEl[]): Layout {
  return { ...l, elements: els };
}

export function updateElement(l: Layout, id: string, f: Partial<LayoutEl> | ((e: LayoutEl) => LayoutEl)): Layout {
  return withEls(l, l.elements.map((e) => (e.id === id ? (typeof f === 'function' ? f(e) : { ...e, ...f }) : e)));
}

export function setElementPath(l: Layout, id: string, path: string, value: unknown): Layout {
  return updateElement(l, id, (e) => setPath(e, path, value));
}

export function updateCanvas(l: Layout, path: string, value: unknown): Layout {
  return { ...l, canvas: setPath(l.canvas, path, value) };
}

export function addElement(l: Layout, el: LayoutEl, afterId?: string | null): Layout {
  const els = [...l.elements];
  const i = afterId ? els.findIndex((e) => e.id === afterId) : -1;
  if (i >= 0) els.splice(i + 1, 0, el);
  else els.push(el);
  return withEls(l, els);
}

export function addMetrics(l: Layout, sensorIds: string[], catalog: Record<string, SensorDesc>): Layout {
  let out = l;
  for (const id of sensorIds) out = addElement(out, newElement('metric', { sensorId: id, catalog, free: l.canvas.arrangement === 'free' }));
  if (l.canvas.arrangement === 'free') out = placeNewElementsBelow(out, out.elements.slice(-sensorIds.length).map((e) => e.id));
  return out;
}

/** In free mode, stack freshly added elements under everything else. */
export function placeNewElementsBelow(l: Layout, ids: string[]): Layout {
  let y = Math.max(0, ...l.elements.filter((e) => !ids.includes(e.id) && e.visible !== false).map((e) => (e.y ?? 0) + (e.h ?? 22))) + 6;
  return withEls(
    l,
    l.elements.map((e) => {
      if (!ids.includes(e.id)) return e;
      const r = { ...e, x: e.x ?? 12, y };
      y += (e.h ?? 22) + 6;
      return r;
    }),
  );
}

export function removeElements(l: Layout, ids: string[]): Layout {
  return withEls(l, l.elements.filter((e) => !ids.includes(e.id)));
}

export function duplicateElements(l: Layout, ids: string[]): { layout: Layout; newIds: string[] } {
  const els: LayoutEl[] = [];
  const newIds: string[] = [];
  for (const e of l.elements) {
    els.push(e);
    if (ids.includes(e.id)) {
      const copy: LayoutEl = JSON.parse(JSON.stringify(e));
      copy.id = newId('el');
      if (copy.x !== undefined) copy.x += 12;
      if (copy.y !== undefined) copy.y += 12;
      els.push(copy);
      newIds.push(copy.id);
    }
  }
  return { layout: withEls(l, els), newIds };
}

export function moveElement(l: Layout, id: string, dir: -1 | 1): Layout {
  const els = [...l.elements];
  const i = els.findIndex((e) => e.id === id);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= els.length) return l;
  [els[i], els[j]] = [els[j], els[i]];
  return withEls(l, els);
}

export function moveElementTo(l: Layout, id: string, toIndex: number): Layout {
  const els = [...l.elements];
  const i = els.findIndex((e) => e.id === id);
  if (i < 0) return l;
  const [el] = els.splice(i, 1);
  els.splice(Math.max(0, Math.min(els.length, toIndex)), 0, el);
  return withEls(l, els);
}

/** Bring forward / send backward in free mode = later elements draw on top. */
export function bringToFront(l: Layout, id: string): Layout {
  return moveElementTo(l, id, l.elements.length);
}
export function sendToBack(l: Layout, id: string): Layout {
  return moveElementTo(l, id, 0);
}

// ───────────────────────────── arrangement ─────────────────────────────

/** Switch a stacked layout to free positioning, freezing the positions the elements currently have. */
export function convertToFree(l: Layout, rects: Record<string, Rect>, size: { w: number; h: number }): Layout {
  const els = l.elements.map((e) => {
    const r = rects[e.id];
    return r ? { ...e, x: Math.round(r.x), y: Math.round(r.y), w: Math.max(8, Math.round(r.w)), h: Math.max(8, Math.round(r.h)) } : e;
  });
  return { ...l, canvas: { ...l.canvas, arrangement: 'free', width: Math.round(size.w), height: Math.round(size.h), padding: 0 }, elements: els };
}

/** Switch back to a flowing list, ordered top-to-bottom then left-to-right. */
export function convertToStack(l: Layout): Layout {
  const els = [...l.elements].sort((a, b) => (a.y ?? 0) - (b.y ?? 0) || (a.x ?? 0) - (b.x ?? 0));
  const cleaned = els.map((e) => {
    const c = { ...e };
    delete c.x;
    delete c.y;
    return c;
  });
  const canvas: LayoutCanvas = { ...l.canvas, arrangement: 'stack', direction: 'column', padding: l.canvas.padding || 10, height: undefined };
  return { ...l, canvas, elements: cleaned };
}

// ───────────────────────────── free-mode geometry ─────────────────────────────

export function snap(v: number, grid: number): number {
  return Math.round(v / grid) * grid;
}

export type Align = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';

export function alignElements(l: Layout, ids: string[], how: Align, rects: Record<string, Rect>): Layout {
  if (ids.length < 2) return l;
  const rs = ids.map((i) => rects[i]).filter(Boolean);
  const minX = Math.min(...rs.map((r) => r.x));
  const maxX = Math.max(...rs.map((r) => r.x + r.w));
  const minY = Math.min(...rs.map((r) => r.y));
  const maxY = Math.max(...rs.map((r) => r.y + r.h));
  return withEls(
    l,
    l.elements.map((e) => {
      const r = rects[e.id];
      if (!ids.includes(e.id) || !r) return e;
      switch (how) {
        case 'left':
          return { ...e, x: Math.round(minX) };
        case 'right':
          return { ...e, x: Math.round(maxX - r.w) };
        case 'center':
          return { ...e, x: Math.round((minX + maxX) / 2 - r.w / 2) };
        case 'top':
          return { ...e, y: Math.round(minY) };
        case 'bottom':
          return { ...e, y: Math.round(maxY - r.h) };
        case 'middle':
          return { ...e, y: Math.round((minY + maxY) / 2 - r.h / 2) };
      }
    }),
  );
}

export function distribute(l: Layout, ids: string[], axis: 'h' | 'v', rects: Record<string, Rect>): Layout {
  if (ids.length < 3) return l;
  const sorted = ids.map((i) => ({ id: i, r: rects[i] })).filter((x) => x.r).sort((a, b) => (axis === 'h' ? a.r.x - b.r.x : a.r.y - b.r.y));
  const first = sorted[0].r;
  const last = sorted[sorted.length - 1].r;
  const total = sorted.reduce((s, x) => s + (axis === 'h' ? x.r.w : x.r.h), 0);
  const span = axis === 'h' ? last.x + last.w - first.x : last.y + last.h - first.y;
  const gap = (span - total) / (sorted.length - 1);
  let pos = axis === 'h' ? first.x : first.y;
  const target: Record<string, number> = {};
  for (const x of sorted) {
    target[x.id] = Math.round(pos);
    pos += (axis === 'h' ? x.r.w : x.r.h) + gap;
  }
  return withEls(l, l.elements.map((e) => (e.id in target ? (axis === 'h' ? { ...e, x: target[e.id] } : { ...e, y: target[e.id] }) : e)));
}

/** Resize the free canvas so it fits all visible elements with `pad` px around them. */
export function fitCanvasToContent(l: Layout, pad = 8): Layout {
  if (l.canvas.arrangement !== 'free') return l;
  const vis = l.elements.filter((e) => e.visible !== false);
  if (!vis.length) return l;
  const w = Math.max(...vis.map((e) => (e.x ?? 0) + (e.w ?? 40))) + pad;
  const h = Math.max(...vis.map((e) => (e.y ?? 0) + (e.h ?? 22))) + pad;
  return updateCanvas(updateCanvas(l, 'width', Math.round(w)), 'height', Math.round(h));
}

// ───────────────────────────── quick style ─────────────────────────────

/** Recolour every element that used the old accent colour. */
export function setAccent(l: Layout, color: string): Layout {
  const old = l.canvas.accent.toLowerCase();
  const els = l.elements.map((e) => {
    let out = e;
    if (e.series?.length) out = { ...out, series: e.series.map((s) => (s.color?.toLowerCase() === old ? { ...s, color } : s)) };
    if (e.type === 'shape') {
      if (typeof e.fill === 'string' && e.fill.toLowerCase() === old) out = { ...out, fill: color };
      if (e.stroke?.toLowerCase() === old) out = { ...out, stroke: color };
    }
    return out;
  });
  return { ...l, canvas: { ...l.canvas, accent: color }, elements: els };
}

/** Sensor ids used by an element (for warnings and series tracking). */
export function sensorsOf(e: LayoutEl): string[] {
  return [...(e.values ?? []).map((v) => v.sensorId), ...(e.series ?? []).map((s) => s.sensorId)];
}
