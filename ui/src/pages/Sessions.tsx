import { GitCompareArrows, History, Star, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Badge, Button, Card, Chip, Empty, PageHead, SearchBox } from '../components/ui';
import { formatDateTime, formatDuration, relativeTime } from '../lib/format';
import { VERDICT_LABEL } from '../lib/labels';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { SessionListItem } from '../lib/types';

export function sessionTitle(m: SessionListItem['meta']): string {
  return m.name || m.gameName || (m.kind === 'manual' ? 'Manual recording' : 'Desktop recording');
}

export default function SessionsPage() {
  const version = useApp((s) => s.sessionsVersion);
  const toast = useApp((s) => s.toast);
  const [items, setItems] = useState<SessionListItem[] | null>(null);
  const [q, setQ] = useState('');
  const [starred, setStarred] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void act<SessionListItem[]>('sessions.list').then((l) => alive && setItems(l ?? []));
    return () => {
      alive = false;
    };
  }, [version]);

  const shown = useMemo(() => (items ?? []).filter((i) => (!starred || i.meta.starred) && (!q.trim() || `${sessionTitle(i.meta)} ${i.meta.exe ?? ''}`.toLowerCase().includes(q.toLowerCase()))), [items, q, starred]);

  const togglePick = (id: string) => setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p.slice(-1), id]));

  const remove = async (i: SessionListItem) => {
    const r = await act('sessions.delete', { id: i.meta.id });
    if (r === undefined) return;
    setPicked((p) => p.filter((x) => x !== i.meta.id));
    toast({ level: 'info', message: `Deleted “${sessionTitle(i.meta)}”` }, async () => {
      await act('sessions.restore', { id: i.meta.id });
    });
  };

  return (
    <div className="page">
      <PageHead
        title="Sessions"
        sub="Every game you play is recorded automatically. Open one to see how it ran, what limited it, and where it stuttered."
        actions={
          <Button variant="primary" disabled={picked.length !== 2} onClick={() => navigate(`/compare?a=${picked[0]}&b=${picked[1]}`)} title={picked.length === 2 ? 'Compare the two selected sessions' : 'Tick two sessions to compare them'}>
            <GitCompareArrows size={16} /> Compare {picked.length === 2 ? 'selected' : `(${picked.length}/2)`}
          </Button>
        }
      />
      <div className="row wrap">
        <SearchBox value={q} onChange={setQ} placeholder="Search by game" label="Search sessions" />
        <Chip on={starred} onClick={() => setStarred(!starred)}>
          <Star size={13} aria-hidden /> Starred only
        </Chip>
      </div>

      {items === null ? (
        <div className="skeleton" style={{ height: 200 }} />
      ) : shown.length === 0 ? (
        <Card>
          <Empty icon={<History size={36} />} title={items.length === 0 ? 'No sessions yet' : 'No sessions match'}>
            {items.length === 0
              ? 'Play a game and FPSFramer records it for you. You can also record by hand with the Record button on the Home page. Sessions shorter than 30 seconds are discarded.'
              : 'Try a different search.'}
          </Empty>
        </Card>
      ) : (
        <Card style={{ padding: 0, overflow: 'hidden' }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: '2.5rem' }}>
                  <span className="sr-only">Select to compare</span>
                </th>
                <th>Session</th>
                <th className="r">Length</th>
                <th className="r">Avg FPS</th>
                <th className="r">1% Low</th>
                <th className="r">Spikes</th>
                <th>Verdict</th>
                <th style={{ width: '5rem' }}>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((i) => (
                <tr key={i.meta.id} className="click" onClick={() => navigate(`/sessions/${i.meta.id}`)}>
                  <td onClick={(e) => e.stopPropagation()}>
                    <input type="checkbox" checked={picked.includes(i.meta.id)} onChange={() => togglePick(i.meta.id)} aria-label={`Select ${sessionTitle(i.meta)} to compare`} />
                  </td>
                  <td>
                    <div className="row" style={{ gap: '0.5rem' }}>
                      {i.meta.starred && <Star size={14} fill="var(--warn)" color="var(--warn)" aria-label="Starred" />}
                      <div>
                        <a
                          href={`#/sessions/${i.meta.id}`}
                          style={{ fontWeight: 650, color: 'var(--text)', textDecoration: 'none' }}
                          onClick={(e) => {
                            e.preventDefault();
                            navigate(`/sessions/${i.meta.id}`);
                          }}
                        >
                          {sessionTitle(i.meta)}
                        </a>
                        <div className="faint" style={{ fontSize: '0.8125rem' }} title={formatDateTime(i.meta.startedAt)}>
                          {relativeTime(i.meta.startedAt)} · {formatDateTime(i.meta.startedAt)}
                          {i.meta.api && ` · ${i.meta.api}`}
                        </div>
                      </div>
                      {i.meta.demo && <Badge tone="warn">Demo</Badge>}
                    </div>
                  </td>
                  <td className="r">{formatDuration(i.meta.durationS)}</td>
                  <td className="r">{i.fpsAvg !== null ? Math.round(i.fpsAvg) : 'N/A'}</td>
                  <td className="r">{i.low1 !== null ? Math.round(i.low1) : 'N/A'}</td>
                  <td className="r">{i.frames > 0 ? i.spikeCount : 'N/A'}</td>
                  <td>
                    <div className="row wrap" style={{ gap: '0.3rem' }}>
                      {i.verdict && i.verdict !== 'unknown' && <Badge tone="accent">{VERDICT_LABEL[i.verdict]}</Badge>}
                      {i.warnings > 0 && <Badge tone="warn">{i.warnings} to look at</Badge>}
                    </div>
                  </td>
                  <td onClick={(e) => e.stopPropagation()}>
                    <Button size="sm" variant="ghost" icon aria-label={`Delete ${sessionTitle(i.meta)}`} onClick={() => void remove(i)}>
                      <Trash2 size={15} />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}
