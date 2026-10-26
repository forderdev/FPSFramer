import { ArrowDown, ArrowLeft, ArrowUp, Bell, ChartLine, Clock, Cpu, Eye, EyeOff, GripVertical, Image as ImageIcon, Lock, LockOpen, Minus, MoveVertical, Plus, Redo2, Save, Shapes, Type, Undo2, Upload, Wand2 } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { SensorPicker, usePreviewCtx } from '../components/shared';
import { Badge, Banner, Button, Empty, InfoTip, Kbd, Segmented, Select, TextInput, confirmAction } from '../components/ui';
import { saveTextFile } from '../lib/files';
import { clone } from '../lib/labels';
import {
  addElement, addMetrics, bringToFront, convertToFree, convertToStack, duplicateElements, elementLabel, frameTimeGraph, moveElement, moveElementTo, newElement, placeNewElementsBelow, removeElements, sensorsOf, snap, updateElement,
  type ElType, type Rect,
} from '../lib/layoutOps';
import { navigate, navigateNow, setNavigationGuard } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { Layout, LayoutEl } from '../lib/types';
import { LayoutRenderer } from '../overlay/LayoutRenderer';
import { Inspector } from './Inspector';
import { useLayoutHistory } from './state';


export default function EditorPage({ id }: { id: string }) {
  const layouts = useApp((s) => s.layouts);
  const stored = layouts.find((l) => l.id === id);
  if (!stored) {
    return (
      <div className="page">
        <Empty title="That layout no longer exists" action={<Button variant="primary" onClick={() => navigate('/layouts')}>Back to layouts</Button>}>
          It may have been deleted. You can start a new one from the Layouts page.
        </Empty>
      </div>
    );
  }
  return <Editor key={stored.id} stored={stored} />;
}

const BACKDROPS = [
  { value: 'game', label: 'Game' },
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Bright' },
  { value: 'checker', label: 'Checks' },
] as const;

function makeCopy(l: Layout): Layout {
  const c = clone(l);
  c.id = '';
  c.builtin = false;
  c.tags = c.tags.filter((t) => t !== 'preset' && t !== 'template');
  c.name = `${l.name} (copy)`;
  return c;
}

type Drag =
  | { kind: 'move'; sx: number; sy: number; base: Layout; origin: Record<string, { x: number; y: number }>; moved: boolean }
  | { kind: 'resize'; handle: string; sx: number; sy: number; base: Layout; id: string; rect: Rect; moved: boolean };

const TYPE_ICON: Record<ElType, ReactNode> = {
  metric: <Cpu size={15} />,
  cores: <Cpu size={15} />,
  text: <Type size={15} />,
  clock: <Clock size={15} />,
  image: <ImageIcon size={15} />,
  shape: <Shapes size={15} />,
  graph: <ChartLine size={15} />,
  alerts: <Bell size={15} />,
  divider: <Minus size={15} />,
  spacer: <MoveVertical size={15} />,
};

function Editor({ stored }: { stored: Layout }) {
  const { catalogById, settings, live } = useApp();
  const advanced = settings.uiMode === 'advanced';
  const isNew = stored.builtin;
  const initial = useMemo(() => (stored.builtin ? makeCopy(stored) : stored), [stored]);
  const h = useLayoutHistory(initial);
  const { draft } = h;
  const savedJson = useRef(JSON.stringify(isNew ? '' : stored));
  const dirty = isNew || JSON.stringify(draft) !== savedJson.current;

  const [selected, setSelected] = useState<string[]>([]);
  const [zoomMode, setZoomMode] = useState<'fit' | number>('fit');
  const centerRef = useRef<HTMLDivElement>(null);
  const [avail, setAvail] = useState({ w: 0, h: 0 });
  const [backdrop, setBackdrop] = useState<(typeof BACKDROPS)[number]['value']>('game');
  const [sample, setSample] = useState(() => live?.capture.state !== 'tracking');
  const [snapOn, setSnapOn] = useState(true);
  const [tab, setTab] = useState<'add' | 'layers'>('add');
  const [picker, setPicker] = useState<'metrics' | 'graph' | null>(null);
  const [rects, setRects] = useState<Record<string, Rect>>({});
  const [size, setSize] = useState({ w: 200, h: 100 });
  // "Fit" picks the biggest zoom that shows the whole overlay in the middle panel.
  const zoom = zoomMode === 'fit' ? ([4, 3, 2, 1.5, 1].find((z) => size.w * z <= avail.w - 48 && size.h * z <= avail.h - 48) ?? 1) : zoomMode;
  useLayoutEffect(() => {
    const el = centerRef.current;
    if (!el) return;
    const measure = () => setAvail({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rootRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const free = draft.canvas.arrangement === 'free';
  const ctx = usePreviewCtx(sample, true);
  const toast = useApp((s) => s.toast);

  // ── measuring rendered elements (for selection boxes, alignment and free conversion) ──
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const next: Record<string, Rect> = {};
    root.querySelectorAll<HTMLElement>('[data-el]').forEach((n) => {
      next[n.dataset.el!] = { x: n.offsetLeft, y: n.offsetTop, w: n.offsetWidth, h: n.offsetHeight };
    });
    const sz = { w: root.offsetWidth, h: root.offsetHeight };
    const same = (a: Rect, b?: Rect) => !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
    const keys = Object.keys(next);
    if (keys.length !== Object.keys(rects).length || keys.some((k) => !same(next[k], rects[k]))) setRects(next);
    if (sz.w !== size.w || sz.h !== size.h) setSize(sz);
  });

  // ── leaving with unsaved changes ──
  useEffect(() => {
    if (!dirty) {
      setNavigationGuard(null);
      return;
    }
    setNavigationGuard(() => confirmAction({ title: 'Leave without saving?', body: 'You have changes in this layout that are not saved yet.', confirmLabel: 'Leave without saving', danger: true }));
    const before = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener('beforeunload', before);
    return () => {
      setNavigationGuard(null);
      window.removeEventListener('beforeunload', before);
    };
  }, [dirty]);

  // ── commands ──
  const select = (ids: string[]) => setSelected(ids);
  const commit = h.commit;

  const addOne = useCallback(
    (el: LayoutEl) => {
      let next = addElement(draft, el, selected.length ? selected[selected.length - 1] : null);
      if (free) next = placeNewElementsBelow(next, [el.id]);
      commit(next);
      select([el.id]);
    },
    [draft, selected, free, commit],
  );

  const duplicateSelected = useCallback(() => {
    if (!selected.length) return;
    const { layout, newIds } = duplicateElements(draft, selected);
    commit(layout);
    select(newIds);
  }, [draft, selected, commit]);

  const deleteSelected = useCallback(() => {
    if (!selected.length) return;
    commit(removeElements(draft, selected));
    select([]);
  }, [draft, selected, commit]);

  const save = async (): Promise<Layout | null> => {
    const l = await act<Layout>('layouts.save', { layout: draft });
    if (!l) return null;
    useApp.setState((s) => ({ layouts: [...s.layouts.filter((x) => x.id !== l.id), l] }));
    savedJson.current = JSON.stringify(l);
    h.reset(l);
    toast({ level: 'success', message: 'Layout saved' });
    if (l.id !== stored.id) {
      setNavigationGuard(null);
      navigateNow(`/editor/${l.id}`);
    }
    return l;
  };

  const useInOverlay = async () => {
    const l = dirty ? await save() : stored;
    if (!l) return;
    await act('overlay.setLayout', { layoutId: l.id }, `“${l.name}” is now your overlay layout`);
  };

  const setArrangement = (a: 'stack' | 'free') => {
    if (a === draft.canvas.arrangement) return;
    if (a === 'free') {
      commit(convertToFree(draft, rects, size));
      toast({ level: 'info', message: 'Switched to free placement. Drag items anywhere. (Ctrl+Z undoes this.)' });
    } else {
      commit(convertToStack(draft));
      toast({ level: 'info', message: 'Switched to an automatic list, ordered top to bottom. (Ctrl+Z undoes this.)' });
    }
  };

  // ── keyboard ──
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        if (e.shiftKey) h.redo();
        else h.undo();
      } else if (mod && e.key.toLowerCase() === 'y') {
        e.preventDefault();
        h.redo();
      } else if (mod && e.key.toLowerCase() === 's') {
        e.preventDefault();
        void save();
      } else if (mod && e.key.toLowerCase() === 'd') {
        e.preventDefault();
        duplicateSelected();
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        if (selected.length) {
          e.preventDefault();
          deleteSelected();
        }
      } else if (e.key === 'Escape') {
        select([]);
      } else if (free && selected.length && e.key.startsWith('Arrow')) {
        e.preventDefault();
        const d = e.shiftKey ? 10 : 1;
        const dx = e.key === 'ArrowLeft' ? -d : e.key === 'ArrowRight' ? d : 0;
        const dy = e.key === 'ArrowUp' ? -d : e.key === 'ArrowDown' ? d : 0;
        commit((l) => ({ ...l, elements: l.elements.map((el) => (selected.includes(el.id) && !el.locked ? { ...el, x: (el.x ?? 0) + dx, y: (el.y ?? 0) + dy } : el)) }), 'nudge');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ── mouse: select, move, resize ──
  const endDrag = useRef<() => void>(() => undefined);
  const startWindowDrag = (d: Drag) => {
    dragRef.current = d;
    const move = (e: PointerEvent) => {
      const dr = dragRef.current;
      if (!dr) return;
      const dx = (e.clientX - dr.sx) / zoom;
      const dy = (e.clientY - dr.sy) / zoom;
      if (!dr.moved && Math.abs(dx) + Math.abs(dy) < 3 / zoom) return;
      dr.moved = true;
      const q = (v: number) => (snapOn && !e.altKey ? snap(v, 4) : Math.round(v));
      if (dr.kind === 'move') {
        h.replace({ ...dr.base, elements: dr.base.elements.map((el) => (dr.origin[el.id] ? { ...el, x: q(dr.origin[el.id].x + dx), y: q(dr.origin[el.id].y + dy) } : el)) });
      } else {
        let { x, y, w, h: hh } = dr.rect;
        if (dr.handle.includes('e')) w = dr.rect.w + dx;
        if (dr.handle.includes('s')) hh = dr.rect.h + dy;
        if (dr.handle.includes('w')) (x = dr.rect.x + dx), (w = dr.rect.w - dx);
        if (dr.handle.includes('n')) (y = dr.rect.y + dy), (hh = dr.rect.h - dy);
        w = Math.max(8, q(w));
        hh = Math.max(8, q(hh));
        h.replace(updateElement(dr.base, dr.id, { x: q(x), y: q(y), w, h: hh }));
      }
    };
    const up = () => {
      const dr = dragRef.current;
      if (dr?.moved) h.checkpoint(dr.base);
      dragRef.current = null;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    endDrag.current = up;
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const onElementPointerDown = (id: string, e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    let sel = selected;
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      sel = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
      select(sel);
    } else if (!selected.includes(id)) {
      sel = [id];
      select(sel);
    }
    if (!free || !sel.includes(id)) return;
    const movable = sel.filter((s) => !draft.elements.find((el) => el.id === s)?.locked);
    if (!movable.length) return;
    const origin: Record<string, { x: number; y: number }> = {};
    for (const s of movable) {
      const el = draft.elements.find((x) => x.id === s)!;
      origin[s] = { x: el.x ?? rects[s]?.x ?? 0, y: el.y ?? rects[s]?.y ?? 0 };
    }
    startWindowDrag({ kind: 'move', sx: e.clientX, sy: e.clientY, base: draft, origin, moved: false });
  };

  const onHandleDown = (handle: string, id: string, e: React.PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    const r = rects[id];
    if (!r) return;
    startWindowDrag({ kind: 'resize', handle, sx: e.clientX, sy: e.clientY, base: draft, id, rect: r, moved: false });
  };

  const single = selected.length === 1 ? draft.elements.find((e) => e.id === selected[0]) : undefined;
  const unavailableNote = useMemo(() => {
    const gone = draft.elements.filter((e) => e.visible !== false).filter((e) => {
      const ids = sensorsOf(e);
      return ids.length > 0 && ids.every((i) => catalogById[i] && !catalogById[i].availability.available);
    });
    return gone.length;
  }, [draft, catalogById]);

  const backdropClass = backdrop === 'game' ? 'game-bg' : backdrop === 'checker' ? 'checker' : '';
  const backdropStyle = backdrop === 'dark' ? { background: '#05070b' } : backdrop === 'light' ? { background: '#e9eef6' } : undefined;

  const exportLayout = async () => {
    if (dirty) return toast({ level: 'warning', message: 'Save the layout first, then export it.' });
    const r = await act<{ filename: string; json: string }>('layouts.export', { id: stored.id });
    if (r && (await saveTextFile(r.filename, r.json))) toast({ level: 'success', message: 'Layout exported' });
  };

  return (
    <div className="ed">
      <div className="ed-top" role="toolbar" aria-label="Editor toolbar">
        <Button variant="ghost" size="sm" onClick={() => navigate('/layouts')}>
          <ArrowLeft size={16} /> Layouts
        </Button>
        <div style={{ width: '14rem' }}>
          <TextInput label="Layout name" small value={draft.name} maxLength={60} onChange={(v) => commit((l) => ({ ...l, name: v }), 'name')} />
        </div>
        {dirty ? <Badge tone="warn">{isNew ? 'New copy, not saved yet' : 'Unsaved changes'}</Badge> : <Badge tone="ok">Saved</Badge>}
        <Button size="sm" icon aria-label="Undo" title="Undo (Ctrl+Z)" disabled={!h.canUndo} onClick={h.undo}>
          <Undo2 size={16} />
        </Button>
        <Button size="sm" icon aria-label="Redo" title="Redo (Ctrl+Y)" disabled={!h.canRedo} onClick={h.redo}>
          <Redo2 size={16} />
        </Button>
        <div style={{ width: '6.5rem' }}>
          <Select small label="Zoom" value={zoomMode} options={[{ value: 'fit' as const, label: `Fit (${zoom * 100}%)` }, ...[1, 1.5, 2, 3, 4].map((z) => ({ value: z as 'fit' | number, label: `${z * 100}%` }))]} onChange={setZoomMode} />
        </div>
        <Segmented label="Preview background" value={backdrop} options={BACKDROPS.map((b) => ({ value: b.value, label: b.label }))} onChange={setBackdrop} />
        <Segmented
          label="Preview values"
          value={sample ? 'sample' : 'live'}
          options={[
            { value: 'sample', label: 'Sample', title: 'Made-up numbers so you can see the design' },
            { value: 'live', label: 'Live', title: 'Your real readings' },
          ]}
          onChange={(v) => setSample(v === 'sample')}
        />
        <InfoTip text={<>Shortcuts: <Kbd>Ctrl+Z</Kbd> undo · <Kbd>Ctrl+S</Kbd> save · <Kbd>Ctrl+D</Kbd> duplicate · <Kbd>Delete</Kbd> remove · arrow keys nudge in free mode · <Kbd>Shift</Kbd>+click selects several.</>} label="Keyboard shortcuts" />
        <div className="spacer" />
        {free && (
          <label className="row" style={{ gap: 6, fontSize: '0.8125rem' }}>
            <input type="checkbox" checked={snapOn} onChange={(e) => setSnapOn(e.target.checked)} /> Snap to grid
          </label>
        )}
        <Button size="sm" disabled={!dirty || isNew} onClick={() => { h.reset(stored); select([]); }}>
          Revert
        </Button>
        <Button size="sm" onClick={exportLayout} disabled={isNew}>
          <Upload size={15} /> Export
        </Button>
        <Button size="sm" variant="primary" onClick={() => void save()} disabled={!dirty}>
          <Save size={15} /> Save
        </Button>
        <Button size="sm" onClick={() => void useInOverlay()}>
          <Wand2 size={15} /> Use in my overlay
        </Button>
      </div>

      <div className="ed-body">
        {/* ───── left: add + layers ───── */}
        <div className="ed-left">
          <div className="ed-pad">
            <Segmented label="Panel" value={tab} options={[{ value: 'add', label: 'Add' }, { value: 'layers', label: `Items (${draft.elements.length})` }]} onChange={setTab} />
          </div>
          {tab === 'add' ? (
            <div className="ed-pad stack" style={{ paddingTop: 0 }}>
              <Button variant="primary" onClick={() => setPicker('metrics')}>
                <Plus size={16} /> Add measurements…
              </Button>
              <div className="faint" style={{ fontSize: '0.75rem', fontWeight: 650, letterSpacing: '0.06em', textTransform: 'uppercase' }}>
                Popular
              </div>
              <div className="row wrap" style={{ gap: 6 }}>
                {['fps', 'fps.low1', 'frametime', 'cpu.load', 'cpu.temp', 'gpu.load', 'gpu.temp', 'gpu.vram.used', 'ram.used', 'net.down'].map((id) => (
                  <button key={id} type="button" className="chip" onClick={() => commit(addMetrics(draft, [id], catalogById))} title={catalogById[id]?.description}>
                    <Plus size={12} aria-hidden /> {catalogById[id]?.short ?? id}
                  </button>
                ))}
              </div>
              <div className="faint" style={{ fontSize: '0.75rem', fontWeight: 650, letterSpacing: '0.06em', textTransform: 'uppercase', marginTop: 6 }}>
                Other things
              </div>
              <PalButton icon={<ChartLine size={16} />} label="Graph of a measurement…" onClick={() => setPicker('graph')} />
              <PalButton icon={<ChartLine size={16} />} label="Frame-time graph" onClick={() => addOne(frameTimeGraph())} />
              <PalButton icon={<Cpu size={16} />} label="CPU cores bars" onClick={() => addOne(newElement('cores', { free }))} />
              <PalButton icon={<Type size={16} />} label="Text" onClick={() => addOne(newElement('text', { free }))} />
              <PalButton icon={<Clock size={16} />} label="Clock" onClick={() => addOne(newElement('clock', { free }))} />
              <PalButton icon={<Clock size={16} />} label="Session timer" onClick={() => addOne({ ...newElement('clock', { free }), mode: 'session' })} />
              <PalButton icon={<ImageIcon size={16} />} label="Image" onClick={() => addOne(newElement('image'))} />
              <PalButton icon={<Shapes size={16} />} label="Shape" onClick={() => addOne(newElement('shape'))} />
              <PalButton icon={<Bell size={16} />} label="Alerts area" onClick={() => addOne(newElement('alerts', { free }))} />
              <PalButton icon={<Minus size={16} />} label="Divider line" onClick={() => addOne(newElement('divider', { free }))} />
              {!free && <PalButton icon={<MoveVertical size={16} />} label="Spacer" onClick={() => addOne(newElement('spacer'))} />}
              <p className="faint" style={{ fontSize: '0.75rem' }}>
                {free ? 'Free placement: drag items anywhere on the canvas.' : 'Automatic list: items flow top to bottom. Switch to free placement in the “Whole overlay” settings (click an empty spot) to position things exactly.'}
              </p>
            </div>
          ) : (
            <Layers draft={draft} selected={selected} select={select} commit={commit} catalog={catalogById} free={free} />
          )}
        </div>

        {/* ───── center: canvas ───── */}
        <div ref={centerRef} className={`ed-center ${backdropClass}`} style={backdropStyle} onPointerDown={(e) => e.target === e.currentTarget && select([])}>
          <div
            ref={stageRef}
            className="ed-stage"
            style={{ width: size.w * zoom, height: size.h * zoom }}
            onPointerDown={(e) => {
              if (e.target === e.currentTarget) select([]);
            }}
          >
            <div style={{ transform: `scale(${zoom})`, transformOrigin: '0 0', width: size.w, height: size.h }}>
              <LayoutRenderer layout={draft} ctx={ctx} rootRef={rootRef} selectedIds={selected} onElementPointerDown={onElementPointerDown} />
            </div>
            <div style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}>
              {selected.map((id) => {
                const r = rects[id];
                if (!r) return null;
                return <div key={id} className={`sel-box ${selected.length > 1 ? 'multi' : ''}`} style={{ left: r.x * zoom - 1, top: r.y * zoom - 1, width: r.w * zoom + 2, height: r.h * zoom + 2 }} />;
              })}
              {free && single && !single.locked && rects[single.id] && <Handles r={rects[single.id]} z={zoom} onDown={(hd, e) => onHandleDown(hd, single.id, e)} />}
            </div>
          </div>
          <div style={{ position: 'sticky', bottom: 8, left: 8, display: 'flex', gap: 8, padding: '0 0.75rem 0.5rem', flexWrap: 'wrap' }}>
            {sample && <Badge>Sample values, not your readings</Badge>}
            {unavailableNote > 0 && !sample && <Badge tone="warn">{unavailableNote} item{unavailableNote === 1 ? '' : 's'} can’t be measured on this PC</Badge>}
          </div>
        </div>

        {/* ───── right: properties ───── */}
        <div className="ed-right">
          <Inspector
            draft={draft}
            selected={selected}
            rects={rects}
            commit={commit}
            advanced={advanced}
            catalog={catalogById}
            onArrangement={setArrangement}
            onDuplicate={duplicateSelected}
          />
          {isNew && (
            <div className="ed-pad">
              <Banner tone="info" title="Editing a copy">
                “{stored.name}” is a built-in layout and can’t be changed, so this is your own copy. Press Save to keep it.
              </Banner>
            </div>
          )}
        </div>
      </div>

      {picker === 'metrics' && (
        <SensorPicker
          multi
          title="Add measurements"
          onClose={() => setPicker(null)}
          onPick={(ids) => {
            setPicker(null);
            commit(addMetrics(draft, ids, catalogById));
          }}
        />
      )}
      {picker === 'graph' && (
        <SensorPicker
          numericOnly
          title="Which measurement should the graph draw?"
          onClose={() => setPicker(null)}
          onPick={([sid]) => {
            setPicker(null);
            addOne(newElement('graph', { sensorId: sid, catalog: catalogById }));
          }}
        />
      )}
    </div>
  );
}

function PalButton({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
  return (
    <button type="button" className="pal-btn" onClick={onClick}>
      {icon}
      {label}
    </button>
  );
}

function Handles({ r, z, onDown }: { r: Rect; z: number; onDown: (handle: string, e: React.PointerEvent) => void }) {
  const pts: [string, number, number, string][] = [
    ['nw', 0, 0, 'nwse-resize'],
    ['n', 0.5, 0, 'ns-resize'],
    ['ne', 1, 0, 'nesw-resize'],
    ['e', 1, 0.5, 'ew-resize'],
    ['se', 1, 1, 'nwse-resize'],
    ['s', 0.5, 1, 'ns-resize'],
    ['sw', 0, 1, 'nesw-resize'],
    ['w', 0, 0.5, 'ew-resize'],
  ];
  return (
    <>
      {pts.map(([k, fx, fy, cur]) => (
        <div key={k} className="handle" style={{ left: (r.x + r.w * fx) * z, top: (r.y + r.h * fy) * z, cursor: cur }} onPointerDown={(e) => onDown(k, e)} aria-hidden />
      ))}
    </>
  );
}

function Layers({ draft, selected, select, commit, catalog, free }: { draft: Layout; selected: string[]; select: (ids: string[]) => void; commit: (l: Layout, key?: string) => void; catalog: Record<string, import('../lib/types').SensorDesc>; free: boolean }) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const one = selected.length === 1 ? selected[0] : null;
  const idx = one ? draft.elements.findIndex((e) => e.id === one) : -1;
  return (
    <div className="ed-pad" style={{ paddingTop: 0 }}>
      {free && <p className="faint" style={{ fontSize: '0.75rem', marginBottom: 6 }}>Items lower in the list are drawn on top.</p>}
      <div role="listbox" aria-label="Items in this layout" aria-multiselectable style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
        {draft.elements.map((el, i) => {
          const gone = sensorsOf(el).length > 0 && sensorsOf(el).every((s) => catalog[s] && !catalog[s].availability.available);
          return (
            <div
              key={el.id}
              role="option"
              aria-selected={selected.includes(el.id)}
              tabIndex={0}
              className={`layer ${selected.includes(el.id) ? 'sel' : ''} ${el.visible === false ? 'off' : ''} ${over === i && dragId ? 'drop-before' : ''}`}
              draggable
              onDragStart={(e) => {
                setDragId(el.id);
                e.dataTransfer.effectAllowed = 'move';
              }}
              onDragOver={(e) => {
                e.preventDefault();
                setOver(i);
              }}
              onDragEnd={() => (setDragId(null), setOver(null))}
              onDrop={(e) => {
                e.preventDefault();
                if (dragId && dragId !== el.id) commit(moveElementTo(draft, dragId, i));
                setDragId(null);
                setOver(null);
              }}
              onClick={(e) => select(e.shiftKey || e.ctrlKey ? (selected.includes(el.id) ? selected.filter((x) => x !== el.id) : [...selected, el.id]) : [el.id])}
              onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), select([el.id]))}
            >
              <GripVertical size={14} aria-hidden style={{ color: 'var(--text-3)', flex: 'none', cursor: 'grab' }} />
              <span style={{ color: 'var(--text-3)', display: 'grid' }}>{TYPE_ICON[el.type]}</span>
              <span className="nm" title={gone ? 'This PC cannot measure this' : undefined}>
                {elementLabel(el, catalog)}
              </span>
              {gone && <Badge tone="warn">n/a</Badge>}
              <button type="button" className="btn ghost sm icon" style={{ minHeight: '1.6rem', width: '1.6rem' }} aria-label={el.visible === false ? 'Show item' : 'Hide item'} onClick={(e) => (e.stopPropagation(), commit(updateElement(draft, el.id, { visible: el.visible === false })))}>
                {el.visible === false ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
              {free && (
                <button type="button" className="btn ghost sm icon" style={{ minHeight: '1.6rem', width: '1.6rem' }} aria-label={el.locked ? 'Unlock item' : 'Lock item'} onClick={(e) => (e.stopPropagation(), commit(updateElement(draft, el.id, { locked: !el.locked || undefined })))}>
                  {el.locked ? <Lock size={14} /> : <LockOpen size={14} />}
                </button>
              )}
            </div>
          );
        })}
        {draft.elements.length === 0 && <p className="muted">Nothing here yet. Use the Add tab.</p>}
      </div>
      {one && (
        <div className="row" style={{ marginTop: 10 }}>
          <Button size="sm" icon aria-label="Move up" disabled={idx <= 0} onClick={() => commit(moveElement(draft, one, -1))}>
            <ArrowUp size={15} />
          </Button>
          <Button size="sm" icon aria-label="Move down" disabled={idx >= draft.elements.length - 1} onClick={() => commit(moveElement(draft, one, 1))}>
            <ArrowDown size={15} />
          </Button>
          {free && (
            <Button size="sm" onClick={() => commit(bringToFront(draft, one))}>
              Bring to front
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
