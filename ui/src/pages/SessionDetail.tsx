import { AlertTriangle, ArrowLeft, CheckCircle2, Download, GitCompareArrows, Info, OctagonAlert, Pencil, Star, Trash2, ZoomOut } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { FrameHistogram, LineChart, seriesColor, type ChartMarker, type ChartSeries } from '../components/Charts';
import { Badge, Banner, Button, Card, CardHead, Empty, InfoTip, PageHead, Tip } from '../components/ui';
import { saveTextFile } from '../lib/files';
import { decimate } from '../lib/decimate';
import { formatClock, formatDateTime, formatDuration } from '../lib/format';
import { VERDICT_HELP, VERDICT_LABEL } from '../lib/labels';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { Finding, SessionData } from '../lib/types';
import { displayScale, UNIT_TITLE } from '../lib/units';
import { sessionTitle } from './Sessions';

const NAMES: Record<string, string> = {
  fps: 'FPS',
  frametime: 'Frame time (average)',
  'frametime.max': 'Slowest frame',
  'cpu.load': 'CPU usage',
  'cpu.core.max': 'Busiest CPU core',
  'gpu.load': 'GPU usage',
  'cpu.temp': 'CPU temperature',
  'gpu.temp': 'GPU temperature',
  'gpu.hotspot': 'GPU hot spot',
  'cpu.clock': 'CPU speed',
  'gpu.clock': 'GPU speed',
  'ram.percent': 'Memory use',
  'gpu.vram.percent': 'Video memory use',
  'cpu.power': 'CPU power',
  'gpu.power': 'GPU power',
};

const CHARTS: { id: string; title: string; unit: string; ids: string[]; yMax?: number; markers?: boolean }[] = [
  { id: 'fps', title: 'Frame rate', unit: 'fps', ids: ['fps'], markers: true },
  { id: 'ft', title: 'Frame time', unit: 'ms', ids: ['frametime', 'frametime.max'], markers: true },
  { id: 'load', title: 'Usage', unit: 'percent', ids: ['gpu.load', 'cpu.load', 'cpu.core.max'], yMax: 100 },
  { id: 'temp', title: 'Temperature', unit: 'celsius', ids: ['gpu.temp', 'cpu.temp', 'gpu.hotspot'] },
  { id: 'clock', title: 'Clock speed', unit: 'mhz', ids: ['gpu.clock', 'cpu.clock'] },
  { id: 'mem', title: 'Memory use', unit: 'percent', ids: ['ram.percent', 'gpu.vram.percent'], yMax: 100 },
  { id: 'power', title: 'Power', unit: 'watt', ids: ['gpu.power', 'cpu.power'] },
];

export default function SessionDetailPage({ id }: { id: string }) {
  const version = useApp((s) => s.sessionsVersion);
  const [data, setData] = useState<SessionData | null | 'missing'>(null);
  useEffect(() => {
    let alive = true;
    setData(null);
    void act<SessionData>('sessions.get', { id }).then((d) => alive && setData(d ?? 'missing'));
    return () => {
      alive = false;
    };
  }, [id, version]);

  if (data === null) {
    return (
      <div className="page">
        <div className="skeleton" style={{ height: 48, width: 320 }} />
        <div className="skeleton" style={{ height: 160 }} />
        <div className="skeleton" style={{ height: 300 }} />
      </div>
    );
  }
  if (data === 'missing') {
    return (
      <div className="page">
        <Empty title="This session couldn’t be found" action={<Button variant="primary" onClick={() => navigate('/sessions')}>Back to sessions</Button>}>
          It may have been deleted.
        </Empty>
      </div>
    );
  }
  return <Detail d={data} />;
}

function Detail({ d }: { d: SessionData }) {
  const toast = useApp((s) => s.toast);
  const [zoom, setZoom] = useState<[number, number] | null>(null);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(d.meta.name);
  const [notes, setNotes] = useState(d.meta.notes);
  const s = d.summary;
  const hasFrames = s.frames > 0;

  const rename = async () => {
    setEditing(false);
    if (name.trim() !== d.meta.name) await act('sessions.update', { id: d.meta.id, name: name.trim() });
  };

  const exportAs = async (format: 'csv' | 'json') => {
    const r = await act<{ filename: string; text: string; mime: string }>('sessions.export', { id: d.meta.id, format });
    if (r && (await saveTextFile(r.filename, r.text, r.mime))) toast({ level: 'success', message: `Exported ${r.filename}` });
  };

  const remove = async () => {
    const r = await act('sessions.delete', { id: d.meta.id });
    if (r === undefined) return;
    navigate('/sessions');
    toast({ level: 'info', message: `Deleted “${sessionTitle(d.meta)}”` }, async () => {
      await act('sessions.restore', { id: d.meta.id });
    });
  };

  const markers = useMemo(() => {
    const m: ChartMarker[] = [];
    d.spikes.forEach((x) => m.push({ x: x.tS, label: `Spike ${x.frameMs.toFixed(0)} ms`, color: '#ffb454' }));
    d.drops.forEach((x) => m.push({ x: x.startS, label: 'FPS drop', color: '#ff6b6b' }));
    return m;
  }, [d]);

  return (
    <div className="page" style={{ maxWidth: '80rem' }}>
      <div>
        <Button variant="ghost" size="sm" onClick={() => navigate('/sessions')}>
          <ArrowLeft size={15} /> All sessions
        </Button>
      </div>
      <PageHead
        title={
          editing ? (
            <input className="input" style={{ fontSize: '1.4rem', fontWeight: 700, maxWidth: '30rem' }} autoFocus value={name} maxLength={120} aria-label="Session name" placeholder={sessionTitle({ ...d.meta, name: '' })} onChange={(e) => setName(e.target.value)} onBlur={() => void rename()} onKeyDown={(e) => e.key === 'Enter' && void rename()} />
          ) : (
            <span className="row" style={{ gap: '0.6rem' }}>
              {sessionTitle(d.meta)}
              <Button variant="ghost" size="sm" icon aria-label="Rename session" onClick={() => setEditing(true)}>
                <Pencil size={15} />
              </Button>
            </span>
          )
        }
        sub={
          <>
            {formatDateTime(d.meta.startedAt)} · {formatDuration(d.meta.durationS)}
            {d.meta.api && ` · ${d.meta.api}`}
            {d.meta.profileName && ` · profile “${d.meta.profileName}”`}
          </>
        }
        actions={
          <>
            <Button onClick={() => void act('sessions.update', { id: d.meta.id, starred: !d.meta.starred })} aria-pressed={d.meta.starred}>
              <Star size={15} fill={d.meta.starred ? 'var(--warn)' : 'none'} color={d.meta.starred ? 'var(--warn)' : 'currentColor'} /> {d.meta.starred ? 'Starred' : 'Star'}
            </Button>
            <Button onClick={() => navigate('/sessions')} title="Pick two sessions in the list, then press Compare">
              <GitCompareArrows size={15} /> Compare…
            </Button>
            <Button onClick={() => void exportAs('csv')} title="Per-second table that opens in Excel or Google Sheets">
              <Download size={15} /> CSV
            </Button>
            <Button onClick={() => void exportAs('json')} title="Complete recording including the frame-time histogram">
              <Download size={15} /> JSON
            </Button>
            <Button variant="danger" onClick={() => void remove()}>
              <Trash2 size={15} /> Delete
            </Button>
          </>
        }
      />
      {d.meta.demo && (
        <Banner tone="warn" title="Demo recording">
          This session was recorded in demo mode. The numbers are simulated, not measured on a real game.
        </Banner>
      )}

      {hasFrames ? <Stats d={d} /> : <Banner tone="info" title="No FPS data in this recording">This was a hardware-only recording (no game was being measured), so there are no FPS statistics. The graphs below show your hardware over time.</Banner>}

      <Card>
        <CardHead title="What happened" sub="Plain-language findings from the recorded data" right={<InfoTip text="“Measured” findings are read straight from sensors. “Likely” findings are estimates from several readings, usually right but not a guaranteed diagnosis. “Possible” means worth checking." />} />
        <Findings list={s.findings} />
      </Card>

      {s.bottleneck && s.bottleneck.verdict !== 'unknown' && <Bottleneck d={d} />}

      <Card>
        <CardHead
          title="Over time"
          sub="Drag across a graph to zoom in. Orange lines are frame-time spikes, red lines are FPS drops."
          right={
            zoom && (
              <Button size="sm" onClick={() => setZoom(null)}>
                <ZoomOut size={15} /> Reset zoom
              </Button>
            )
          }
        />
        <div className="stack lg">
          {CHARTS.map((c) => (
            <SessionChart key={c.id} cfg={c} d={d} zoom={zoom} setZoom={setZoom} markers={c.markers ? markers : []} />
          ))}
        </div>
      </Card>

      {hasFrames && (
        <Card>
          <CardHead title="Frame-time distribution" sub="Most frames should sit in one tall, narrow peak on the left. A long tail to the right is stutter." right={<InfoTip text="Each bar shows how many frames took that long to draw. Height uses a square-root scale so rare slow frames stay visible." />} />
          <FrameHistogram series={[{ label: 'This session', color: 'var(--accent)', bins: d.hist.bins }]} />
        </Card>
      )}

      <div className="grid cols-2">
        <Card>
          <CardHead title={`Frame-time spikes (${d.spikes.length})`} sub="Single frames that took far longer than usual" />
          {d.spikes.length === 0 ? (
            <p className="muted">No spikes over 25 ms were detected.</p>
          ) : (
            <EventTable
              rows={[...d.spikes].sort((a, b) => b.frameMs - a.frameMs).slice(0, 25).map((x) => ({ key: `${x.tS}`, t: x.tS, a: `${x.frameMs.toFixed(0)} ms`, b: `usually ${x.baselineMs.toFixed(0)} ms`, hint: x.frames > 1 ? `${x.frames} slow frames in a row` : null }))}
              head={['When', 'Slowest frame', 'Normally']}
              onPick={(t) => setZoom([Math.max(0, t - 15), t + 15])}
            />
          )}
        </Card>
        <Card>
          <CardHead title={`FPS drops (${d.drops.length})`} sub="Stretches where FPS fell well below its recent level" />
          {d.drops.length === 0 ? (
            <p className="muted">No significant FPS drops were detected.</p>
          ) : (
            <EventTable
              rows={d.drops.map((x) => ({ key: `${x.startS}`, t: x.startS, a: `${x.minFps.toFixed(0)} FPS`, b: `from ${x.baselineFps.toFixed(0)}, ${x.durationS.toFixed(0)} s`, hint: x.hint }))}
              head={['Started', 'Lowest', 'Details']}
              onPick={(t) => setZoom([Math.max(0, t - 10), t + 25])}
            />
          )}
        </Card>
      </div>

      {d.alerts.length > 0 && (
        <Card>
          <CardHead title="Alerts during this session" />
          <table className="table">
            <tbody>
              {d.alerts.map((a, i) => (
                <tr key={i}>
                  <td style={{ width: '5rem' }} className="num">{formatClock(a.tS)}</td>
                  <td>
                    <b>{a.name}</b> <span className="muted">{a.kind === 'cleared' ? 'back to normal' : a.message}</span>
                  </td>
                  <td>{a.kind === 'fired' && <Badge tone={a.severity === 'critical' ? 'bad' : 'warn'}>{a.severity}</Badge>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <div className="grid cols-2">
        <Card>
          <CardHead title="This PC at the time" />
          <dl className="kv">
            <dt>Processor</dt>
            <dd>{d.meta.hardware.cpu || 'N/A'}</dd>
            <dt>Graphics</dt>
            <dd>{d.meta.hardware.gpus.join(', ') || 'N/A'}</dd>
            <dt>Memory</dt>
            <dd>{d.meta.hardware.ramGb ? `${d.meta.hardware.ramGb} GB` : 'N/A'}</dd>
            <dt>Windows</dt>
            <dd>{d.meta.hardware.os || 'N/A'}</dd>
            <dt>Displays</dt>
            <dd>{d.meta.hardware.displays.join('; ') || 'N/A'}</dd>
            <dt>Program</dt>
            <dd>{d.meta.exe ?? 'N/A'}</dd>
            <dt>FPSFramer</dt>
            <dd>{d.meta.hardware.appVersion || 'N/A'}</dd>
          </dl>
        </Card>
        <Card>
          <CardHead title="Your notes" sub="Settings you used, what you were testing… saved with the session." />
          <textarea className="textarea" aria-label="Notes" value={notes} maxLength={4000} placeholder="For example: Ultra settings, DLSS Quality, new driver." onChange={(e) => setNotes(e.target.value)} onBlur={() => notes !== d.meta.notes && void act('sessions.update', { id: d.meta.id, notes })} />
        </Card>
      </div>
    </div>
  );
}

// ───────────────────────────── stats ─────────────────────────────

function consistencyBand(c: number | null): { label: string; tone: 'ok' | 'warn' | 'bad' | undefined; help: string } {
  if (c === null) return { label: 'N/A', tone: undefined, help: 'Needs about 100 frames of data.' };
  if (c >= 0.8) return { label: 'Very smooth', tone: 'ok', help: 'Your 1% Low is at least 80% of your average, so frames arrived evenly.' };
  if (c >= 0.6) return { label: 'Mostly smooth', tone: undefined, help: 'Some dips, but nothing severe.' };
  return { label: 'Uneven', tone: 'bad', help: 'Your 1% Low is under 60% of your average, so you probably felt stutter.' };
}

function Stats({ d }: { d: SessionData }) {
  const s = d.summary;
  const n = (v: number | null, dec = 0) => (v === null ? 'N/A' : v.toFixed(dec));
  const band = consistencyBand(s.consistency);
  const items: { label: string; value: string; unit?: string; tip: string; badge?: React.ReactNode }[] = [
    { label: 'Average FPS', value: n(s.fpsAvg), tip: 'Frames per second averaged over the whole session.' },
    { label: '1% Low', value: n(s.low1), tip: 'Average FPS of the slowest 1% of frames, which shows how bad the dips were.' },
    { label: '0.1% Low', value: n(s.low01), tip: 'Average FPS of the slowest 0.1% of frames. Needs about 1,000 frames.' },
    { label: 'Slowest frame', value: n(s.worstMs), unit: 'ms', tip: 'The longest a single frame took to draw.' },
    { label: 'Consistency', value: s.consistency === null ? 'N/A' : `${(s.consistency * 100).toFixed(0)}%`, tip: `1% Low divided by average FPS. ${band.help}`, badge: <Badge tone={band.tone}>{band.label}</Badge> },
    { label: 'Spikes', value: String(s.spikeCount), tip: 'Frames that took over 25 ms and at least 2.5× longer than usual.' },
    { label: 'Minimum', value: n(s.fpsMin), tip: 'FPS of the single slowest frame.' },
    { label: 'Maximum', value: n(s.fpsMax), tip: 'FPS of the single fastest frame.' },
  ];
  if (s.targetFps && s.belowTargetPct !== null) items.push({ label: `Below ${s.targetFps} FPS target`, value: s.belowTargetPct.toFixed(0), unit: '% of the time', tip: 'Share of seconds where FPS was more than 5% under your target.' });
  return (
    <div className="grid cols-4">
      {items.map((i) => (
        <Card key={i.label} className="tight">
          <div className="faint row" style={{ fontSize: '0.8125rem', gap: 4 }}>
            {i.label} <InfoTip text={i.tip} />
          </div>
          <div className="num" style={{ fontSize: '1.9rem', fontWeight: 700, letterSpacing: '-0.02em' }}>
            {i.value}
            {i.unit && <span className="muted" style={{ fontSize: '0.9rem', fontWeight: 600 }}> {i.unit}</span>}
          </div>
          {i.badge}
        </Card>
      ))}
    </div>
  );
}

const CERT: Record<Finding['certainty'], { label: string; tip: string }> = {
  measured: { label: 'Measured', tip: 'Read directly from a sensor.' },
  likely: { label: 'Likely', tip: 'An estimate from several readings, usually right but not a guaranteed diagnosis.' },
  possible: { label: 'Possible', tip: 'A possibility worth checking.' },
};

function Findings({ list }: { list: Finding[] }) {
  if (list.length === 0) return <p className="muted">Nothing to report for this recording.</p>;
  return (
    <div className="stack">
      {list.map((f) => {
        const Icon = f.level === 'good' ? CheckCircle2 : f.level === 'warning' ? OctagonAlert : f.level === 'notice' ? AlertTriangle : Info;
        const color = f.level === 'good' ? 'var(--ok)' : f.level === 'warning' ? 'var(--bad)' : f.level === 'notice' ? 'var(--warn)' : 'var(--info)';
        return (
          <div key={f.id} className="row top" style={{ gap: '0.8rem', padding: '0.85rem 1rem', border: '1px solid var(--border)', borderRadius: 14, background: 'var(--surface-2)' }}>
            <Icon size={22} color={color} aria-hidden style={{ flex: 'none', marginTop: 2 }} />
            <div style={{ minWidth: 0 }}>
              <div className="row wrap" style={{ gap: '0.5rem' }}>
                <b>{f.title}</b>
                <Tip text={CERT[f.certainty].tip}>
                  <Badge tone={f.certainty === 'measured' ? 'ok' : undefined}>{CERT[f.certainty].label}</Badge>
                </Tip>
              </div>
              <p className="muted" style={{ marginTop: 4 }}>{f.detail}</p>
              {f.evidence.length > 0 && (
                <ul className="faint" style={{ margin: '0.4rem 0 0', paddingLeft: '1.1rem', fontSize: '0.8125rem' }}>
                  {f.evidence.map((e) => (
                    <li key={e}>{e}</li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Bottleneck({ d }: { d: SessionData }) {
  const b = d.summary.bottleneck!;
  const rows: { label: string; v: number | null; tip: string }[] = [
    { label: 'Graphics card at its limit (≥95%)', v: b.gpuBoundPct, tip: 'Share of the time the GPU was 95% busy or more.' },
    { label: 'Processor limiting (GPU had room)', v: b.cpuBoundPct, tip: 'Share of the time a CPU core was ≥85% busy while the GPU was under 85%.' },
    { label: 'Video memory nearly full', v: b.vramPressurePct, tip: 'Time VRAM was 95% full or more.' },
    { label: 'System memory nearly full', v: b.ramPressurePct, tip: 'Time RAM was 92% full or more.' },
    { label: 'Running hot', v: b.hotPct, tip: 'Time CPU was over 90 °C or GPU over 88 °C.' },
    { label: 'Graphics driver slowing the card', v: b.throttlePct, tip: 'Time the driver reported a thermal or power slowdown (NVIDIA only).' },
  ];
  return (
    <Card>
      <CardHead title="What limited performance?" sub="An estimate from CPU and GPU usage, not a confirmed diagnosis" />
      <div className="row wrap" style={{ gap: '0.6rem', marginBottom: '0.6rem' }}>
        <Badge tone="accent">{VERDICT_LABEL[b.verdict]}</Badge>
        <Badge>Confidence: {b.confidence}</Badge>
        {b.capFps && <Badge>Capped near {b.capFps} FPS</Badge>}
        <span className="faint">based on {formatDuration(b.analyzedSeconds)} of play</span>
      </div>
      <p className="muted" style={{ marginBottom: '0.9rem' }}>{VERDICT_HELP[b.verdict]}</p>
      <div className="stack" style={{ gap: '0.6rem' }}>
        {rows.filter((r) => r.v !== null).map((r) => (
          <div key={r.label}>
            <div className="row" style={{ fontSize: '0.875rem' }}>
              <span>{r.label}</span> <InfoTip text={r.tip} />
              <span className="spacer" />
              <b className="num">{r.v!.toFixed(0)}%</b>
            </div>
            <div className="bar" role="img" aria-label={`${r.label}: ${r.v!.toFixed(0)} percent of the time`}>
              <i style={{ width: `${r.v}%` }} />
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

function EventTable({ rows, head, onPick }: { rows: { key: string; t: number; a: string; b: string; hint: string | null }[]; head: string[]; onPick: (t: number) => void }) {
  return (
    <div style={{ maxHeight: 320, overflowY: 'auto' }}>
      <table className="table">
        <thead>
          <tr>
            {head.map((h) => (
              <th key={h}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.key} className="click" tabIndex={0} onClick={() => onPick(r.t)} onKeyDown={(e) => e.key === 'Enter' && onPick(r.t)} title="Zoom the graphs to this moment">
              <td className="num">{formatClock(r.t)}</td>
              <td><b>{r.a}</b></td>
              <td>
                <span className="muted">{r.b}</span>
                {r.hint && <div className="faint" style={{ fontSize: '0.8125rem' }}>{r.hint}</div>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SessionChart({ cfg, d, zoom, setZoom, markers }: { cfg: (typeof CHARTS)[number]; d: SessionData; zoom: [number, number] | null; setZoom: (z: [number, number]) => void; markers: ChartMarker[] }) {
  const settings = useApp((s) => s.settings);
  const ids = cfg.ids.filter((id) => d.columns[id]?.some((v) => v !== null));
  const n = Math.max(0, ...ids.map((id) => d.columns[id].length));
  const range = useMemo<[number, number]>(() => (zoom ? [Math.max(0, Math.floor(zoom[0])), Math.min(n - 1, Math.ceil(zoom[1]))] : [0, n - 1]), [zoom, n]);
  const built = useMemo(() => {
    if (!ids.length) return null;
    const xs = Array.from({ length: range[1] - range[0] + 1 }, (_, i) => range[0] + i);
    const raw = ids.map((id) => ({ data: d.columns[id].slice(range[0], range[1] + 1), peak: id.endsWith('.max') }));
    const dec = decimate(xs, raw, 1400);
    const all = dec.series.flat().filter((v): v is number => v !== null);
    const sc = displayScale(cfg.unit as never, settings, all.length ? Math.max(...all) : 0);
    const series: ChartSeries[] = ids.map((id, i) => ({ id, label: NAMES[id] ?? id, color: seriesColor(i), data: dec.series[i].map((v) => (v === null ? null : sc.map(v))), format: (v) => `${v.toFixed(sc.decimals)} ${sc.label}`.trim() }));
    return { xs: dec.xs, series, sc };
  }, [ids.join('|'), range, d, settings.units.temperature, settings.units.network]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!built) return null;
  const target = cfg.id === 'fps' ? d.summary.targetFps : null;
  return (
    <div>
      <div style={{ fontWeight: 650, marginBottom: 4 }}>
        {cfg.title} <span className="faint">({built.sc.label || UNIT_TITLE[cfg.unit]})</span>
      </div>
      <LineChart
        series={built.series}
        xs={built.xs}
        xFormat={formatClock}
        height={180}
        yMin={cfg.unit === 'percent' || cfg.unit === 'fps' || cfg.unit === 'ms' || cfg.unit === 'watt' ? 0 : undefined}
        yMax={cfg.yMax}
        refLines={target ? [{ y: target, label: `target ${target}`, color: 'var(--warn)' }] : []}
        markers={markers}
        onBrush={(a, b) => setZoom([a, b])}
        ariaLabel={`${cfg.title} during the session`}
      />
    </div>
  );
}
