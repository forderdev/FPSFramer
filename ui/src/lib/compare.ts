import type { SessionData } from './types';

export interface CompareRow {
  id: string;
  label: string;
  a: number | null;
  b: number | null;
  /** Value formatter id: how to print it. */
  kind: 'fps' | 'ms' | 'int' | 'pct' | 'temp' | 'ratio' | 'dur';
  /** Whether a higher number is better, worse, or neither. */
  better: 'higher' | 'lower' | 'none';
}

const stat = (d: SessionData, id: string, k: 'avg' | 'max') => d.summary.sensorStats[id]?.[k] ?? null;

export function compareRows(a: SessionData, b: SessionData): CompareRow[] {
  const s = (d: SessionData) => d.summary;
  const r = (id: string, label: string, kind: CompareRow['kind'], better: CompareRow['better'], fa: number | null | undefined, fb: number | null | undefined): CompareRow => ({ id, label, kind, better, a: fa ?? null, b: fb ?? null });
  return [
    r('avg', 'Average FPS', 'fps', 'higher', s(a).fpsAvg, s(b).fpsAvg),
    r('low1', '1% Low FPS', 'fps', 'higher', s(a).low1, s(b).low1),
    r('low01', '0.1% Low FPS', 'fps', 'higher', s(a).low01, s(b).low01),
    r('min', 'Minimum FPS', 'fps', 'higher', s(a).fpsMin, s(b).fpsMin),
    r('max', 'Maximum FPS', 'fps', 'none', s(a).fpsMax, s(b).fpsMax),
    r('median', 'Typical frame time', 'ms', 'lower', s(a).medianMs, s(b).medianMs),
    r('p99', '99th-percentile frame time', 'ms', 'lower', s(a).p99Ms, s(b).p99Ms),
    r('worst', 'Slowest frame', 'ms', 'lower', s(a).worstMs, s(b).worstMs),
    r('cons', 'Consistency (1% Low ÷ average)', 'ratio', 'higher', s(a).consistency, s(b).consistency),
    r('spikes', 'Frame-time spikes', 'int', 'lower', s(a).spikeCount, s(b).spikeCount),
    r('drops', 'FPS drops', 'int', 'lower', s(a).dropCount, s(b).dropCount),
    r('cpu', 'Average CPU usage', 'pct', 'none', stat(a, 'cpu.load', 'avg'), stat(b, 'cpu.load', 'avg')),
    r('gpu', 'Average GPU usage', 'pct', 'none', stat(a, 'gpu.load', 'avg'), stat(b, 'gpu.load', 'avg')),
    r('cput', 'Highest CPU temperature', 'temp', 'lower', stat(a, 'cpu.temp', 'max'), stat(b, 'cpu.temp', 'max')),
    r('gput', 'Highest GPU temperature', 'temp', 'lower', stat(a, 'gpu.temp', 'max'), stat(b, 'gpu.temp', 'max')),
    r('ram', 'Highest memory use', 'pct', 'lower', stat(a, 'ram.percent', 'max'), stat(b, 'ram.percent', 'max')),
    r('vram', 'Highest video-memory use', 'pct', 'lower', stat(a, 'gpu.vram.percent', 'max'), stat(b, 'gpu.vram.percent', 'max')),
    r('dur', 'Length', 'dur', 'none', s(a).durationS, s(b).durationS),
  ];
}

export function delta(a: number | null, b: number | null): { abs: number; pct: number | null } | null {
  if (a === null || b === null) return null;
  return { abs: b - a, pct: a !== 0 ? ((b - a) / Math.abs(a)) * 100 : null };
}

/** Is the change from A to B an improvement? `null` when it doesn't matter or is negligible. */
export function verdictOf(row: CompareRow): 'better' | 'worse' | null {
  const d = delta(row.a, row.b);
  if (!d || row.better === 'none') return null;
  const rel = d.pct === null ? (d.abs === 0 ? 0 : 100) : Math.abs(d.pct);
  if (rel < 2) return null;
  const up = d.abs > 0;
  return (row.better === 'higher') === up ? 'better' : 'worse';
}

/** A short, plain-language summary of how B differs from A. Hedged: scenes and settings may differ. */
export function summarize(rows: CompareRow[]): string[] {
  const out: string[] = [];
  const get = (id: string) => rows.find((r) => r.id === id)!;
  const avg = delta(get('avg').a, get('avg').b);
  if (avg?.pct != null) {
    if (Math.abs(avg.pct) < 2) out.push('Average FPS was about the same in both sessions.');
    else out.push(`Average FPS was ${Math.abs(avg.pct).toFixed(0)}% ${avg.pct > 0 ? 'higher' : 'lower'} in the second session (${get('avg').a!.toFixed(0)} → ${get('avg').b!.toFixed(0)}).`);
  }
  const low = delta(get('low1').a, get('low1').b);
  if (low?.pct != null && Math.abs(low.pct) >= 2) out.push(`The 1% Low was ${Math.abs(low.pct).toFixed(0)}% ${low.pct > 0 ? 'better' : 'worse'}, meaning the dips felt ${low.pct > 0 ? 'milder' : 'harsher'}.`);
  const sp = delta(get('spikes').a, get('spikes').b);
  if (sp && sp.abs !== 0) out.push(`${Math.abs(sp.abs)} ${Math.abs(sp.abs) === 1 ? 'spike' : 'spikes'} ${sp.abs < 0 ? 'fewer' : 'more'} in the second session.`);
  const gt = delta(get('gput').a, get('gput').b);
  if (gt && Math.abs(gt.abs) >= 3) out.push(`The graphics card peaked ${Math.abs(gt.abs).toFixed(0)}°C ${gt.abs > 0 ? 'hotter' : 'cooler'}.`);
  if (out.length === 0) out.push('There is not enough data in common to describe a difference.');
  return out;
}

/** Do two sessions come from the same machine setup? */
export function hardwareDiff(a: SessionData, b: SessionData): string[] {
  const out: string[] = [];
  const A = a.meta.hardware;
  const B = b.meta.hardware;
  if (A.cpu && B.cpu && A.cpu !== B.cpu) out.push(`CPU: ${A.cpu} → ${B.cpu}`);
  if (A.gpus.join() !== B.gpus.join() && (A.gpus.length || B.gpus.length)) out.push(`Graphics: ${A.gpus.join(', ') || '?'} → ${B.gpus.join(', ') || '?'}`);
  if (A.ramGb && B.ramGb && Math.abs(A.ramGb - B.ramGb) > 0.5) out.push(`Memory: ${A.ramGb} GB → ${B.ramGb} GB`);
  if (a.meta.api && b.meta.api && a.meta.api !== b.meta.api) out.push(`Graphics API: ${a.meta.api} → ${b.meta.api}`);
  if (a.meta.exe !== b.meta.exe && a.meta.exe && b.meta.exe) out.push(`Different programs: ${a.meta.exe} and ${b.meta.exe}`);
  return out;
}
