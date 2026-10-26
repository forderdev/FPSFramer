/** Reduce a long series to at most `max` points. `peak` series keep each bucket's maximum so spikes survive. */
export function decimate(xs: number[], series: { data: (number | null)[]; peak?: boolean }[], max: number): { xs: number[]; series: (number | null)[][] } {
  const n = xs.length;
  if (n <= max) return { xs, series: series.map((s) => s.data) };
  const per = Math.ceil(n / max);
  const outX: number[] = [];
  const outS: (number | null)[][] = series.map(() => []);
  for (let i = 0; i < n; i += per) {
    const end = Math.min(n, i + per);
    outX.push(xs[i + ((end - i) >> 1)]);
    series.forEach((s, k) => {
      let sum = 0;
      let cnt = 0;
      let peak = -Infinity;
      for (let j = i; j < end; j++) {
        const v = s.data[j];
        if (v === null || v === undefined) continue;
        sum += v;
        cnt++;
        if (v > peak) peak = v;
      }
      outS[k].push(cnt === 0 ? null : s.peak ? peak : sum / cnt);
    });
  }
  return { xs: outX, series: outS };
}
