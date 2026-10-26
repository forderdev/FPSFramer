import { describe, expect, it } from 'vitest';
import { decimate } from '../src/lib/decimate';

describe('decimate', () => {
  it('leaves short series alone', () => {
    const xs = [0, 1, 2];
    expect(decimate(xs, [{ data: [1, 2, 3] }], 10).xs).toEqual(xs);
  });

  it('averages ordinary series and keeps peaks for peak series', () => {
    const xs = Array.from({ length: 100 }, (_, i) => i);
    const data = xs.map((i) => (i === 37 ? 100 : 10));
    const r = decimate(xs, [{ data }, { data, peak: true }], 10);
    expect(r.xs.length).toBe(10);
    expect(Math.max(...(r.series[0] as number[]))).toBeLessThan(30); // smoothed
    expect(Math.max(...(r.series[1] as number[]))).toBe(100); // spike preserved
  });

  it('keeps gaps as gaps', () => {
    const xs = Array.from({ length: 20 }, (_, i) => i);
    const data = xs.map((i) => (i < 10 ? null : 5));
    const r = decimate(xs, [{ data }], 4);
    expect(r.series[0][0]).toBeNull();
    expect(r.series[0][3]).toBe(5);
  });
});
