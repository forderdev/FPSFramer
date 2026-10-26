// Client-side rolling buffers of recent readings for sparklines and overlay graphs.
// Only sensors that something is displaying ("tracked") are recorded, which keeps
// the lightweight overlay window small.

import { useEffect, useRef, useSyncExternalStore } from 'react';
import type { Snapshot } from './types';

const MAX_AGE_MS = 330_000;
const MAX_FRAMES = 1500;

interface Buf {
  t: number[];
  v: (number | null)[];
}

class SeriesStore {
  private bufs = new Map<string, Buf>();
  private refs = new Map<string, number>();
  private listeners = new Set<() => void>();
  private version = 0;
  frames: number[] = [];

  /** Start recording these sensors (ref-counted). Returns a release function. */
  track(ids: string[]): () => void {
    for (const id of ids) this.refs.set(id, (this.refs.get(id) ?? 0) + 1);
    return () => {
      for (const id of ids) {
        const n = (this.refs.get(id) ?? 1) - 1;
        if (n <= 0) {
          this.refs.delete(id);
          this.bufs.delete(id);
        } else this.refs.set(id, n);
      }
    };
  }

  push(s: Snapshot): void {
    for (const id of this.refs.keys()) {
      const raw = s.values[id];
      let b = this.bufs.get(id);
      if (!b) this.bufs.set(id, (b = { t: [], v: [] }));
      b.t.push(s.t);
      b.v.push(typeof raw === 'number' ? raw : null);
      const cut = s.t - MAX_AGE_MS;
      let i = 0;
      while (i < b.t.length - 1 && b.t[i] < cut) i++;
      if (i > 0) {
        b.t.splice(0, i);
        b.v.splice(0, i);
      }
    }
    if (s.frames.length) {
      this.frames.push(...s.frames);
      if (this.frames.length > MAX_FRAMES) this.frames.splice(0, this.frames.length - MAX_FRAMES);
    }
    this.version++;
    this.listeners.forEach((l) => l());
  }

  /** Fill in history from before this window opened (1 s steps ending at `endMs`). */
  seed(id: string, values: (number | null)[], endMs: number, intervalMs: number): void {
    const b = this.bufs.get(id);
    if (!b) return;
    const firstLive = b.t.length ? b.t[0] : Infinity;
    const t: number[] = [];
    const v: (number | null)[] = [];
    values.forEach((x, i) => {
      const ts = endMs - (values.length - 1 - i) * intervalMs;
      if (ts < firstLive - intervalMs / 2) {
        t.push(ts);
        v.push(x);
      }
    });
    b.t.unshift(...t);
    b.v.unshift(...v);
  }

  get(id: string, seconds: number): { t: number[]; v: (number | null)[] } {
    const b = this.bufs.get(id);
    if (!b || !b.t.length) return { t: [], v: [] };
    const cut = b.t[b.t.length - 1] - seconds * 1000;
    let i = 0;
    while (i < b.t.length && b.t[i] < cut) i++;
    return { t: b.t.slice(i), v: b.v.slice(i) };
  }

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
  getVersion = (): number => this.version;
}

export const series = new SeriesStore();

/** Re-render when new data arrives. */
export function useSeriesTick(): number {
  return useSyncExternalStore(series.subscribe, series.getVersion);
}

/** Track `ids` while the component is mounted and return the last `seconds` of each. */
export function useSeries(ids: string[], seconds: number): Record<string, { t: number[]; v: (number | null)[] }> {
  const key = ids.join('|');
  useEffect(() => series.track(ids), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  useSeriesTick();
  const out: Record<string, { t: number[]; v: (number | null)[] }> = {};
  for (const id of ids) out[id] = series.get(id, seconds);
  return out;
}

export function useFrames(count: number): number[] {
  useSeriesTick();
  const f = series.frames;
  const ref = useRef<number[]>([]);
  ref.current = f.length > count ? f.slice(f.length - count) : f.slice();
  return ref.current;
}
