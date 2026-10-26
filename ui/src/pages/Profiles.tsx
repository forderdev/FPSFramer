import { Copy, Download, FolderOpen, Gamepad2, Globe, MonitorPlay, Pencil, Plus, Trash2, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Badge, Banner, Button, Card, CardHead, Chip, Disclosure, Empty, Field, InfoTip, Modal, NumberInput, PageHead, SearchBox, Setting, Switch, TextInput, confirmAction } from '../components/ui';
import { rpc } from '../lib/bridge';
import { openTextFile, saveTextFile } from '../lib/files';
import { clone, newId } from '../lib/labels';
import { navigate, useRoute } from '../lib/router';
import { act, useApp } from '../lib/store';
import type { Profile, Scene } from '../lib/types';

const FPS_PRESETS = [30, 40, 60, 75, 90, 120, 144, 165, 240];

export default function ProfilesPage() {
  const profiles = useApp((s) => s.profiles);
  const live = useApp((s) => s.live);
  const route = useRoute();
  const toast = useApp((s) => s.toast);
  const [q, setQ] = useState('');
  const [adding, setAdding] = useState(false);
  const selectedId = route.segments[1] ?? live?.profile.id ?? 'default';
  const selected = profiles.find((p) => p.id === selectedId) ?? profiles[0];

  const shown = useMemo(() => profiles.filter((p) => !q.trim() || `${p.name} ${p.executables.join(' ')}`.toLowerCase().includes(q.toLowerCase())), [profiles, q]);

  const importFile = async () => {
    const text = await openTextFile('.json');
    if (text === null) return;
    const r = await act<{ profile: Profile; warnings?: string[] }>('profiles.import', { json: text });
    if (!r) return;
    const p = r.profile ?? (r as unknown as Profile);
    toast({ level: r.warnings?.length ? 'warning' : 'success', message: r.warnings?.length ? `Imported “${p.name}”. ${r.warnings.join(' ')}` : `Imported “${p.name}”` });
    navigate(`/profiles/${p.id}`);
  };

  return (
    <div className="page">
      <PageHead
        title="Games & profiles"
        sub="A profile remembers how FPSFramer should look and behave for one game. FPSFramer switches to it automatically when it recognises the game."
        actions={
          <>
            <Button onClick={() => void importFile()}>
              <FolderOpen size={16} /> Import…
            </Button>
            <Button variant="primary" onClick={() => setAdding(true)}>
              <Plus size={16} /> Add a game
            </Button>
          </>
        }
      />

      <div className="profiles-grid">
        <div className="stack">
          <SearchBox value={q} onChange={setQ} placeholder="Search games" label="Search games" />
          <Card style={{ padding: '0.35rem' }}>
            <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
              {shown.map((p) => (
                <li key={p.id}>
                  <button type="button" className={`nav-item ${p.id === selected?.id ? 'active' : ''}`} style={{ width: '100%' }} onClick={() => navigate(`/profiles/${p.id}`)} aria-current={p.id === selected?.id ? 'true' : undefined}>
                    {p.kind === 'default' ? <Globe size={18} aria-hidden /> : <Gamepad2 size={18} aria-hidden />}
                    <span className="grow" style={{ textAlign: 'left', minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {p.name}
                    </span>
                    {live?.profile.id === p.id && <Badge tone="ok">In use</Badge>}
                  </button>
                </li>
              ))}
              {shown.length === 0 && <li className="muted" style={{ padding: '0.75rem' }}>No games match “{q}”.</li>}
            </ul>
          </Card>
        </div>

        {selected ? (
          <ProfileEditor key={selected.id} profile={selected} />
        ) : (
          <Card>
            <Empty icon={<Gamepad2 size={36} />} title="No profile selected" />
          </Card>
        )}
      </div>

      {adding && <AddGame onClose={() => setAdding(false)} />}
      <style>{`.profiles-grid{display:grid;grid-template-columns:18rem minmax(0,1fr);gap:1rem;align-items:start}@media (max-width:900px){.profiles-grid{grid-template-columns:1fr}}`}</style>
    </div>
  );
}

// ───────────────────────────── editor ─────────────────────────────

function ProfileEditor({ profile }: { profile: Profile }) {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const toast = useApp((s) => s.toast);
  const live = useApp((s) => s.live);
  const isDefault = profile.kind === 'default';
  const [name, setName] = useState(profile.name);
  const [notes, setNotes] = useState(profile.notes);
  const [exeText, setExeText] = useState('');
  const [picking, setPicking] = useState(false);
  const [renamingScene, setRenamingScene] = useState<Scene | null>(null);

  useEffect(() => {
    setName(profile.name);
    setNotes(profile.notes);
  }, [profile.id, profile.name, profile.notes]);

  const save = async (next: Profile, message?: string) => act<Profile>('profiles.save', { profile: next }, message);

  const addExe = async (raw: string) => {
    const exe = raw.trim().replace(/^.*[\\/]/, '').toLowerCase();
    if (!exe) return;
    const withExt = /\.[a-z0-9]{2,4}$/.test(exe) ? exe : `${exe}.exe`;
    if (profile.executables.includes(withExt)) return setExeText('');
    const r = await save({ ...profile, executables: [...profile.executables, withExt] });
    if (r) setExeText('');
  };

  const removeExe = (exe: string) => void save({ ...profile, executables: profile.executables.filter((e) => e !== exe) });

  const setTarget = (v: number | null) => void save({ ...profile, targetFps: v });

  const duplicate = async () => {
    const c = await act<Profile>('profiles.duplicate', { id: profile.id });
    if (c) {
      toast({ level: 'success', message: `Created “${c.name}”. Give it its own program name to use it.` });
      navigate(`/profiles/${c.id}`);
    }
  };

  const exportIt = async () => {
    const r = await act<{ filename: string; json: string }>('profiles.export', { id: profile.id });
    if (r && (await saveTextFile(r.filename, r.json))) toast({ level: 'success', message: 'Profile exported, including its layouts.' });
  };

  const remove = async () => {
    const ok = await confirmAction({
      title: `Delete “${profile.name}”?`,
      body: 'The game will use the “All games” profile instead. Its layouts are kept. This can’t be undone, but you can export it first.',
      confirmLabel: 'Delete profile',
      danger: true,
    });
    if (!ok) return;
    const copy = clone(profile);
    const r = await act('profiles.delete', { id: profile.id });
    if (r === undefined) return;
    navigate('/profiles');
    toast({ level: 'info', message: `Deleted “${profile.name}”` }, async () => {
      await act('profiles.save', { profile: { ...copy, id: '' } }, `Restored “${profile.name}”`);
    });
  };

  // ── scenes ──
  const addScene = () => {
    const base = profile.scenes.find((s) => s.id === profile.activeSceneId) ?? profile.scenes[0];
    const sc: Scene = { id: newId('sc'), name: `Style ${profile.scenes.length + 1}`, overlays: (base?.overlays ?? []).map((o) => ({ ...clone(o), id: newId('pl') })) };
    void save({ ...profile, scenes: [...profile.scenes, sc] }, 'Style set added');
  };
  const dupScene = (s: Scene) => {
    const sc: Scene = { id: newId('sc'), name: `${s.name} (copy)`, overlays: s.overlays.map((o) => ({ ...clone(o), id: newId('pl') })) };
    void save({ ...profile, scenes: [...profile.scenes, sc] });
  };
  const delScene = async (s: Scene) => {
    if (profile.scenes.length < 2) return;
    const ok = await confirmAction({ title: `Delete the style set “${s.name}”?`, body: 'Its overlays are removed from this profile. The layouts themselves are kept.', confirmLabel: 'Delete', danger: true });
    if (!ok) return;
    const scenes = profile.scenes.filter((x) => x.id !== s.id);
    void save({ ...profile, scenes, activeSceneId: profile.activeSceneId === s.id ? scenes[0].id : profile.activeSceneId });
  };

  const pinned = settings.pinnedProfile === profile.id;

  return (
    <div className="stack">
      <Card>
        <div className="stack">
          <div className="row wrap" style={{ justifyContent: 'flex-end', gap: '0.5rem' }}>
            <Button size="sm" onClick={() => void duplicate()}>
              <Copy size={14} /> Duplicate
            </Button>
            <Button size="sm" onClick={() => void exportIt()}>
              <Download size={14} /> Export…
            </Button>
            {!isDefault && (
              <Button size="sm" variant="danger" onClick={() => void remove()}>
                <Trash2 size={14} /> Delete
              </Button>
            )}
          </div>
          <div className="row wrap" style={{ gap: '0.75rem' }}>
            <div style={{ flex: 1, minWidth: '14rem' }}>
              <Field label="Name">
                <TextInput
                  label="Profile name"
                  value={name}
                  onChange={setName}
                  maxLength={60}
                  onEnter={() => name.trim() && name !== profile.name && void save({ ...profile, name: name.trim() })}
                />
              </Field>
            </div>
            <Button disabled={!name.trim() || name === profile.name} onClick={() => void save({ ...profile, name: name.trim() }, 'Renamed')}>
              <Pencil size={14} /> Rename
            </Button>
          </div>

          {isDefault ? (
            <p className="muted" style={{ margin: 0 }}>
              This profile is used for every game that has no profile of its own, and while you use the desktop.
            </p>
          ) : (
            <Field
              label="Programs this profile applies to"
              tip="The name of the game’s program file, for example “cs2.exe”. FPSFramer switches to this profile when one of these programs is running."
              help="Tip: start the game first, then use “Pick a running program”. It’s the easiest way to get the name right."
            >
              <div className="stack" style={{ gap: '0.5rem' }}>
                <div className="row wrap">
                  {profile.executables.map((e) => (
                    <span key={e} className="chip on" style={{ cursor: 'default' }}>
                      {e}
                      <button type="button" className="x" aria-label={`Remove ${e}`} onClick={() => removeExe(e)}>
                        <X size={13} aria-hidden />
                      </button>
                    </span>
                  ))}
                  {profile.executables.length === 0 && (
                    <Badge tone="warn">No program yet. This profile will never switch on by itself</Badge>
                  )}
                </div>
                <div className="row wrap">
                  <div style={{ width: '14rem' }}>
                    <TextInput label="Add a program name" value={exeText} onChange={setExeText} placeholder="game.exe" onEnter={() => void addExe(exeText)} />
                  </div>
                  <Button onClick={() => void addExe(exeText)} disabled={!exeText.trim()}>
                    <Plus size={14} /> Add
                  </Button>
                  <Button onClick={() => setPicking(true)}>
                    <MonitorPlay size={14} /> Pick a running program
                  </Button>
                </div>
              </div>
            </Field>
          )}

          <Field label="Performance goal" tip="Your FPS target. Overlays, the Home page and session reports show how often you stayed on target.">
            <div className="row wrap">
              <Chip on={profile.targetFps === null} onClick={() => setTarget(null)}>
                No target
              </Chip>
              {FPS_PRESETS.map((f) => (
                <Chip key={f} on={profile.targetFps === f} onClick={() => setTarget(f)}>
                  {f}
                </Chip>
              ))}
              <NumberInput label="Custom FPS target" value={profile.targetFps ?? 60} min={10} max={1000} suffix="FPS" onChange={(v) => setTarget(Math.round(v))} />
            </div>
          </Field>
        </div>
      </Card>

      <Card>
        <CardHead
          title={
            <span className="row" style={{ gap: '0.4rem' }}>
              Style sets <InfoTip text="A style set (also called a scene) is a group of overlays. Make one for each way you like to play, for example “Minimal” for gaming and “Detailed” for testing, and switch between them with a hotkey, even in-game." />
            </span>
          }
          sub="Choose which one is used by default. Each style set has its own overlays."
          right={
            <Button size="sm" onClick={addScene}>
              <Plus size={14} /> Add style set
            </Button>
          }
        />
        <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
          {profile.scenes.map((s) => {
            const active = s.id === profile.activeSceneId;
            return (
              <li key={s.id} className="setting">
                <div className="grow">
                  <div className="row wrap" style={{ gap: '0.5rem' }}>
                    <b>{s.name}</b>
                    {active && <Badge tone="accent">Default</Badge>}
                  </div>
                  <div className="setting-desc">
                    {s.overlays.filter((o) => o.kind === 'overlay').length} overlay{s.overlays.filter((o) => o.kind === 'overlay').length === 1 ? '' : 's'}
                  </div>
                </div>
                <div className="ctrl">
                  {!active && (
                    <Button size="sm" onClick={() => void save({ ...profile, activeSceneId: s.id })}>
                      Make default
                    </Button>
                  )}
                  <Button size="sm" icon variant="ghost" aria-label={`Rename ${s.name}`} title="Rename" onClick={() => setRenamingScene(s)}>
                    <Pencil size={15} />
                  </Button>
                  <Button size="sm" icon variant="ghost" aria-label={`Duplicate ${s.name}`} title="Duplicate" onClick={() => dupScene(s)}>
                    <Copy size={15} />
                  </Button>
                  <Button size="sm" icon variant="ghost" aria-label={`Delete ${s.name}`} title={profile.scenes.length < 2 ? 'A profile needs at least one style set' : 'Delete'} disabled={profile.scenes.length < 2} onClick={() => void delScene(s)}>
                    <Trash2 size={15} />
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
        <p className="faint" style={{ marginBottom: 0, fontSize: '0.8125rem' }}>
          To change what a style set shows, where it sits or which screen it appears on, use the{' '}
          <a
            href="#/overlay"
            onClick={(e) => {
              e.preventDefault();
              navigate('/overlay');
            }}
          >
            Overlay page
          </a>
          {live?.profile.id !== profile.id && ' (it edits the profile that is currently in use, so turn on “Use this profile now” below to edit this one)'}.
        </p>
      </Card>

      <Disclosure title="More options" sub="Notes, use this profile now">
        <div className="stack">
          {!isDefault && (
            <Setting title="Use this profile now" desc="Keep using this profile even when its game isn’t running. Handy for setting up the overlay before you play. Turn it off to go back to automatic switching.">
              <Switch checked={pinned} label="Use this profile now" onChange={(v) => void updateSettings({ pinnedProfile: v ? profile.id : null })} />
            </Setting>
          )}
          <Field label="Notes (optional)">
            <textarea
              className="input"
              rows={2}
              aria-label="Notes"
              value={notes}
              maxLength={2000}
              placeholder="e.g. Plays best at High settings with DLSS on"
              onChange={(e) => setNotes(e.target.value)}
              onBlur={() => notes !== profile.notes && void save({ ...profile, notes })}
              style={{ resize: 'vertical' }}
            />
          </Field>
        </div>
      </Disclosure>

      <FrameCapCard profile={profile} />

      {picking && (
        <PickProcess
          taken={profile.executables}
          onClose={() => setPicking(false)}
          onPick={(exe) => {
            setPicking(false);
            void addExe(exe);
          }}
        />
      )}
      {renamingScene && (
        <RenameScene
          scene={renamingScene}
          onClose={() => setRenamingScene(null)}
          onSave={(n) => {
            setRenamingScene(null);
            void save({ ...profile, scenes: profile.scenes.map((s) => (s.id === renamingScene.id ? { ...s, name: n } : s)) });
          }}
        />
      )}
    </div>
  );
}

// ───────────────────────────── frame-rate cap helper ─────────────────────────────

function FrameCapCard({ profile }: { profile: Profile }) {
  const target = profile.targetFps;
  return (
    <Disclosure title="Capping your frame rate" sub="FPSFramer measures, it doesn’t limit">
      <div className="stack" style={{ gap: '0.6rem' }}>
        <p className="muted" style={{ margin: 0 }}>
          A built-in frame limiter would have to hook into the game, which anti-cheat software can treat as tampering, so FPSFramer leaves that to tools designed for it.
          A cap just below your screen’s refresh rate usually gives the smoothest result{target ? ` (you set a target of ${target} FPS)` : ''}. You can set one in:
        </p>
        <ul className="muted" style={{ margin: 0, paddingLeft: '1.2rem' }}>
          <li>the game’s own graphics options (look for “Max FPS” or “Frame rate limit”), the safest choice,</li>
          <li>NVIDIA Control Panel → “Max Frame Rate”, or AMD Software → “Frame Rate Target Control”,</li>
          <li>V-Sync or G-Sync / FreeSync, which hold the game to your monitor’s refresh rate.</li>
        </ul>
        <div>
          <Button onClick={() => void act('system.openUrl', { url: 'ms-settings:display-advancedgraphics' })}>
            <MonitorPlay size={15} /> Open Windows graphics settings
          </Button>
        </div>
      </div>
    </Disclosure>
  );
}

// ───────────────────────────── dialogs ─────────────────────────────

function RenameScene({ scene, onClose, onSave }: { scene: Scene; onClose: () => void; onSave: (name: string) => void }) {
  const [n, setN] = useState(scene.name);
  const go = () => n.trim() && onSave(n.trim());
  return (
    <Modal
      title="Rename style set"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={go} disabled={!n.trim()}>
            Rename
          </Button>
        </>
      }
    >
      <TextInput label="Style set name" value={n} onChange={setN} maxLength={40} onEnter={go} autoFocus />
    </Modal>
  );
}

interface ProcRow {
  exe: string;
  presenting: boolean;
}

function PickProcess({ taken, onPick, onClose }: { taken: string[]; onPick: (exe: string) => void; onClose: () => void }) {
  const [list, setList] = useState<ProcRow[] | null>(null);
  const [q, setQ] = useState('');
  const [failed, setFailed] = useState(false);
  const load = () => {
    setList(null);
    setFailed(false);
    rpc<ProcRow[]>('system.processes')
      .then(setList)
      .catch(() => setFailed(true));
  };
  useEffect(load, []);
  const rows = (list ?? []).filter((r) => !taken.includes(r.exe) && (!q.trim() || r.exe.includes(q.toLowerCase())));
  rows.sort((a, b) => Number(b.presenting) - Number(a.presenting) || a.exe.localeCompare(b.exe));
  return (
    <Modal
      title="Pick a running program"
      onClose={onClose}
      footer={
        <>
          <Button onClick={load}>Refresh</Button>
          <Button onClick={onClose}>Cancel</Button>
        </>
      }
    >
      <div className="stack">
        <p className="muted" style={{ margin: 0 }}>
          Start your game, then choose it here. The program FPSFramer is measuring right now is listed first.
        </p>
        <SearchBox value={q} onChange={setQ} placeholder="Filter programs" label="Filter programs" />
        {failed && <Banner tone="bad">The list of running programs couldn’t be read.</Banner>}
        {list === null && !failed && <div className="skeleton" style={{ height: 120 }} />}
        <div style={{ maxHeight: '45vh', overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 4 }}>
          {rows.map((r) => (
            <button key={r.exe} type="button" className="choice" style={{ padding: '0.55rem 0.8rem' }} onClick={() => onPick(r.exe)}>
              <span style={{ flex: 1, fontWeight: 600 }}>{r.exe}</span>
              {r.presenting && <Badge tone="ok">Being measured</Badge>}
            </button>
          ))}
          {list !== null && rows.length === 0 && <p className="muted">No other programs found{q ? ` matching “${q}”` : ''}.</p>}
        </div>
      </div>
    </Modal>
  );
}

function AddGame({ onClose }: { onClose: () => void }) {
  const profiles = useApp((s) => s.profiles);
  const [name, setName] = useState('');
  const [exe, setExe] = useState('');
  const [picking, setPicking] = useState(false);
  const def = profiles.find((p) => p.kind === 'default');

  const create = async () => {
    if (!def || !name.trim()) return;
    const p = clone(def);
    p.id = '';
    p.kind = 'game';
    p.name = name.trim();
    const e = exe.trim().replace(/^.*[\\/]/, '').toLowerCase();
    p.executables = e ? [/\.[a-z0-9]{2,4}$/.test(e) ? e : `${e}.exe`] : [];
    p.notes = '';
    const ids = new Map<string, string>();
    p.scenes.forEach((s) => {
      const nid = newId('sc');
      ids.set(s.id, nid);
      s.id = nid;
      s.overlays.forEach((o) => (o.id = newId('pl')));
    });
    p.activeSceneId = ids.get(def.activeSceneId) ?? p.scenes[0]?.id ?? '';
    const saved = await act<Profile>('profiles.save', { profile: p }, `Profile created for ${p.name}`);
    if (saved) {
      onClose();
      navigate(`/profiles/${saved.id}`);
    }
  };

  return (
    <Modal
      title="Add a game"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={() => void create()} disabled={!name.trim()}>
            Create profile
          </Button>
        </>
      }
    >
      <div className="stack">
        <p className="muted" style={{ margin: 0 }}>
          The new profile starts as a copy of “All games”, so the overlay looks the same. Then you can change anything for this game only.
        </p>
        <Field label="Game name">
          <TextInput label="Game name" value={name} onChange={setName} maxLength={60} autoFocus placeholder="e.g. Cyberpunk 2077" onEnter={() => void create()} />
        </Field>
        <Field label="Program name (optional)" help="Looks like “game.exe”. You can add it later.">
          <div className="row">
            <TextInput label="Program name" value={exe} onChange={setExe} placeholder="game.exe" onEnter={() => void create()} />
            <Button onClick={() => setPicking(true)}>Pick running…</Button>
          </div>
        </Field>
      </div>
      {picking && (
        <PickProcess
          taken={[]}
          onClose={() => setPicking(false)}
          onPick={(e) => {
            setPicking(false);
            setExe(e);
            if (!name.trim()) setName(e.replace(/\.exe$/, ''));
          }}
        />
      )}
    </Modal>
  );
}
