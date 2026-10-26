import { ArrowLeft, ArrowRight, TrendingDown, TrendingUp } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { FrameHistogram, LineChart, type ChartSeries } from '../components/Charts';
import { Banner, Button, Card, CardHead, Empty, PageHead, Select } from '../components/ui';
import { compareRows, delta, hardwareDiff, summarize, verdictOf, type CompareRow } from '../lib/compare';
import { formatClock, formatDateTime, formatDuration } from '../lib/format';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { SessionData, SessionListItem } from '../lib/types';
import { sessionTitle } from './Sessions';

function fmt(r: CompareRow, v: number | null, temp: 'celsius' | 'fahrenheit'): string {
  if (v === null) return 'N/A';
  switch (r.kind) {
    case 'fps':
      return v.toFixed(0);
    case 'ms':
      return `${v.toFixed(1)} ms`;
    case 'int':
      return String(Math.round(v));
    case 'pct':
      return `${v.toFixed(0)}%`;
    case 'temp':
      return temp === 'fahrenheit' ? `${((v * 9) / 5 + 32).toFixed(0)}°F` : `${v.toFixed(0)}°C`;
    case 'ratio':
      return `${(v * 100).toFixed(0)}%`;
    case 'dur':
      return formatDuration(v);
  }
}

export default function ComparePage({ a, b }: { a: string; b: string }) {
  const [list, setList] = useState<SessionListItem[]>([]);
  const [A, setA] = useState<SessionData | null>(null);
  const [B, setB] = useState<SessionData | null>(null);
  const [failed, setFailed] = useState(false);
  const temp = useApp((s) => s.settings.units.temperature);

  useEffect(() => {
    void act<SessionListItem[]>('sessions.list').then((l) => setList(l ?? []));
  }, []);
  useEffect(() => {
    setA(null);
    setB(null);
    setFailed(false);
    if (!a || !b) return;
    let alive = true;
    void Promise.all([act<SessionData>('sessions.get', { id: a }), act<SessionData>('sessions.get', { id: b })]).then(([x, y]) => {
      if (!alive) return;
      if (!x || !y) return setFailed(true);
      setA(x);
      setB(y);
    });
    return () => {
      alive = false;
    };
  }, [a, b]);

  const options = list.map((i) => ({ value: i.meta.id, label: `${sessionTitle(i.meta)}, ${formatDateTime(i.meta.startedAt)}` }));
  const choose = (na: string, nb: string) => navigate(`/compare?a=${na}&b=${nb}`);

  const rows = useMemo(() => (A && B ? compareRows(A, B) : []), [A, B]);
  const hw = useMemo(() => (A && B ? hardwareDiff(A, B) : []), [A, B]);

  return (
    <div className="page">
      <div>
        <Button variant="ghost" size="sm" onClick={() => navigate('/sessions')}>
          <ArrowLeft size={15} /> All sessions
        </Button>
      </div>
      <PageHead title="Compare sessions" sub="See how two recordings differ, for example before and after a driver update, or Medium versus High settings." />

      <Card>
        <div className="row wrap" style={{ gap: '1rem' }}>
          <div style={{ flex: 1, minWidth: '16rem' }}>
            <div className="faint" style={{ fontSize: '0.8125rem', marginBottom: 4 }}>
              First session (A)
            </div>
            <Select label="First session" value={a} options={[{ value: '', label: 'Choose…' }, ...options]} onChange={(v) => choose(v, b)} />
          </div>
          <ArrowRight size={18} aria-hidden style={{ marginTop: 18 }} />
          <div style={{ flex: 1, minWidth: '16rem' }}>
            <div className="faint" style={{ fontSize: '0.8125rem', marginBottom: 4 }}>
              Second session (B)
            </div>
            <Select label="Second session" value={b} options={[{ value: '', label: 'Choose…' }, ...options]} onChange={(v) => choose(a, v)} />
          </div>
        </div>
      </Card>

      {failed && <Banner tone="bad" title="One of the sessions couldn’t be loaded">It may have been deleted.</Banner>}
      {!a || !b ? (
        <Card>
          <Empty title="Choose two sessions above">Tip: in the Sessions list, tick two sessions and press Compare.</Empty>
        </Card>
      ) : !A || !B ? (
        <div className="skeleton" style={{ height: 300 }} />
      ) : (
        <>
          <Card>
            <CardHead title="In short" />
            <ul style={{ margin: 0, paddingLeft: '1.2rem' }}>
              {summarize(rows).map((l) => (
                <li key={l}>{l}</li>
              ))}
            </ul>
            <p className="faint" style={{ marginTop: 10, fontSize: '0.8125rem' }}>
              Differences can come from the game scene, settings, drivers or background programs. Compare like with like for the fairest result.
            </p>
            {hw.length > 0 && (
              <Banner tone="warn" title="These sessions were recorded on different setups" className="">
                <ul style={{ margin: 0, paddingLeft: '1.1rem' }}>
                  {hw.map((h) => (
                    <li key={h}>{h}</li>
                  ))}
                </ul>
              </Banner>
            )}
          </Card>

          <Card style={{ padding: 0, overflow: 'hidden' }}>
            <table className="table">
              <thead>
                <tr>
                  <th>Measurement</th>
                  <th className="r">A · {sessionTitle(A.meta)}</th>
                  <th className="r">B · {sessionTitle(B.meta)}</th>
                  <th className="r">Change</th>
                </tr>
              </thead>
              <tbody>
                {rows
                  .filter((r) => r.a !== null || r.b !== null)
                  .map((r) => {
                    const d = delta(r.a, r.b);
                    const v = verdictOf(r);
                    const Icon = d && d.abs > 0 ? TrendingUp : TrendingDown;
                    return (
                      <tr key={r.id}>
                        <td>{r.label}</td>
                        <td className="r">{fmt(r, r.a, temp)}</td>
                        <td className="r">{fmt(r, r.b, temp)}</td>
                        <td className="r">
                          {d ? (
                            <span style={{ color: v === 'better' ? 'var(--ok)' : v === 'worse' ? 'var(--bad)' : 'var(--text-2)', fontWeight: 600 }}>
                              {d.abs !== 0 && <Icon size={14} aria-hidden style={{ verticalAlign: '-2px', marginRight: 4 }} />}
                              {d.pct !== null ? `${d.pct > 0 ? '+' : ''}${d.pct.toFixed(0)}%` : d.abs === 0 ? '0' : fmt(r, d.abs, temp)}
                              {v && <span className="sr-only"> ({v})</span>}
                            </span>
                          ) : (
                            'N/A'
                          )}
                        </td>
                      </tr>
                    );
                  })}
              </tbody>
            </table>
          </Card>

          <Card>
            <CardHead title="FPS over time" sub="Both sessions aligned from their start" />
            <Overlay A={A} B={B} />
          </Card>
          <Card>
            <CardHead title="Frame-time distribution" sub="The tighter and further left, the smoother" />
            <FrameHistogram
              series={[
                { label: `A · ${sessionTitle(A.meta)}`, color: '#3ddcff', bins: A.hist.bins },
                { label: `B · ${sessionTitle(B.meta)}`, color: '#ffb454', bins: B.hist.bins },
              ]}
            />
          </Card>
        </>
      )}
    </div>
  );
}

function Overlay({ A, B }: { A: SessionData; B: SessionData }) {
  const fa = A.columns['fps'] ?? [];
  const fb = B.columns['fps'] ?? [];
  const n = Math.max(fa.length, fb.length);
  if (n === 0) return <p className="muted">Neither session has FPS data.</p>;
  const xs = Array.from({ length: n }, (_, i) => i);
  const pad = (c: (number | null)[]) => Array.from({ length: n }, (_, i) => c[i] ?? null);
  const series: ChartSeries[] = [
    { id: 'a', label: `A · ${sessionTitle(A.meta)}`, color: '#3ddcff', data: pad(fa), format: (v) => `${v.toFixed(0)} FPS` },
    { id: 'b', label: `B · ${sessionTitle(B.meta)}`, color: '#ffb454', data: pad(fb), format: (v) => `${v.toFixed(0)} FPS` },
  ];
  return <LineChart series={series} xs={xs} xFormat={formatClock} height={220} yMin={0} ariaLabel="FPS of both sessions over time" fill={false} />;
}
