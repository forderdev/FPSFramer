import { AlignCenterHorizontal, AlignCenterVertical, AlignEndHorizontal, AlignEndVertical, AlignStartHorizontal, AlignStartVertical, Copy, Maximize2, Plus, Trash2, Upload } from 'lucide-react';
import { useState } from 'react';
import { SensorPicker } from '../components/shared';
import { Banner, Button, Segmented } from '../components/ui';
import { openImageFile } from '../lib/files';
import { alignElements, distribute, elementLabel, fitCanvasToContent, removeElements, setAccent, setElementPath, setPath, typeLabel, updateCanvas, type Align, type Rect } from '../lib/layoutOps';
import { useApp } from '../lib/store';
import type { Layout, LayoutEl, SensorDesc } from '../lib/types';
import { FONT_OPTIONS } from '../overlay/fonts';
import { Prop, PColor, PNum, PSelect, PSeg, PSwitch, PText, Section } from './fields';

export interface InspectorProps {
  draft: Layout;
  selected: string[];
  rects: Record<string, Rect>;
  commit: (next: Layout, key?: string) => void;
  advanced: boolean;
  catalog: Record<string, SensorDesc>;
  onArrangement: (a: 'stack' | 'free') => void;
  onDuplicate: () => void;
}

export function Inspector(p: InspectorProps) {
  if (p.selected.length === 0) return <CanvasProps {...p} />;
  if (p.selected.length > 1) return <MultiProps {...p} />;
  const el = p.draft.elements.find((e) => e.id === p.selected[0]);
  if (!el) return <CanvasProps {...p} />;
  return <ElementProps {...p} el={el} />;
}

// ───────────────────────────── whole layout ─────────────────────────────

function CanvasProps({ draft, commit, advanced, onArrangement }: InspectorProps) {
  const c = draft.canvas;
  const set = (path: string, v: unknown, key?: string) => commit(updateCanvas(draft, path, v), key ?? `canvas:${path}`);
  const free = c.arrangement === 'free';
  return (
    <>
      <div className="ed-pad">
        <div style={{ fontWeight: 700 }}>Whole overlay</div>
        <div className="muted" style={{ fontSize: '0.8125rem' }}>Nothing is selected. These settings apply to the whole layout. Click an item to edit just that item.</div>
      </div>
      <Section title="Arrangement">
        <PSeg label="Mode" value={c.arrangement} options={[{ value: 'stack', label: 'List' }, { value: 'free', label: 'Free' }]} onChange={onArrangement} />
        {!free && (
          <>
            <PSeg label="Direction" value={c.direction} options={[{ value: 'column', label: 'Down' }, { value: 'row', label: 'Across' }]} onChange={(v) => set('direction', v)} />
            {c.direction === 'column' && <PNum label="Columns" value={c.columns} min={1} max={6} onChange={(v) => set('columns', v ?? 1)} help="More than one column places measurements side by side." />}
            <PNum label="Width" value={c.width} min={60} max={1600} suffix="px" placeholder="Fits content" onChange={(v) => set('width', v)} />
            <PNum label="Spacing" value={c.gap} min={0} max={40} suffix="px" onChange={(v) => set('gap', v ?? 0)} />
            <PNum label="Padding" value={c.padding} min={0} max={40} suffix="px" onChange={(v) => set('padding', v ?? 0)} />
          </>
        )}
        {free && (
          <>
            <PNum label="Width" value={c.width ?? 360} min={40} max={3000} suffix="px" onChange={(v) => set('width', v ?? 360)} />
            <PNum label="Height" value={c.height ?? 180} min={20} max={2000} suffix="px" onChange={(v) => set('height', v ?? 180)} />
            <Button size="sm" onClick={() => commit(fitCanvasToContent(draft, 8))}>
              <Maximize2 size={14} /> Shrink to fit items
            </Button>
          </>
        )}
      </Section>
      <Section title="Background">
        <PColor label="Colour" value={c.background.color} fallback="#0a0e14" onChange={(v) => set('background.color', v ?? '#0a0e14')} />
        <PNum label="Opacity" value={Math.round(c.background.opacity * 100)} min={0} max={100} suffix="%" onChange={(v) => set('background.opacity', (v ?? 0) / 100)} />
        <PNum label="Corner radius" value={c.background.radius} min={0} max={60} suffix="px" onChange={(v) => set('background.radius', v ?? 0)} />
        {advanced && (
          <>
            <PColor label="Border colour" value={c.background.borderColor} fallback="#ffffff" onChange={(v) => set('background.borderColor', v ?? '#ffffff')} />
            <PNum label="Border width" value={c.background.borderWidth ?? 0} min={0} max={8} suffix="px" onChange={(v) => set('background.borderWidth', v ?? 0)} />
            <PNum label="Border opacity" value={Math.round((c.background.borderOpacity ?? 0.1) * 100)} min={0} max={100} suffix="%" onChange={(v) => set('background.borderOpacity', (v ?? 0) / 100)} />
          </>
        )}
      </Section>
      <Section title="Text">
        <PSelect label="Font" value={c.font.family} options={FONT_OPTIONS.map((f) => ({ value: f.value, label: f.label }))} onChange={(v) => set('font.family', v)} />
        <PNum label="Size" value={c.font.size} min={8} max={48} suffix="px" onChange={(v) => set('font.size', v ?? 14)} />
        <PSelect label="Weight" value={c.font.weight} options={[400, 500, 600, 700, 800].map((w) => ({ value: w, label: String(w) }))} onChange={(v) => set('font.weight', Number(v))} />
        <PColor label="Text colour" value={c.textColor} fallback="#f1f5f9" onChange={(v) => set('textColor', v ?? '#f1f5f9')} />
        <PColor label="Label colour" value={c.labelColor} fallback="#9fb3c8" onChange={(v) => set('labelColor', v ?? '#9fb3c8')} />
        <PColor label="Accent" value={c.accent} fallback="#3ddcff" onChange={(v) => commit(setAccent(draft, v ?? '#3ddcff'), 'accent')} />
        <PSelect label="Readability" value={c.textShadow} options={[{ value: 'none', label: 'Plain' }, { value: 'soft', label: 'Soft shadow' }, { value: 'outline', label: 'Outline' }]} onChange={(v) => set('textShadow', v)} help="A shadow or outline keeps numbers legible on bright scenes." />
      </Section>
      <Section title="Behaviour" open={advanced}>
        <PSwitch label="Hide missing" value={c.unavailable === 'hide'} onChange={(v) => set('unavailable', v ? 'hide' : 'show')} help="Hide rows your PC can’t measure (for example CPU temperature without a companion tool). Turn off to show “N/A” instead." />
      </Section>
      {advanced && (
        <Section title="Warning colours" open={false}>
          <PColor label="Elevated" value={c.colors.elevated} fallback="#fbbf24" onChange={(v) => set('colors.elevated', v ?? '#fbbf24')} />
          <PColor label="Critical" value={c.colors.critical} fallback="#ff6b6b" onChange={(v) => set('colors.critical', v ?? '#ff6b6b')} />
        </Section>
      )}
    </>
  );
}

// ───────────────────────────── several elements ─────────────────────────────

function MultiProps({ draft, selected, rects, commit, onDuplicate }: InspectorProps) {
  const free = draft.canvas.arrangement === 'free';
  const al = (how: Align) => commit(alignElements(draft, selected, how, rects));
  return (
    <>
      <div className="ed-pad">
        <div style={{ fontWeight: 700 }}>{selected.length} items selected</div>
        <div className="muted" style={{ fontSize: '0.8125rem' }}>Shift-click an item to add or remove it from the selection.</div>
      </div>
      <Section title="Actions">
        <div className="row wrap">
          <Button size="sm" onClick={onDuplicate}>
            <Copy size={14} /> Duplicate
          </Button>
          <Button size="sm" variant="danger" onClick={() => commit(removeElements(draft, selected))}>
            <Trash2 size={14} /> Delete
          </Button>
        </div>
      </Section>
      {free && (
        <Section title="Align">
          <div className="row wrap" role="group" aria-label="Align">
            {(
              [
                ['left', AlignStartVertical, 'Align left edges'],
                ['center', AlignCenterVertical, 'Centre horizontally'],
                ['right', AlignEndVertical, 'Align right edges'],
                ['top', AlignStartHorizontal, 'Align top edges'],
                ['middle', AlignCenterHorizontal, 'Centre vertically'],
                ['bottom', AlignEndHorizontal, 'Align bottom edges'],
              ] as const
            ).map(([k, Icon, label]) => (
              <Button key={k} size="sm" icon aria-label={label} title={label} onClick={() => al(k)}>
                <Icon size={16} />
              </Button>
            ))}
          </div>
          {selected.length > 2 && (
            <div className="row wrap">
              <Button size="sm" onClick={() => commit(distribute(draft, selected, 'h', rects))}>
                Space evenly →
              </Button>
              <Button size="sm" onClick={() => commit(distribute(draft, selected, 'v', rects))}>
                Space evenly ↓
              </Button>
            </div>
          )}
        </Section>
      )}
    </>
  );
}

// ───────────────────────────── one element ─────────────────────────────

function ElementProps({ draft, el, commit, advanced, catalog, onDuplicate }: InspectorProps & { el: LayoutEl }) {
  const free = draft.canvas.arrangement === 'free';
  const set = (path: string, v: unknown, key?: string) => commit(setElementPath(draft, el.id, path, v), key ?? `${el.id}:${path}`);
  const style = el.style ?? {};
  const hasText = el.type === 'metric' || el.type === 'text' || el.type === 'clock';
  return (
    <>
      <div className="ed-pad">
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="faint" style={{ fontSize: '0.75rem', textTransform: 'uppercase', letterSpacing: '0.06em', fontWeight: 650 }}>
              {typeLabel(el.type)}
            </div>
            <div style={{ fontWeight: 700, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{elementLabel(el, catalog)}</div>
          </div>
          <Button size="sm" icon aria-label="Duplicate" title="Duplicate (Ctrl+D)" onClick={onDuplicate}>
            <Copy size={15} />
          </Button>
          <Button size="sm" variant="danger" icon aria-label="Delete" title="Delete (Delete key)" onClick={() => commit(removeElements(draft, [el.id]))}>
            <Trash2 size={15} />
          </Button>
        </div>
      </div>

      {el.type === 'metric' && <MetricProps el={el} set={set} catalog={catalog} advanced={advanced} />}
      {el.type === 'cores' && (
        <Section title="CPU cores">
          <PSeg label="Show" value={el.metric ?? 'load'} options={[{ value: 'load', label: 'Usage' }, { value: 'clock', label: 'Speed' }, { value: 'temp', label: 'Temp' }]} onChange={(v) => set('metric', v)} />
          <PNum label="Columns" value={el.columns ?? 8} min={1} max={16} onChange={(v) => set('columns', v ?? 8)} />
          <PNum label="Bar height" value={el.barHeight ?? 16} min={4} max={60} suffix="px" onChange={(v) => set('barHeight', v ?? 16)} />
          <PSwitch label="Numbers" value={el.showNumbers !== false} onChange={(v) => set('showNumbers', v)} />
        </Section>
      )}
      {el.type === 'text' && (
        <Section title="Text">
          <PText label="Content" multiline value={el.text ?? ''} onChange={(v) => set('text', v)} />
        </Section>
      )}
      {el.type === 'clock' && (
        <Section title="Clock">
          <PSelect label="Shows" value={el.mode ?? 'time'} options={[{ value: 'time', label: 'Time of day' }, { value: 'date', label: 'Date' }, { value: 'datetime', label: 'Date and time' }, { value: 'session', label: 'Session timer' }]} onChange={(v) => set('mode', v)} help="The session timer counts up while a recording is running." />
          {el.mode !== 'session' && (
            <>
              <PSwitch label="12-hour" value={!!el.hour12} onChange={(v) => set('hour12', v)} />
              <PSwitch label="Seconds" value={!!el.seconds} onChange={(v) => set('seconds', v)} />
            </>
          )}
        </Section>
      )}
      {el.type === 'image' && <ImageProps el={el} set={set} />}
      {el.type === 'shape' && (
        <Section title="Shape">
          <PSeg label="Type" value={el.shape ?? 'rect'} options={[{ value: 'rect', label: 'Box' }, { value: 'ellipse', label: 'Circle' }, { value: 'line', label: 'Line' }]} onChange={(v) => set('shape', v)} />
          <PColor label="Fill" value={typeof el.fill === 'string' ? el.fill : undefined} fallback="#3ddcff" onChange={(v) => set('fill', v ?? '#3ddcff')} />
          <PNum label="Fill opacity" value={Math.round((el.fillOpacity ?? 1) * 100)} min={0} max={100} suffix="%" onChange={(v) => set('fillOpacity', (v ?? 0) / 100)} />
          <PColor label="Outline" value={el.stroke} fallback="#3ddcff" onChange={(v) => set('stroke', v ?? '#3ddcff')} />
          <PNum label="Outline width" value={el.strokeWidth ?? 0} min={0} max={20} suffix="px" onChange={(v) => set('strokeWidth', v ?? 0)} />
          {el.shape === 'rect' && <PNum label="Corner radius" value={el.radius ?? 0} min={0} max={100} suffix="px" onChange={(v) => set('radius', v ?? 0)} />}
        </Section>
      )}
      {el.type === 'graph' && <GraphProps el={el} set={set} catalog={catalog} advanced={advanced} />}
      {el.type === 'alerts' && (
        <Section title="Alerts">
          <PNum label="Show up to" value={el.maxVisible ?? 3} min={1} max={8} onChange={(v) => set('maxVisible', v ?? 3)} />
          <div className="field-help">Shows active warnings (for example “GPU temperature is high”) on the overlay. It stays empty when everything is fine. The editor shows a sample so you can place it.</div>
        </Section>
      )}
      {(el.type === 'divider' || el.type === 'spacer') && (
        <Section title={typeLabel(el.type)}>
          <PNum label="Size" value={el.size ?? 8} min={1} max={200} suffix="px" onChange={(v) => set('size', v ?? 8)} />
        </Section>
      )}

      {hasText && (
        <Section title="Text style" open={false}>
          <PNum label="Size" value={style.fontSize} min={6} max={120} suffix="px" placeholder="Same as layout" onChange={(v) => set('style.fontSize', v)} />
          <PSelect label="Weight" value={style.fontWeight ?? 0} options={[{ value: 0, label: 'Same as layout' }, ...[400, 500, 600, 700, 800].map((w) => ({ value: w, label: String(w) }))]} onChange={(v) => set('style.fontWeight', Number(v) || undefined)} />
          <PColor label="Colour" value={style.color} fallback={draft.canvas.textColor} resetTo="" onChange={(v) => set('style.color', v)} />
          <PSeg label="Align" value={style.align ?? 'left'} options={[{ value: 'left', label: 'Left' }, { value: 'center', label: 'Centre' }, { value: 'right', label: 'Right' }]} onChange={(v) => set('style.align', v)} />
          {advanced && <PSelect label="Font" value={style.fontFamily ?? ''} options={[{ value: '', label: 'Same as layout' }, ...FONT_OPTIONS.map((f) => ({ value: f.value, label: f.label }))]} onChange={(v) => set('style.fontFamily', v || undefined)} />}
        </Section>
      )}

      <Section title={free ? 'Position & size' : 'Size'} open={free}>
        {free ? (
          <>
            <PNum label="Left" value={el.x ?? 0} suffix="px" onChange={(v) => set('x', v ?? 0)} />
            <PNum label="Top" value={el.y ?? 0} suffix="px" onChange={(v) => set('y', v ?? 0)} />
            <PNum label="Width" value={el.w} min={4} suffix="px" placeholder="Automatic" onChange={(v) => set('w', v)} />
            <PNum label="Height" value={el.h} min={4} suffix="px" placeholder="Automatic" onChange={(v) => set('h', v)} />
          </>
        ) : (
          <>
            {draft.canvas.columns > 1 && draft.canvas.direction === 'column' && <PNum label="Spans" value={el.span ?? 1} min={1} max={draft.canvas.columns} suffix={`of ${draft.canvas.columns} columns`} onChange={(v) => set('span', v === 1 ? undefined : v)} />}
            <PNum label="Width" value={el.w} min={4} suffix="px" placeholder="Automatic" onChange={(v) => set('w', v)} />
            {(el.type === 'graph' || el.type === 'image' || el.type === 'shape') && <PNum label="Height" value={el.h} min={4} suffix="px" placeholder="Automatic" onChange={(v) => set('h', v)} />}
          </>
        )}
      </Section>

      {advanced && (
        <Section title="Advanced" open={false}>
          <PNum label="Opacity" value={Math.round((el.opacity ?? 1) * 100)} min={0} max={100} suffix="%" onChange={(v) => set('opacity', (v ?? 100) >= 100 ? undefined : (v ?? 0) / 100)} />
          <PText label="Name" value={el.name ?? ''} placeholder="Shown in the layer list" onChange={(v) => set('name', v || undefined)} />
          {(el.type === 'metric' || el.type === 'graph' || el.type === 'cores') && <PSwitch label="Show if missing" value={!!el.showWhenUnavailable} onChange={(v) => set('showWhenUnavailable', v || undefined)} help="Show this item (with “N/A”) even when this PC can’t provide the measurement." />}
        </Section>
      )}
    </>
  );
}

// ───────────────────────────── metric ─────────────────────────────

type Setter = (path: string, v: unknown, key?: string) => void;

function MetricProps({ el, set, catalog, advanced }: { el: LayoutEl; set: Setter; catalog: Record<string, SensorDesc>; advanced: boolean }) {
  const [picker, setPicker] = useState<{ kind: 'change'; i: number } | { kind: 'add' } | null>(null);
  const values = el.values ?? [];
  const label = el.label ?? { mode: 'auto' as const };
  return (
    <>
      <Section title="What it shows">
        {values.map((v, i) => {
          const d = catalog[v.sensorId];
          return (
            <div key={i} className="stack" style={{ gap: '0.4rem', padding: '0.5rem', border: '1px solid var(--border)', borderRadius: 10 }}>
              <div className="row">
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 650, fontSize: '0.875rem' }}>{d?.name ?? v.sensorId}</div>
                  {d && !d.availability.available && <div style={{ color: 'var(--warn)', fontSize: '0.75rem' }}>Not available on this PC</div>}
                </div>
                <Button size="sm" onClick={() => setPicker({ kind: 'change', i })}>
                  Change
                </Button>
                {values.length > 1 && (
                  <Button size="sm" variant="ghost" icon aria-label="Remove this value" onClick={() => set('values', values.filter((_, k) => k !== i))}>
                    <Trash2 size={14} />
                  </Button>
                )}
              </div>
              {advanced && (
                <>
                  <PSelect label="Value" value={v.stat ?? 'live'} options={[{ value: 'live', label: 'Live' }, { value: 'min', label: 'Lowest so far' }, { value: 'max', label: 'Highest so far' }, { value: 'avg', label: 'Average so far' }]} onChange={(s) => set(`values.${i}.stat`, s === 'live' ? undefined : s)} />
                  <PNum label="Decimals" value={v.decimals} min={0} max={3} placeholder="Automatic" onChange={(n) => set(`values.${i}.decimals`, n)} />
                  <PSelect label="Colour" value={v.colorMode ?? 'status'} options={[{ value: 'status', label: 'Warn when high/low' }, { value: 'fixed', label: 'Fixed colour' }, { value: 'inherit', label: 'Plain text' }]} onChange={(m) => set(`values.${i}.colorMode`, m)} />
                  {v.colorMode === 'fixed' && <PColor label="Fixed colour" value={v.color} fallback="#ffffff" onChange={(c) => set(`values.${i}.color`, c)} />}
                  <PSwitch label="Hide unit" value={!!v.hideUnit} onChange={(h) => set(`values.${i}.hideUnit`, h || undefined)} />
                </>
              )}
            </div>
          );
        })}
        <Button size="sm" onClick={() => setPicker({ kind: 'add' })}>
          <Plus size={14} /> Add another value to this row
        </Button>
      </Section>
      <Section title="Label & style">
        <PSeg label="Label" value={label.mode} options={[{ value: 'auto', label: 'Automatic' }, { value: 'text', label: 'Custom' }, { value: 'none', label: 'None' }]} onChange={(m) => set('label', { ...label, mode: m, text: label.text ?? catalog[values[0]?.sensorId]?.short ?? '' }, `${el.id}:label`)} />
        {label.mode === 'text' && <PText label="Label text" value={label.text ?? ''} onChange={(t) => set('label.text', t)} />}
        <PSelect label="Display as" value={el.display ?? 'text'} options={[{ value: 'text', label: 'Number' }, { value: 'bar', label: 'Bar' }, { value: 'gauge', label: 'Round gauge' }, { value: 'spark', label: 'Mini graph' }]} onChange={(v) => set('display', v === 'text' ? undefined : v)} />
        <PSeg label="Layout" value={el.arrange ?? 'inline'} options={[{ value: 'inline', label: 'Label | value' }, { value: 'stacked', label: 'Big number' }]} onChange={(v) => set('arrange', v === 'inline' ? undefined : v)} />
        {(el.display === 'bar' || el.display === 'spark') && <PNum label="Bar width" value={el.barWidth ?? 60} min={20} max={600} suffix="px" onChange={(v) => set('barWidth', v)} />}
      </Section>
      {picker && (
        <SensorPicker
          title={picker.kind === 'add' ? 'Add a value to this row' : 'Change measurement'}
          numericOnly={false}
          onClose={() => setPicker(null)}
          onPick={([id]) => {
            if (picker.kind === 'add') set('values', [...values, { sensorId: id, colorMode: 'status' }], `${el.id}:addv`);
            else set(`values.${picker.i}.sensorId`, id, `${el.id}:sensor${picker.i}`);
            setPicker(null);
          }}
        />
      )}
    </>
  );
}

// ───────────────────────────── graph ─────────────────────────────

function GraphProps({ el, set, catalog, advanced }: { el: LayoutEl; set: Setter; catalog: Record<string, SensorDesc>; advanced: boolean }) {
  const [picker, setPicker] = useState<{ kind: 'change'; i: number } | { kind: 'add' } | null>(null);
  const series = el.series ?? [];
  const palette = ['#3ddcff', '#ffb454', '#b79bff', '#7ee787'];
  return (
    <>
      <Section title="Graph">
        <PSeg label="Draws" value={el.source ?? 'history'} options={[{ value: 'history', label: 'Measurements over time' }, { value: 'frames', label: 'Frame times' }]} onChange={(v) => set('source', v, `${el.id}:source`)} />
        {el.source === 'frames' ? (
          <PNum label="Frames shown" value={el.frames ?? 240} min={30} max={1200} onChange={(v) => set('frames', v ?? 240)} help="Each frame is one bar of the line. Spikes show as tall peaks." />
        ) : (
          <>
            <PNum label="Time span" value={(el.seconds as number) ?? 60} min={10} max={300} suffix="seconds" onChange={(v) => set('seconds', v ?? 60)} />
            {series.map((s, i) => (
              <div key={i} className="row" style={{ gap: 6 }}>
                <input type="color" className="color-input" aria-label={`Colour of ${catalog[s.sensorId]?.name ?? s.sensorId}`} value={s.color ?? palette[i % 4]} onChange={(e) => set(`series.${i}.color`, e.target.value)} />
                <button type="button" className="btn sm" style={{ flex: 1, justifyContent: 'flex-start', overflow: 'hidden', textOverflow: 'ellipsis' }} onClick={() => setPicker({ kind: 'change', i })}>
                  {catalog[s.sensorId]?.name ?? s.sensorId}
                </button>
                {series.length > 1 && (
                  <Button size="sm" variant="ghost" icon aria-label="Remove line" onClick={() => set('series', series.filter((_, k) => k !== i))}>
                    <Trash2 size={14} />
                  </Button>
                )}
              </div>
            ))}
            {series.length < 4 && (
              <Button size="sm" onClick={() => setPicker({ kind: 'add' })}>
                <Plus size={14} /> Add a line
              </Button>
            )}
          </>
        )}
      </Section>
      <Section title="Look">
        <PText label="Title" value={el.title ?? ''} onChange={(v) => set('title', v)} />
        <PSwitch label="Show title" value={el.showLabel !== false} onChange={(v) => set('showLabel', v)} />
        <PSwitch label="Show value" value={!!el.showValue} onChange={(v) => set('showValue', v)} />
        <PSwitch label="Fill under line" value={!!el.fill} onChange={(v) => set('fill', v)} />
        <PSwitch label="Grid lines" value={!!el.grid} onChange={(v) => set('grid', v)} />
        {el.source === 'frames' && <PSwitch label="Target line" value={!!el.showTarget} onChange={(v) => set('showTarget', v)} help="Draws your game profile’s frame-time target." />}
        {advanced && (
          <>
            <PNum label="Line thickness" value={el.thickness ?? 1.6} min={0.5} max={6} step={0.1} onChange={(v) => set('thickness', v)} />
            <PNum label="Lowest value" value={el.min} placeholder="Automatic" onChange={(v) => set('min', v)} />
            <PNum label="Highest value" value={el.max} placeholder="Automatic" onChange={(v) => set('max', v)} />
          </>
        )}
      </Section>
      {picker && (
        <SensorPicker
          title={picker.kind === 'add' ? 'Add a line to the graph' : 'Change what the line shows'}
          numericOnly
          onClose={() => setPicker(null)}
          onPick={([id]) => {
            if (picker.kind === 'add') set('series', [...series, { sensorId: id, color: palette[series.length % 4] }]);
            else set(`series.${picker.i}.sensorId`, id);
            setPicker(null);
          }}
        />
      )}
    </>
  );
}

// ───────────────────────────── image ─────────────────────────────

function ImageProps({ el, set }: { el: LayoutEl; set: Setter }) {
  const [err, setErr] = useState<string | null>(null);
  const toast = useApp((s) => s.toast);
  return (
    <Section title="Image">
      <div className="row wrap">
        <Button
          onClick={async () => {
            const r = await openImageFile();
            if (!r) return;
            if ('error' in r) return setErr(r.error);
            setErr(null);
            set('src', r.dataUrl);
            toast({ level: 'success', message: 'Image added. It is stored inside the layout.' });
          }}
        >
          <Upload size={15} /> {el.src ? 'Replace image' : 'Choose image'}
        </Button>
      </div>
      {err && <Banner tone="warn">{err}</Banner>}
      <PSeg label="Fit" value={el.fit ?? 'contain'} options={[{ value: 'contain', label: 'Fit' }, { value: 'cover', label: 'Fill' }, { value: 'fill', label: 'Stretch' }]} onChange={(v) => set('fit', v)} />
      <PNum label="Width" value={el.w ?? 48} min={4} suffix="px" onChange={(v) => set('w', v ?? 48)} />
      <PNum label="Height" value={el.h ?? 48} min={4} suffix="px" onChange={(v) => set('h', v ?? 48)} />
      <p className="field-help">PNG, JPG, GIF, WebP or SVG under about 900 KB. The picture is saved inside the layout, so it travels when you export it.</p>
    </Section>
  );
}

// Re-exports used by the page.
export { Segmented, setPath, Prop };
