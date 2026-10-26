import { describe, expect, it } from 'vitest';
import { compareRows, delta, hardwareDiff, summarize, verdictOf } from '../src/lib/compare';
import type { SessionData } from '../src/lib/types';

function session(over: Partial<SessionData['summary']> = {}, hw: Partial<SessionData['meta']['hardware']> = {}): SessionData {
  return {
    schemaVersion: 1,
    meta: { id: 'x', name: '', kind: 'game', startedAt: 0, endedAt: 0, durationS: 100, gameName: 'G', exe: 'g.exe', api: 'DirectX 12', profileId: null, profileName: null, demo: false, hardware: { cpu: 'C', gpus: ['G1'], ramGb: 32, os: '', displays: [], appVersion: '', ...hw }, notes: '', starred: false },
    summary: { durationS: 100, frames: 1000, fpsAvg: 100, fpsMin: 50, fpsMax: 150, low1: 70, low01: 50, medianMs: 10, p99Ms: 14, worstMs: 30, consistency: 0.7, spikeCount: 4, dropCount: 1, targetFps: null, belowTargetPct: null, sensorStats: {}, bottleneck: null, findings: [], ...over },
    intervalMs: 1000,
    columns: {},
    hist: { bins: [], sumMs: 0 },
    spikes: [],
    drops: [],
    alerts: [],
  };
}

describe('session comparison', () => {
  it('computes deltas and handles missing values honestly', () => {
    expect(delta(100, 110)).toEqual({ abs: 10, pct: 10 });
    expect(delta(null, 110)).toBeNull();
    expect(delta(0, 5)?.pct).toBeNull();
  });

  it('knows which direction is an improvement', () => {
    const rows = compareRows(session(), session({ fpsAvg: 120, spikeCount: 1, medianMs: 8 }));
    const by = (id: string) => rows.find((r) => r.id === id)!;
    expect(verdictOf(by('avg'))).toBe('better');
    expect(verdictOf(by('spikes'))).toBe('better'); // fewer spikes
    expect(verdictOf(by('median'))).toBe('better'); // lower frame time
    expect(verdictOf(by('low1'))).toBeNull(); // unchanged
    expect(verdictOf(by('max'))).toBeNull(); // direction doesn't matter
    const worse = compareRows(session(), session({ fpsAvg: 80 }));
    expect(verdictOf(worse.find((r) => r.id === 'avg')!)).toBe('worse');
  });

  it('writes a hedged plain-language summary', () => {
    const lines = summarize(compareRows(session(), session({ fpsAvg: 120, low1: 60, spikeCount: 1 })));
    expect(lines.join(' ')).toContain('20% higher');
    expect(lines.join(' ')).toContain('worse');
    expect(lines.join(' ')).toContain('3 spikes fewer');
  });

  it('flags hardware differences so unlike things are not compared silently', () => {
    expect(hardwareDiff(session(), session()).length).toBe(0);
    const d = hardwareDiff(session(), session({}, { gpus: ['Other GPU'], ramGb: 16 }));
    expect(d.some((x) => x.startsWith('Graphics'))).toBe(true);
    expect(d.some((x) => x.startsWith('Memory'))).toBe(true);
  });
});
