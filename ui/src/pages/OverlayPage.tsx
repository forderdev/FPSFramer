import { ArrowDown, ArrowUp, Check, Eye, Move, Pencil, Plus, TriangleAlert } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { LayoutPreview, SensorPicker } from '../components/shared';
import { Badge, Banner, Button, Card, CardHead, ColorField, Disclosure, Field, Kbd, PageHead, Segmented, Select, Setting, Slider, Switch, Tabs } from '../components/ui';
import { listMonitors, rpc, type MonitorInfo } from '../lib/bridge';
import { addMetrics, elementLabel, moveElement, sensorsOf, updateCanvas, setAccent, updateElement } from '../lib/layoutOps';
import { navigate } from '../lib/router';
import { act, currentProfile, errorMessage, useApp } from '../lib/store';
import type { Anchor, Layout, Placement } from '../lib/types';
import { FONT_OPTIONS } from '../overlay/fonts';

type OverlayTab = 'style' | 'position' | 'look' | 'content' | 'more';
const OVERLAY_TABS: { id: OverlayTab; label: string }[] = [
  { id: 'style', label: 'Style' },
  { id: 'position', label: 'Position' },
  { id: 'look', label: 'Look' },
  { id: 'content', label: 'What to show' },
  { id: 'more', label: 'More' },
];

const ANCHORS: Anchor[] = ['top-left', 'top-center', 'top-right', 'middle-left', 'center', 'middle-right', 'bottom-left', 'bottom-center', 'bottom-right'];
const ANCHOR_LABEL: Record<Anchor, string> = {
  'top-left': 'Top left',
  'top-center': 'Top centre',
  'top-right': 'Top right',
  'middle-left': 'Middle left',
  center: 'Centre',
  'middle-right': 'Middle right',
  'bottom-left': 'Bottom left',
  'bottom-center': 'Bottom centre',
  'bottom-right': 'Bottom right',
};

/** Edit the layout used by a placement. Built-in layouts are copied on the first change. */
function useLayoutEditing(layoutId: string | undefined, placementId: string | undefined) {
  const layouts = useApp((s) => s.layouts);
  const stored = layouts.find((l) => l.id === layoutId);
  const [draft, setDraft] = useState<Layout | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const ver = useRef(0);
  const [forked, setForked] = useState(false);

  useEffect(() => {
    setDraft(null);
    setForked(false);
  }, [layoutId]);

  const layout = draft && draft.id === layoutId ? draft : stored;

  const edit = async (fn: (l: Layout) => Layout) => {
    let base = draft && draft.id === layoutId ? draft : stored;
    if (!base) return;
    if (base.builtin) {
      try {
        const copy = await rpc<Layout>('layouts.duplicate', { id: base.id, name: `${base.name} (mine)` });
        await rpc('overlay.setLayout', { layoutId: copy.id, placementId });
        base = copy;
        setForked(true);
        useApp.getState().toast({ level: 'info', message: `“${stored?.name}” is built in, so FPSFramer made your own copy to edit.` });
      } catch (e) {
        useApp.getState().toast({ level: 'error', message: errorMessage(e) });
        return;
      }
    }
    const next = fn(base);
    setDraft(next);
    const my = ++ver.current;
    clearTimeout(timer.current);
    timer.current = setTimeout(async () => {
      try {
        await rpc('layouts.save', { layout: next });
        if (ver.current === my) setDraft(null);
      } catch (e) {
        useApp.getState().toast({ level: 'error', message: errorMessage(e) });
      }
    }, 350);
  };
  return { layout, edit, forked };
}

export default function OverlayPage() {
  const { layouts, settings, profiles, live } = useApp();
  const update = useApp((s) => s.updateSettings);
  const advanced = settings.uiMode === 'advanced';
  const profile = currentProfile(profiles, live);
  const scene = profile?.scenes.find((s) => s.id === profile.activeSceneId) ?? profile?.scenes[0];
  const placement = scene?.overlays.find((o) => o.kind === 'overlay');
  const { layout, edit } = useLayoutEditing(placement?.layoutId, placement?.id);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [picker, setPicker] = useState(false);
  const [tab, setTab] = useState<OverlayTab>('style');
  const catalog = useApp((s) => s.catalogById);
  const editMode = live?.editMode ?? false;

  useEffect(() => {
    void listMonitors().then(setMonitors);
  }, []);

  const presets = layouts.filter((l) => l.builtin && l.tags.includes('preset') && !l.tags.includes('template'));
  const mine = layouts.filter((l) => !l.builtin);

  const patchPlacement = async (patch: Partial<Placement>) => {
    if (!profile || !scene || !placement) return;
    const np = { ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: s.overlays.map((o) => (o.id === placement.id ? { ...o, ...patch } : o)) } : s)) };
    await act('profiles.save', { profile: np });
  };

  const addOverlay = async () => {
    if (!profile || !scene) return;
    const np = { ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: [...s.overlays, { id: '', layoutId: 'preset.gaming', kind: 'overlay', enabled: true, monitor: 'game', anchor: 'top-left', offsetX: 16, offsetY: 16, scale: 1, opacity: 1, clickThrough: true, visibility: 'game', alwaysOnTop: true } as Placement] } : s)) };
    await act('profiles.save', { profile: np });
  };

  const monitorOptions = useMemo(
    () => [
      { value: 'game', label: 'The screen the game is on (recommended)' },
      { value: 'primary', label: 'Main screen' },
      ...monitors.filter((m) => m.name !== 'primary').map((m) => ({ value: m.name, label: `${m.label} (${m.width}×${m.height})` })),
    ],
    [monitors],
  );

  return (
    <div className="page">
      <PageHead title="Overlay" sub="Pick how the numbers on top of your game look, where they sit and what they show. Changes apply instantly." />

      <Card>
        <div className="row wrap" style={{ gap: '1.25rem' }}>
          <label className="row" style={{ gap: 10 }}>
            <Switch label="Show the overlay in games" checked={settings.overlayEnabled} onChange={(v) => void update({ overlayEnabled: v })} />
            <b>Show the overlay in games</b>
          </label>
          <span className="muted">
            {settings.hotkeys.toggleOverlay ? (
              <>
                Hotkey: <Kbd>{settings.hotkeys.toggleOverlay}</Kbd>
              </>
            ) : (
              'No hotkey set'
            )}
          </span>
          <div className="spacer" />
          <Button onClick={() => void act('overlay.preview', { seconds: 12 })} title="Show the overlay on your desktop for 12 seconds">
            <Eye size={16} /> Preview on desktop
          </Button>
          <Button onClick={() => void act('overlay.setEdit', { on: !editMode })} aria-pressed={editMode} variant={editMode ? 'primary' : 'default'}>
            <Move size={16} /> {editMode ? 'Done moving' : 'Move overlay with the mouse'}
          </Button>
        </div>
        {editMode && (
          <Banner tone="info" className="" title="Move mode is on">
            Drag the overlay to where you want it, then press “Done moving” (or {settings.hotkeys.toggleEdit ? <Kbd>{settings.hotkeys.toggleEdit}</Kbd> : 'your move shortcut'}). It stays in the same place for this game profile.
          </Banner>
        )}
      </Card>

      {profile && scene && (
        <p className="muted" style={{ marginTop: '-0.4rem' }}>
          Editing <b style={{ color: 'var(--text)' }}>{profile.name}</b> · style set “{scene.name}”.{' '}
          <a href="#/profiles" onClick={(e) => (e.preventDefault(), navigate('/profiles'))}>
            Change games & style sets
          </a>
        </p>
      )}

      <Tabs value={tab} tabs={OVERLAY_TABS} onChange={setTab} />

      {tab === 'style' && (
        <>
      <div className="grid cols-3" role="radiogroup" aria-label="Overlay style">
        {presets
          .filter((l) => l.id !== 'preset.widget')
          .map((l) => (
            <StyleCard key={l.id} layout={l} active={placement?.layoutId === l.id} onPick={() => void act('overlay.setLayout', { layoutId: l.id, placementId: placement?.id })} />
          ))}
      </div>
      {mine.length > 0 && (
        <>
          <h3 className="muted" style={{ fontSize: '0.9rem', fontWeight: 650 }}>
            Your layouts
          </h3>
          <div className="grid cols-3" role="radiogroup" aria-label="Your layouts">
            {mine.map((l) => (
              <StyleCard key={l.id} layout={l} active={placement?.layoutId === l.id} onPick={() => void act('overlay.setLayout', { layoutId: l.id, placementId: placement?.id })} />
            ))}
          </div>
        </>
      )}
        </>
      )}

      {!placement && (
        <Banner tone="warn" title="This style set has no overlay yet" actions={<Button variant="primary" size="sm" onClick={() => void addOverlay()}><Plus size={14} /> Add overlay</Button>}>
          Add an overlay to show numbers during games.
        </Banner>
      )}

      {placement && layout && tab === 'position' && (
            <Card>
              <CardHead title="Position & size" />
              <div className="row top wrap" style={{ gap: '1.5rem' }}>
                <Field label="Where on screen" help={ANCHOR_LABEL[placement.anchor]}>
                  <div className="anchor-grid" role="group" aria-label="Screen position">
                    {ANCHORS.map((a) => (
                      <button key={a} type="button" aria-label={ANCHOR_LABEL[a]} aria-pressed={placement.anchor === a} onClick={() => void patchPlacement({ anchor: a, offsetX: 16, offsetY: 16 })} />
                    ))}
                  </div>
                </Field>
                <div className="stack" style={{ flex: 1, minWidth: '12rem' }}>
                  <Field label="Size">
                    <Slider label="Overlay size" value={placement.scale} min={0.5} max={2.5} step={0.05} onChange={(v) => void patchPlacement({ scale: v })} format={(v) => `${Math.round(v * 100)}%`} />
                  </Field>
                  <Field label="Transparency" help="Lower = see more of the game through the overlay">
                    <Slider label="Overlay opacity" value={placement.opacity} min={0.2} max={1} step={0.05} onChange={(v) => void patchPlacement({ opacity: v })} format={(v) => `${Math.round(v * 100)}%`} />
                  </Field>
                </div>
              </div>
              {monitors.length > 1 && (
                <div className="stack" style={{ marginTop: '0.75rem' }}>
                  <Field label="Which screen" help="Pick a specific screen if you play on one monitor and keep other things on another.">
                    <Select label="Screen" value={placement.monitor} options={monitorOptions} onChange={(v) => void patchPlacement({ monitor: v })} />
                  </Field>
                </div>
              )}
              {advanced && (
                <div className="row wrap" style={{ marginTop: '0.75rem' }}>
                  <Field label="Offset from the edge (px)">
                    <div className="row">
                      <input className="input sm" style={{ width: '5.5rem' }} type="number" aria-label="Horizontal offset" value={Math.round(placement.offsetX)} onChange={(e) => void patchPlacement({ offsetX: Number(e.target.value) })} />
                      <input className="input sm" style={{ width: '5.5rem' }} type="number" aria-label="Vertical offset" value={Math.round(placement.offsetY)} onChange={(e) => void patchPlacement({ offsetY: Number(e.target.value) })} />
                    </div>
                  </Field>
                </div>
              )}
            </Card>
      )}

      {placement && layout && tab === 'look' && (
            <Card>
              <CardHead title="Look" sub={layout.builtin ? 'Changing anything makes your own copy of this style.' : undefined} />
              <div className="stack">
                <Field label="Accent colour" help="Used for bars, graphs and highlights">
                  <ColorField swatches label="Accent colour" value={layout.canvas.accent} onChange={(c) => void edit((l) => setAccent(l, c))} />
                </Field>
                <Field label="Background darkness">
                  <Slider label="Background opacity" value={layout.canvas.background.opacity} min={0} max={1} step={0.05} onChange={(v) => void edit((l) => updateCanvas(l, 'background.opacity', v))} format={(v) => `${Math.round(v * 100)}%`} />
                </Field>
                <Field label="Text size">
                  <Slider label="Text size" value={layout.canvas.font.size} min={10} max={28} step={1} onChange={(v) => void edit((l) => updateCanvas(l, 'font.size', v))} format={(v) => `${v}px`} />
                </Field>
                <Field label="Font" help="Fonts must be installed on your PC (these are standard Windows fonts).">
                  <Select label="Font" value={layout.canvas.font.family} options={FONT_OPTIONS.map((f) => ({ value: f.value, label: f.label }))} onChange={(v) => void edit((l) => updateCanvas(l, 'font.family', v))} />
                </Field>
                <Field label="Make text easier to read on busy backgrounds">
                  <Segmented
                    label="Text outline"
                    value={layout.canvas.textShadow}
                    options={[
                      { value: 'none', label: 'None' },
                      { value: 'soft', label: 'Soft shadow' },
                      { value: 'outline', label: 'Outline' },
                    ]}
                    onChange={(v) => void edit((l) => updateCanvas(l, 'textShadow', v))}
                  />
                </Field>
              </div>
            </Card>
      )}

      {placement && layout && tab === 'content' && (
          <Card>
            <div className="row wrap" style={{ marginBottom: '0.6rem' }}>
              <p className="muted" style={{ flex: 1, margin: 0 }}>
                Turn rows on or off. Measurements your PC can’t provide are hidden automatically in games.
              </p>
              <Button onClick={() => navigate(`/editor/${layout.id}`)}>
                <Pencil size={16} /> Open visual editor
              </Button>
              <Button variant="primary" onClick={() => setPicker(true)}>
                <Plus size={16} /> Add measurement
              </Button>
            </div>
            <div className="stack" style={{ gap: 0 }}>
              {layout.elements.map((el, i) => {
                const missing = sensorsOf(el).filter((id) => catalog[id] && !catalog[id].availability.available);
                const allMissing = el.type !== 'text' && missing.length > 0 && missing.length === sensorsOf(el).length;
                return (
                  <div key={el.id} className="setting" style={{ padding: '0.55rem 0' }}>
                    <Switch label={`Show ${elementLabel(el, catalog)}`} checked={el.visible !== false} onChange={(v) => void edit((l) => updateElement(l, el.id, { visible: v }))} />
                    <div className="grow" style={{ opacity: el.visible === false ? 0.55 : 1 }}>
                      <span style={{ fontWeight: 600 }}>{elementLabel(el, catalog)}</span>
                      {allMissing && (
                        <>
                          {' '}
                          <Badge tone="warn">
                            <TriangleAlert size={12} aria-hidden /> Not available on this PC
                          </Badge>
                        </>
                      )}
                    </div>
                    <Button variant="ghost" size="sm" icon aria-label={`Move ${elementLabel(el, catalog)} up`} disabled={i === 0} onClick={() => void edit((l) => moveElement(l, el.id, -1))}>
                      <ArrowUp size={16} />
                    </Button>
                    <Button variant="ghost" size="sm" icon aria-label={`Move ${elementLabel(el, catalog)} down`} disabled={i === layout.elements.length - 1} onClick={() => void edit((l) => moveElement(l, el.id, 1))}>
                      <ArrowDown size={16} />
                    </Button>
                  </div>
                );
              })}
            </div>
            <p className="faint" style={{ marginTop: '0.75rem', fontSize: '0.8125rem' }}>
              Want to place things freely, add images, shapes and graphs? Use the visual editor.
            </p>
          </Card>
      )}

      {tab === 'more' && (
        <>
          {placement && (
            <Card>
              <CardHead title="Behaviour" />
              <Setting title="Let clicks pass through" desc="Recommended. Your mouse clicks go to the game, not the overlay.">
                <Switch label="Let clicks pass through the overlay" checked={placement.clickThrough} onChange={(v) => void patchPlacement({ clickThrough: v })} />
              </Setting>
            </Card>
          )}
          {advanced && profile && scene && <ScenesCard />}
          <WidgetCard />
        </>
      )}

      {picker && layout && (
        <SensorPicker
          multi
          title="Add to the overlay"
          onClose={() => setPicker(false)}
          onPick={(ids) => {
            setPicker(false);
            void edit((l) => addMetrics(l, ids, catalog));
          }}
        />
      )}
    </div>
  );
}

function StyleCard({ layout, active, onPick }: { layout: Layout; active: boolean; onPick: () => void }) {
  return (
    <button type="button" role="radio" aria-checked={active} className={`card interactive ${active ? 'selected' : ''}`} onClick={onPick} style={{ textAlign: 'left', color: 'inherit', display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={{ pointerEvents: 'none' }}>
        <LayoutPreview layout={layout} scale={0.72} minHeight={150} label={false} />
      </div>
      <div className="row">
        <b>{layout.name}</b>
        <span className="spacer" />
        {active && (
          <Badge tone="accent">
            <Check size={12} aria-hidden /> In use
          </Badge>
        )}
      </div>
      <span className="muted" style={{ fontSize: '0.8125rem' }}>
        {layout.description || 'Your own layout'}
      </span>
    </button>
  );
}

function ScenesCard() {
  const { profiles, live, layouts } = useApp();
  const profile = currentProfile(profiles, live);
  if (!profile) return null;
  const scene = profile.scenes.find((s) => s.id === profile.activeSceneId) ?? profile.scenes[0];
  if (!scene) return null;
  const save = (np: typeof profile) => void act('profiles.save', { profile: np });
  return (
    <Disclosure title="Several overlays at once" sub="Advanced">
      <p className="muted" style={{ margin: '0 0 0.75rem' }}>A style set can show several independent overlays at once, for example one per monitor.</p>
      <div className="stack">
        {scene.overlays
          .filter((o) => o.kind === 'overlay')
          .map((o) => (
            <div key={o.id} className="row wrap" style={{ gap: '0.75rem' }}>
              <Switch label="Enabled" checked={o.enabled} onChange={(v) => save({ ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: s.overlays.map((x) => (x.id === o.id ? { ...x, enabled: v } : x)) } : s)) })} />
              <div style={{ minWidth: '14rem', flex: 1 }}>
                <Select
                  label="Layout"
                  value={o.layoutId}
                  options={layouts.map((l) => ({ value: l.id, label: l.name }))}
                  onChange={(v) => save({ ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: s.overlays.map((x) => (x.id === o.id ? { ...x, layoutId: v } : x)) } : s)) })}
                />
              </div>
              <Select
                label="Position"
                value={o.anchor}
                options={ANCHORS.map((a) => ({ value: a, label: ANCHOR_LABEL[a] }))}
                onChange={(v) => save({ ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: s.overlays.map((x) => (x.id === o.id ? { ...x, anchor: v } : x)) } : s)) })}
              />
              <Button
                variant="danger"
                size="sm"
                disabled={scene.overlays.filter((x) => x.kind === 'overlay').length < 2}
                onClick={() => save({ ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: s.overlays.filter((x) => x.id !== o.id) } : s)) })}
              >
                Remove
              </Button>
            </div>
          ))}
        <div>
          <Button
            onClick={() =>
              save({ ...profile, scenes: profile.scenes.map((s) => (s.id === scene.id ? { ...s, overlays: [...s.overlays, { id: '', layoutId: 'preset.minimal', kind: 'overlay', enabled: true, monitor: 'game', anchor: 'top-right', offsetX: 16, offsetY: 16, scale: 1, opacity: 1, clickThrough: true, visibility: 'game', alwaysOnTop: true } as Placement] } : s)) })
            }
          >
            <Plus size={16} /> Add another overlay
          </Button>
        </div>
      </div>
    </Disclosure>
  );
}

function WidgetCard() {
  const { settings, layouts } = useApp();
  const update = useApp((s) => s.updateSettings);
  const w = settings.widgets[0];
  if (!w) return null;
  const patch = (p: Partial<Placement>) => void update({ widgets: settings.widgets.map((x, i) => (i === 0 ? { ...x, ...p } : x)) });
  return (
    <Disclosure title="Desktop widget" sub={w.enabled ? 'On' : 'Off'}>
      <Setting title="Show a small card on your desktop" desc="Always on, independent of games.">
        <Switch label="Show the desktop widget" checked={w.enabled} onChange={(v) => patch({ enabled: v })} />
      </Setting>
      <div className="row wrap" style={{ gap: '1rem', marginTop: '0.75rem' }}>
        <Field label="Layout">
          <Select label="Widget layout" value={w.layoutId} options={layouts.map((l) => ({ value: l.id, label: l.name }))} onChange={(v) => patch({ layoutId: v })} />
        </Field>
        <Field label="Corner">
          <Select label="Widget position" value={w.anchor} options={ANCHORS.map((a) => ({ value: a, label: ANCHOR_LABEL[a] }))} onChange={(v) => patch({ anchor: v })} />
        </Field>
        <Field label="Size">
          <Slider label="Widget size" value={w.scale} min={0.6} max={2} step={0.05} onChange={(v) => patch({ scale: v })} format={(v) => `${Math.round(v * 100)}%`} />
        </Field>
      </div>
    </Disclosure>
  );
}
