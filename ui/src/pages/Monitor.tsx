import { Pause, Play, Search } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { LineChart, seriesColor, type ChartSeries } from '../components/Charts';
import { Reading, matchesSensor } from '../components/shared';
import { Badge, Banner, Button, Card, CardHead, Chip, Empty, PageHead, SearchBox, Segmented, Tip } from '../components/ui';
import { rpc } from '../lib/bridge';
import { unavailableText } from '../lib/format';
import { useRoute } from '../lib/router';
import { useApp } from '../lib/store';
import type { HistoryData, SensorDesc } from '../lib/types';
import { UNIT_TITLE, displayScale } from '../lib/units';

const VIEWS: { id: string; label: string; ids: string[] }[] = [
  { id: 'performance', label: 'FPS', ids: ['fps', 'fps.low1', 'fps.avg'] },
  { id: 'frametime', label: 'Frame time', ids: ['frametime', 'frametime.max'] },
  { id: 'cpu', label: 'Processor', ids: ['cpu.load', 'cpu.core.max', 'cpu.clock', 'cpu.temp'] },
  { id: 'gpu', label: 'Graphics card', ids: ['gpu.load', 'gpu.vram.percent', 'gpu.clock', 'gpu.temp'] },
  { id: 'temps', label: 'Temperatures', ids: ['cpu.temp', 'gpu.temp', 'gpu.hotspot'] },
  { id: 'memory', label: 'Memory', ids: ['ram.percent', 'gpu.vram.percent', 'ram.used'] },
  { id: 'storage', label: 'Storage', ids: ['disk.active', 'disk.read', 'disk.write', 'disk.latency'] },
  { id: 'network', label: 'Network', ids: ['net.down', 'net.up'] },
  { id: 'power', label: 'Power & fans', ids: ['gpu.power', 'cpu.power', 'gpu.fan'] },
];

const RANGES = [
  { value: 60, label: '1 min' },
  { value: 300, label: '5 min' },
  { value: 900, label: '15 min' },
  { value: 1800, label: '30 min' },
];

export default function MonitorPage() {
  const route = useRoute();
  const catalog = useApp((s) => s.catalogById);
  const all = useApp((s) => s.catalog);
  const [view, setView] = useState(route.query.get('set') ?? 'performance');
  const [custom, setCustom] = useState<string[] | null>(null);
  const [range, setRange] = useState(300);
  const [paused, setPaused] = useState(false);
  const [q, setQ] = useState('');
  const [data, setData] = useState<HistoryData | null>(null);

  useEffect(() => {
    const s = route.query.get('set');
    if (s) {
      setView(s);
      setCustom(null);
    }
  }, [route.query]);

  const viewIds = useMemo(() => (custom ?? VIEWS.find((v) => v.id === view)?.ids ?? []).filter((id) => catalog[id]), [custom, view, catalog]);
  const available = viewIds.filter((id) => catalog[id]?.availability.available);
  const missing = viewIds.filter((id) => !catalog[id]?.availability.available);
  const key = available.join('|');

  useEffect(() => {
    if (!available.length) {
      setData(null);
      return;
    }
    let alive = true;
    const pull = async () => {
      if (paused) return;
      try {
        const d = await rpc<HistoryData>('history.get', { ids: available, seconds: range });
        if (alive) setData(d);
      } catch {
        /* the connection banner covers this */
      }
    };
    void pull();
    const t = setInterval(pull, 1000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [key, range, paused]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggle = (id: string) => {
    const base = custom ?? viewIds;
    setCustom(base.includes(id) ? base.filter((x) => x !== id) : [...base, id]);
  };

  const list = useMemo(() => all.filter((d) => d.unit !== 'text' && matchesSensor(d, q)).slice(0, 150), [all, q]);

  return (
    <div className="page wide" style={{ maxWidth: '90rem' }}>
      <PageHead
        title="Live graphs"
        sub="Watch how your hardware behaves over time. Pick a view, or tick any measurement on the right to add it."
        actions={
          <>
            <Segmented label="Time range" value={range} options={RANGES} onChange={setRange} />
            <Button onClick={() => setPaused(!paused)} aria-pressed={paused}>
              {paused ? <Play size={15} /> : <Pause size={15} />} {paused ? 'Resume' : 'Pause'}
            </Button>
          </>
        }
      />
      <div className="row wrap" role="group" aria-label="Quick views">
        {VIEWS.map((v) => (
          <Chip
            key={v.id}
            on={!custom && view === v.id}
            onClick={() => {
              setView(v.id);
              setCustom(null);
            }}
          >
            {v.label}
          </Chip>
        ))}
        {custom && <Badge tone="accent">Custom selection</Badge>}
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) 20rem', gap: '1rem', alignItems: 'start' }} className="monitor-grid">
        <div className="stack">
          {paused && <Banner tone="info">Paused. The graphs are frozen. Press Resume to continue.</Banner>}
          {available.length === 0 ? (
            <Card>
              <Empty icon={<Search size={32} />} title="Nothing to draw yet">
                {missing.length > 0 ? 'None of the measurements in this view are available on your PC. See below for why, or pick something else.' : 'Tick a measurement on the right to draw it.'}
              </Empty>
            </Card>
          ) : (
            <Charts ids={available} data={data} range={range} />
          )}
          {missing.length > 0 && (
            <Card className="tight">
              <div style={{ fontWeight: 650, marginBottom: 6 }}>Not available on this PC</div>
              <div className="stack" style={{ gap: 4 }}>
                {missing.map((id) => (
                  <div key={id} className="muted" style={{ fontSize: '0.875rem' }}>
                    <b style={{ color: 'var(--text)' }}>{catalog[id].name}</b>: {unavailableText(catalog[id])}
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>

        <Card style={{ position: 'sticky', top: 0 }}>
          <CardHead title="All measurements" sub={`${all.filter((d) => d.availability.available).length} available on this PC`} />
          <SearchBox value={q} onChange={setQ} placeholder="Try “temp”, “fan” or “memory”" label="Search measurements" />
          <div style={{ maxHeight: '60vh', overflowY: 'auto', marginTop: '0.6rem', display: 'flex', flexDirection: 'column', gap: 2 }}>
            {list.map((d) => (
              <SensorRow key={d.id} d={d} on={viewIds.includes(d.id)} onToggle={() => toggle(d.id)} />
            ))}
            {list.length === 0 && <p className="muted">Nothing matches “{q}”.</p>}
          </div>
        </Card>
      </div>
      <style>{`@media (max-width: 1100px){.monitor-grid{grid-template-columns:1fr !important}}`}</style>
    </div>
  );
}

function SensorRow({ d, on, onToggle }: { d: SensorDesc; on: boolean; onToggle: () => void }) {
  const ok = d.availability.available;
  return (
    <label className="layer" style={{ opacity: ok ? 1 : 0.65, cursor: 'pointer' }}>
      <input type="checkbox" checked={on} onChange={onToggle} aria-label={`Graph ${d.name}`} />
      <span className="nm" title={d.description}>
        {d.name}
        {d.group && <span className="faint"> · {d.group}</span>}
      </span>
      {ok ? (
        <b className="num" style={{ fontSize: '0.8125rem' }}>
          <Reading id={d.id} />
        </b>
      ) : (
        <Tip text={unavailableText(d)}>
          <Badge tone="warn">n/a</Badge>
        </Tip>
      )}
    </label>
  );
}

function Charts({ ids, data, range }: { ids: string[]; data: HistoryData | null; range: number }) {
  const catalog = useApp((s) => s.catalogById);
  const settings = useApp((s) => s.settings);
  const groups = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const id of ids) {
      const u = catalog[id].unit;
      m.set(u, [...(m.get(u) ?? []), id]);
    }
    return [...m.entries()];
  }, [ids, catalog]);
  if (!data) return <div className="skeleton" style={{ height: 260 }} />;
  const n = Math.max(0, ...Object.values(data.series).map((s) => s.length));
  const xs = Array.from({ length: n }, (_, i) => data.endMs - (n - 1 - i) * data.intervalMs);
  const end = data.endMs;
  const xFormat = (x: number) => {
    const s = Math.round((x - end) / 1000);
    if (s === 0) return 'now';
    return range > 120 ? `−${Math.round(-s / 60)} min` : `−${-s} s`;
  };
  return (
    <>
      {groups.map(([unit, gids]) => {
        const raw = gids.flatMap((id) => data.series[id] ?? []).filter((v): v is number => v !== null);
        const max = raw.length ? Math.max(...raw) : 0;
        const sc = displayScale(catalog[gids[0]].unit, settings, max);
        const series: ChartSeries[] = gids.map((id, i) => ({
          id,
          label: catalog[id].name,
          color: seriesColor(i),
          data: (data.series[id] ?? []).map((v) => (v === null ? null : sc.map(v))),
          format: (v) => `${v.toFixed(sc.decimals)} ${sc.label}`.trim(),
        }));
        const pct = unit === 'percent';
        return (
          <Card key={unit}>
            <CardHead title={`${UNIT_TITLE[unit] ?? 'Value'}${sc.label ? ` (${sc.label})` : ''}`} />
            <LineChart
              series={series}
              xs={xs}
              xFormat={xFormat}
              height={220}
              yMin={pct || unit === 'fps' || unit === 'ms' || unit === 'rpm' || unit === 'watt' ? 0 : undefined}
              yMax={pct ? 100 : undefined}
              yFormat={(v) => String(Math.round(v * 10) / 10)}
              ariaLabel={`${UNIT_TITLE[unit] ?? 'Value'} over the last ${range >= 120 ? Math.round(range / 60) + ' minutes' : range + ' seconds'}`}
            />
          </Card>
        );
      })}
    </>
  );
}
