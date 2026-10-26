import { describe, expect, it } from 'vitest';
import {
  addElement, addMetrics, alignElements, bringToFront, convertToFree, convertToStack, distribute, duplicateElements, getPath, moveElement, newElement, removeElements, sendToBack, setPath, snap, updateElement,
} from '../src/lib/layoutOps';
import type { Layout, LayoutEl, SensorDesc } from '../src/lib/types';

const canvas: Layout['canvas'] = {
  arrangement: 'stack', direction: 'column', columns: 1, gap: 4, padding: 10, unavailable: 'hide',
  background: { color: '#000000', opacity: 0.7, radius: 8 }, font: { family: 'Inter', size: 13, weight: 500 },
  textColor: '#ffffff', labelColor: '#cccccc', accent: '#3ddcff', textShadow: 'soft', colors: { elevated: '#ffb454', critical: '#ff5d5d' },
};
const layout = (els: Partial<LayoutEl>[], over: Partial<Layout['canvas']> = {}): Layout => ({
  schemaVersion: 1, id: 'lay_t', name: 'T', description: '', builtin: false, tags: [], canvas: { ...canvas, ...over },
  elements: els.map((e, i) => ({ id: `e${i}`, type: 'text', ...e }) as LayoutEl), createdAt: 0, updatedAt: 0,
});
const ids = (l: Layout) => l.elements.map((e) => e.id);

const catalog = {
  fps: { id: 'fps', name: 'Frames per second', short: 'FPS', unit: 'fps', category: 'fps' },
  'gpu.temp': { id: 'gpu.temp', name: 'GPU temperature', short: 'GPU temp', unit: 'celsius', category: 'gpu' },
} as unknown as Record<string, SensorDesc>;

describe('paths', () => {
  it('reads and writes nested values without mutating', () => {
    const el = { style: { fontSize: 12 } };
    const next = setPath(el, 'style.fontSize', 20);
    expect(getPath(next, 'style.fontSize')).toBe(20);
    expect(el.style.fontSize).toBe(12);
    expect(getPath(setPath({}, 'a.b.c', 1), 'a.b.c')).toBe(1);
  });
  it('removes a key when set to undefined', () => {
    expect('color' in (setPath({ style: { color: 'red', align: 'left' } }, 'style.color', undefined) as { style: object }).style).toBe(false);
  });
});

describe('editing a list of items', () => {
  const base = layout([{}, {}, {}]);
  it('adds after a given item or at the end', () => {
    const el = { id: 'new', type: 'text' } as LayoutEl;
    expect(ids(addElement(base, el, 'e0'))).toEqual(['e0', 'new', 'e1', 'e2']);
    expect(ids(addElement(base, el))).toEqual(['e0', 'e1', 'e2', 'new']);
  });
  it('removes several at once', () => {
    expect(ids(removeElements(base, ['e0', 'e2']))).toEqual(['e1']);
  });
  it('moves up and down and stops at the ends', () => {
    expect(ids(moveElement(base, 'e1', -1))).toEqual(['e1', 'e0', 'e2']);
    expect(ids(moveElement(base, 'e0', -1))).toEqual(['e0', 'e1', 'e2']);
    expect(ids(moveElement(base, 'e2', 1))).toEqual(['e0', 'e1', 'e2']);
  });
  it('brings to front and sends to back', () => {
    expect(ids(bringToFront(base, 'e0'))).toEqual(['e1', 'e2', 'e0']);
    expect(ids(sendToBack(base, 'e2'))).toEqual(['e2', 'e0', 'e1']);
  });
  it('duplicates with a new id, offset, and returns the new ids', () => {
    const l = layout([{ x: 10, y: 20 }], { arrangement: 'free' });
    const { layout: out, newIds } = duplicateElements(l, ['e0']);
    expect(out.elements).toHaveLength(2);
    expect(newIds).toHaveLength(1);
    expect(out.elements[1].id).toBe(newIds[0]);
    expect(out.elements[1].id).not.toBe('e0');
    expect([out.elements[1].x, out.elements[1].y]).toEqual([22, 32]);
  });
  it('updates one item immutably', () => {
    const out = updateElement(base, 'e1', { visible: false });
    expect(out.elements[1].visible).toBe(false);
    expect(base.elements[1].visible).toBeUndefined();
  });
});

describe('adding measurements', () => {
  it('creates one metric per sensor, labelled from the catalogue', () => {
    const out = addMetrics(layout([]), ['fps', 'gpu.temp'], catalog);
    expect(out.elements).toHaveLength(2);
    expect(out.elements.map((e) => e.type)).toEqual(['metric', 'metric']);
    expect(out.elements.map((e) => e.values?.[0].sensorId)).toEqual(['fps', 'gpu.temp']);
  });
  it('stacks new items below existing ones in a free layout', () => {
    const l = layout([{ type: 'text', x: 12, y: 12, w: 100, h: 20 }], { arrangement: 'free' });
    const out = addMetrics(l, ['fps', 'gpu.temp'], catalog);
    const [a, b] = out.elements.slice(1);
    expect(a.y!).toBeGreaterThanOrEqual(32);
    expect(b.y!).toBeGreaterThan(a.y!);
  });
  it('knows how to build every element type', () => {
    for (const t of ['metric', 'cores', 'text', 'clock', 'image', 'shape', 'graph', 'alerts', 'divider', 'spacer'] as const) {
      const e = newElement(t, { sensorId: 'fps', catalog });
      expect(e.type).toBe(t);
      expect(e.id).toBeTruthy();
    }
  });
});

describe('arrangement', () => {
  it('freezes measured positions when switching to free placement', () => {
    const l = layout([{}, {}]);
    const out = convertToFree(l, { e0: { x: 4.4, y: 5.6, w: 100.2, h: 20 }, e1: { x: 4, y: 30, w: 100, h: 20 } }, { w: 120, h: 60 });
    expect(out.canvas.arrangement).toBe('free');
    expect(out.elements[0]).toMatchObject({ x: 4, y: 6, w: 100, h: 20 });
    expect(out.canvas.width).toBe(120);
  });
  it('flows items top-to-bottom again when switching back', () => {
    const l = layout([{ y: 40, x: 0 }, { y: 10, x: 5 }, { y: 10, x: 0 }], { arrangement: 'free' });
    const out = convertToStack(l);
    expect(ids(out)).toEqual(['e2', 'e1', 'e0']);
    expect(out.canvas.arrangement).toBe('stack');
    expect(out.elements.every((e) => e.x === undefined && e.y === undefined)).toBe(true);
  });
});

describe('free-mode geometry', () => {
  it('snaps to a grid', () => {
    expect(snap(13, 8)).toBe(16);
    expect(snap(11, 8)).toBe(8);
    expect(Math.abs(snap(-3, 8))).toBe(0);
    expect(snap(-13, 8)).toBe(-16);
  });
  const rects = { e0: { x: 0, y: 0, w: 20, h: 10 }, e1: { x: 50, y: 30, w: 40, h: 10 }, e2: { x: 100, y: 60, w: 20, h: 10 } };
  const free = layout([{ x: 0, y: 0, w: 20, h: 10 }, { x: 50, y: 30, w: 40, h: 10 }, { x: 100, y: 60, w: 20, h: 10 }], { arrangement: 'free' });
  it('aligns to the left edge of the selection', () => {
    const out = alignElements(free, ['e0', 'e1', 'e2'], 'left', rects);
    expect(out.elements.map((e) => e.x)).toEqual([0, 0, 0]);
  });
  it('aligns centres', () => {
    const out = alignElements(free, ['e0', 'e1'], 'center', rects);
    const c = (e: LayoutEl) => e.x! + e.w! / 2;
    expect(c(out.elements[0])).toBeCloseTo(c(out.elements[1]), 5);
  });
  it('distributes with equal gaps', () => {
    const out = distribute(free, ['e0', 'e1', 'e2'], 'h', rects);
    const [a, b, c] = out.elements;
    expect(b.x! - (a.x! + a.w!)).toBeCloseTo(c.x! - (b.x! + b.w!), 5);
  });
});
