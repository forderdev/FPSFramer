import { Camera, ChevronLeft, ChevronRight, FolderOpen, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Banner, Button, Card, Chip, Empty, Kbd, Modal, PageHead } from '../components/ui';
import { rpc } from '../lib/bridge';
import { formatBytes, formatDateTime } from '../lib/format';
import { act, useApp } from '../lib/store';

interface Shot {
  id: string;
  game: string;
  takenAt: number;
  size: number;
}
interface ShotList {
  folder: string;
  items: Shot[];
}

export default function CapturesPage() {
  const version = useApp((s) => s.screenshotsVersion);
  const hotkey = useApp((s) => s.settings.hotkeys.screenshot);
  const toast = useApp((s) => s.toast);
  const [data, setData] = useState<ShotList | null>(null);
  const [error, setError] = useState(false);
  const [game, setGame] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    rpc<ShotList>('screenshots.list')
      .then((d) => alive && (setData(d), setError(false)))
      .catch(() => alive && setError(true));
    return () => {
      alive = false;
    };
  }, [version]);

  const games = Array.from(new Set((data?.items ?? []).map((s) => s.game))).sort();
  const items = (data?.items ?? []).filter((s) => !game || s.game === game);

  const remove = async (s: Shot) => {
    const r = await act('screenshots.delete', { id: s.id });
    if (r !== undefined) toast({ level: 'info', message: 'Screenshot deleted' });
  };

  const step = useCallback(
    (delta: number) => {
      if (!open) return;
      const i = items.findIndex((s) => s.id === open);
      const n = items[i + delta];
      if (n) setOpen(n.id);
    },
    [open, items],
  );

  return (
    <div className="page">
      <PageHead
        title="Screenshots"
        sub={
          <>
            Pictures of your screen, saved with the game’s name.{' '}
            {hotkey ? (
              <>
                Press <Kbd>{hotkey}</Kbd> while playing to take one.
              </>
            ) : (
              'Set a screenshot hotkey in Settings → Hotkeys to take them while playing.'
            )}
          </>
        }
        actions={
          <>
            <Button onClick={() => void act('screenshots.open')} title={data?.folder}>
              <FolderOpen size={16} /> Open folder
            </Button>
            <Button variant="primary" onClick={() => void act('screenshots.take')}>
              <Camera size={16} /> Take one now
            </Button>
          </>
        }
      />

      {error && <Banner tone="bad" title="The screenshots couldn’t be listed">Check that the screenshot folder exists and is writable (Settings → General).</Banner>}
      <Banner tone="info">
        Screenshots capture what is on your screen, including the overlay. In some full-screen modes Windows may return a black picture. Borderless windowed mode works best.
      </Banner>

      {games.length > 1 && (
        <div className="row wrap" role="group" aria-label="Filter by game">
          <Chip on={game === null} onClick={() => setGame(null)}>
            All games
          </Chip>
          {games.map((g) => (
            <Chip key={g} on={game === g} onClick={() => setGame(g)}>
              {g}
            </Chip>
          ))}
        </div>
      )}

      {data === null && !error ? (
        <div className="skeleton" style={{ height: 220 }} />
      ) : items.length === 0 ? (
        <Card>
          <Empty icon={<Camera size={36} />} title="No screenshots yet">
            Take one with the button above or with your hotkey. They are saved in {data?.folder ?? 'your Pictures folder'}.
          </Empty>
        </Card>
      ) : (
        <div className="grid cols-3" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(15rem, 1fr))' }}>
          {items.map((s) => (
            <Thumb key={s.id} shot={s} onOpen={() => setOpen(s.id)} onDelete={() => void remove(s)} />
          ))}
        </div>
      )}

      {open && (
        <Viewer
          shot={items.find((s) => s.id === open)}
          hasPrev={items.findIndex((s) => s.id === open) > 0}
          hasNext={items.findIndex((s) => s.id === open) < items.length - 1}
          onStep={step}
          onClose={() => setOpen(null)}
          onDelete={(s) => {
            void remove(s);
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}

/** Thumbnails load only once they scroll near the viewport. */
function Thumb({ shot, onOpen, onDelete }: { shot: Shot; onOpen: () => void; onDelete: () => void }) {
  const [seen, setSeen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const [src, setSrc] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    const el = wrap.current;
    if (!el || typeof IntersectionObserver === 'undefined') return setSeen(true);
    const io = new IntersectionObserver(
      (e) => {
        if (e.some((x) => x.isIntersecting)) {
          setSeen(true);
          io.disconnect();
        }
      },
      { rootMargin: '300px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);
  useEffect(() => {
    if (!seen) return;
    let alive = true;
    rpc<string>('screenshots.image', { id: shot.id, thumb: true })
      .then((d) => alive && setSrc(d))
      .catch(() => alive && setSrc(null));
    return () => {
      alive = false;
    };
  }, [seen, shot.id]);
  return (
    <Card className="stack" style={{ padding: 0, overflow: 'hidden', gap: 0 }}>
      <div ref={wrap}>
        <button type="button" onClick={onOpen} aria-label={`Open screenshot of ${shot.game} from ${formatDateTime(shot.takenAt)}`} style={{ display: 'block', width: '100%', aspectRatio: '16 / 9', background: 'var(--bg-sunken)', border: 0, padding: 0, cursor: 'zoom-in' }}>
          {src ? <img src={src} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover', display: 'block' }} /> : src === null ? <span className="faint">Can’t show</span> : <span className="skeleton" style={{ display: 'block', width: '100%', height: '100%' }} />}
        </button>
      </div>
      <div className="row" style={{ padding: '0.6rem 0.8rem', gap: '0.5rem' }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontWeight: 650, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{shot.game}</div>
          <div className="faint" style={{ fontSize: '0.8125rem' }}>
            {formatDateTime(shot.takenAt)} · {formatBytes(shot.size)}
          </div>
        </div>
        <Button size="sm" variant="ghost" icon aria-label={`Delete screenshot of ${shot.game}`} onClick={onDelete}>
          <Trash2 size={15} />
        </Button>
      </div>
    </Card>
  );
}

function Viewer({ shot, hasPrev, hasNext, onStep, onClose, onDelete }: { shot: Shot | undefined; hasPrev: boolean; hasNext: boolean; onStep: (d: number) => void; onClose: () => void; onDelete: (s: Shot) => void }) {
  const [src, setSrc] = useState<string | null | undefined>(undefined);
  const id = shot?.id;
  useEffect(() => {
    if (!id) return;
    setSrc(undefined);
    let alive = true;
    rpc<string>('screenshots.image', { id, thumb: false })
      .then((d) => alive && setSrc(d))
      .catch(() => alive && setSrc(null));
    return () => {
      alive = false;
    };
  }, [id]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === 'ArrowLeft') onStep(-1);
      if (e.key === 'ArrowRight') onStep(1);
    };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [onStep]);
  if (!shot) return null;
  return (
    <Modal
      title={`${shot.game}, ${formatDateTime(shot.takenAt)}`}
      onClose={onClose}
      wide
      className="viewer"
      footer={
        <>
          <Button onClick={() => onStep(-1)} disabled={!hasPrev}>
            <ChevronLeft size={16} /> Newer
          </Button>
          <Button onClick={() => onStep(1)} disabled={!hasNext}>
            Older <ChevronRight size={16} />
          </Button>
          <span style={{ flex: 1 }} />
          <Button onClick={() => void act('screenshots.open', { id: shot.id })}>
            <FolderOpen size={15} /> Show file
          </Button>
          <Button variant="danger" onClick={() => onDelete(shot)}>
            <Trash2 size={15} /> Delete
          </Button>
          <Button onClick={onClose}>
            <X size={15} /> Close
          </Button>
        </>
      }
    >
      {src ? <img src={src} alt={`Screenshot of ${shot.game}`} style={{ width: '100%', borderRadius: 8, display: 'block' }} /> : src === null ? <Banner tone="bad">This picture couldn’t be loaded.</Banner> : <div className="skeleton" style={{ aspectRatio: '16 / 9' }} />}
    </Modal>
  );
}
