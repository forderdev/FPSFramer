import { Copy, Download, FolderOpen, LayoutTemplate, Pencil, Plus, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { LayoutPreview } from '../components/shared';
import { Badge, Button, Card, Empty, Field, Modal, PageHead, SearchBox, Segmented, Select, TextInput, confirmAction } from '../components/ui';
import { openTextFile, saveTextFile } from '../lib/files';
import { navigate } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { Layout } from '../lib/types';

function usageOf(id: string, profiles: ReturnType<typeof useApp.getState>['profiles'], widgets: { layoutId: string }[]): string[] {
  const out: string[] = [];
  for (const p of profiles) for (const s of p.scenes) if (s.overlays.some((o) => o.layoutId === id)) out.push(`${p.name} › ${s.name}`);
  if (widgets.some((w) => w.layoutId === id)) out.push('Desktop widget');
  return out;
}

export default function LayoutsPage() {
  const { layouts, profiles, settings, toast } = useApp();
  const [filter, setFilter] = useState<'all' | 'builtin' | 'mine'>('all');
  const [q, setQ] = useState('');
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState<Layout | null>(null);

  const shown = useMemo(
    () =>
      layouts.filter((l) => (filter === 'all' || (filter === 'builtin' ? l.builtin : !l.builtin)) && (!q.trim() || `${l.name} ${l.description}`.toLowerCase().includes(q.toLowerCase()))),
    [layouts, filter, q],
  );

  const importFile = async () => {
    const text = await openTextFile('.json');
    if (text === null) return;
    const l = await act<Layout>('layouts.import', { json: text });
    if (l) toast({ level: 'success', message: `Imported “${l.name}”`, action: { label: 'Open in editor', route: `/editor/${l.id}` } });
  };

  const duplicate = async (l: Layout) => {
    const c = await act<Layout>('layouts.duplicate', { id: l.id });
    if (c) toast({ level: 'success', message: `Created “${c.name}”`, action: { label: 'Edit', route: `/editor/${c.id}` } });
  };

  const exportOne = async (l: Layout) => {
    const r = await act<{ filename: string; json: string }>('layouts.export', { id: l.id });
    if (r && (await saveTextFile(r.filename, r.json))) toast({ level: 'success', message: 'Layout exported. Share the file with anyone. They can import it on their PC.' });
  };

  const remove = async (l: Layout) => {
    const used = usageOf(l.id, profiles, settings.widgets);
    if (used.length) {
      const ok = await confirmAction({
        title: `Delete “${l.name}”?`,
        body: (
          <>
            It is used by {used.join(', ')}. Those overlays will switch to the built-in “Gaming” style.
          </>
        ),
        confirmLabel: 'Delete layout',
        danger: true,
      });
      if (!ok) return;
    }
    const copy = JSON.parse(JSON.stringify(l)) as Layout;
    const res = await act('layouts.delete', { id: l.id });
    if (res === undefined) return;
    toast({ level: 'info', message: `Deleted “${l.name}”` }, async () => {
      await act('layouts.save', { layout: copy }, `Restored “${l.name}”`);
    });
  };

  return (
    <div className="page">
      <PageHead
        title="Layouts"
        sub="Overlay designs. The built-in styles can’t be changed, but you can make your own copy of any of them and edit it freely."
        actions={
          <>
            <Button onClick={() => void importFile()}>
              <FolderOpen size={16} /> Import…
            </Button>
            <Button variant="primary" onClick={() => setCreating(true)}>
              <Plus size={16} /> New layout
            </Button>
          </>
        }
      />
      <div className="row wrap">
        <SearchBox value={q} onChange={setQ} placeholder="Search layouts" label="Search layouts" />
        <Segmented
          label="Show"
          value={filter}
          options={[
            { value: 'all', label: `All (${layouts.length})` },
            { value: 'builtin', label: 'Built-in' },
            { value: 'mine', label: `Mine (${layouts.filter((l) => !l.builtin).length})` },
          ]}
          onChange={setFilter}
        />
      </div>

      {shown.length === 0 ? (
        <Empty icon={<LayoutTemplate size={36} />} title={filter === 'mine' && !q ? 'You haven’t made a layout yet' : 'No layouts match'} action={filter === 'mine' && !q ? <Button variant="primary" onClick={() => setCreating(true)}>Create your first layout</Button> : undefined}>
          {filter === 'mine' && !q ? 'Start from one of the built-in styles and change what you like.' : 'Try a different search.'}
        </Empty>
      ) : (
        <div className="grid cols-3">
          {shown.map((l) => {
            const used = usageOf(l.id, profiles, settings.widgets);
            return (
              <Card key={l.id} className="stack" style={{ gap: '0.7rem' }}>
                <LayoutPreview layout={l} scale={0.75} minHeight={150} />
                <div className="row wrap" style={{ gap: '0.5rem' }}>
                  <b style={{ fontSize: '1.02rem' }}>{l.name}</b>
                  {l.builtin ? <Badge>Built-in</Badge> : <Badge tone="accent">Mine</Badge>}
                  {l.canvas.arrangement === 'free' && <Badge>Free placement</Badge>}
                </div>
                <div className="muted" style={{ fontSize: '0.8125rem', minHeight: '2.4em' }}>
                  {l.description || (l.elements.length ? `${l.elements.length} items` : 'Empty layout')}
                </div>
                {used.length > 0 && (
                  <div className="faint" style={{ fontSize: '0.75rem' }}>
                    In use: {used.slice(0, 2).join(', ')}
                    {used.length > 2 && ` +${used.length - 2}`}
                  </div>
                )}
                <div className="row wrap" style={{ gap: '0.4rem' }}>
                  <Button size="sm" variant="primary" onClick={() => navigate(`/editor/${l.id}`)}>
                    <Pencil size={14} /> {l.builtin ? 'Customize' : 'Edit'}
                  </Button>
                  <Button size="sm" onClick={() => void duplicate(l)} title="Make a copy">
                    <Copy size={14} /> Duplicate
                  </Button>
                  <Button size="sm" icon aria-label={`Export ${l.name}`} title="Export to a file" onClick={() => void exportOne(l)}>
                    <Download size={14} />
                  </Button>
                  {!l.builtin && (
                    <>
                      <Button size="sm" icon aria-label={`Rename ${l.name}`} title="Rename" onClick={() => setRenaming(l)}>
                        <Pencil size={14} />
                      </Button>
                      <Button size="sm" variant="danger" icon aria-label={`Delete ${l.name}`} title="Delete" onClick={() => void remove(l)}>
                        <Trash2 size={14} />
                      </Button>
                    </>
                  )}
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {creating && <NewLayout onClose={() => setCreating(false)} />}
      {renaming && <Rename layout={renaming} onClose={() => setRenaming(null)} />}
    </div>
  );
}

function NewLayout({ onClose }: { onClose: () => void }) {
  const layouts = useApp((s) => s.layouts);
  const [base, setBase] = useState('preset.gaming');
  const [name, setName] = useState('My layout');
  const builtin = layouts.filter((l) => l.builtin);
  const create = async () => {
    const c = await act<Layout>('layouts.duplicate', { id: base, name: name.trim() || 'My layout' });
    if (c) {
      onClose();
      navigate(`/editor/${c.id}`);
    }
  };
  return (
    <Modal
      title="New layout"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={() => void create()}>
            Create and edit
          </Button>
        </>
      }
    >
      <div className="stack">
        <Field label="Name">
          <TextInput label="Layout name" value={name} onChange={setName} maxLength={60} onEnter={() => void create()} autoFocus />
        </Field>
        <Field label="Start from" help="You can change everything afterwards. “Blank canvas” lets you place items anywhere.">
          <Select label="Start from" value={base} options={builtin.map((l) => ({ value: l.id, label: l.name }))} onChange={setBase} />
        </Field>
      </div>
    </Modal>
  );
}

function Rename({ layout, onClose }: { layout: Layout; onClose: () => void }) {
  const [name, setName] = useState(layout.name);
  const save = async () => {
    const r = await act('layouts.save', { layout: { ...layout, name: name.trim() || layout.name } });
    if (r) onClose();
  };
  return (
    <Modal
      title="Rename layout"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={() => void save()}>
            Rename
          </Button>
        </>
      }
    >
      <TextInput label="Layout name" value={name} onChange={setName} maxLength={60} onEnter={() => void save()} autoFocus />
    </Modal>
  );
}
