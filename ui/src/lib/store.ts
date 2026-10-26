import { create } from 'zustand';
import { rpc, subscribe, RpcError } from './bridge';
import { series } from './series';
import type {
  AlertLogItem, AlertRule, AppStateDto, EngineEvent, HardwareInfo, Layout, OverlaySpec, Profile, SensorDesc, Settings, Snapshot, StatusInfo, TrayState, UiToast,
} from './types';

export interface Toast extends UiToast {
  id: number;
  /** Optional undo handler (the toast shows an Undo button). */
  undo?: () => void | Promise<void>;
}

interface AppState {
  ready: boolean;
  connected: boolean;
  loadError: string | null;
  version: string;
  platform: string;
  demo: boolean;
  demoGameRunning: boolean;
  firstRun: boolean;
  settings: Settings;
  profiles: Profile[];
  layouts: Layout[];
  rules: AlertRule[];
  catalog: SensorDesc[];
  catalogById: Record<string, SensorDesc>;
  status: StatusInfo | null;
  notices: string[];
  hardware: HardwareInfo | null;
  paths: { config: string; data: string };
  live: Snapshot | null;
  overlays: OverlaySpec[];
  tray: TrayState | null;
  alertLog: AlertLogItem[];
  toasts: Toast[];
  sessionsVersion: number;
  screenshotsVersion: number;

  init: () => Promise<void>;
  applyEvent: (e: EngineEvent) => void;
  toast: (t: Partial<UiToast> & { message: string; level?: UiToast['level'] }, undo?: Toast['undo']) => void;
  dismissToast: (id: number) => void;
  updateSettings: (patch: Record<string, unknown>) => Promise<Settings | null>;
}

let toastId = 1;
let started = false;

function indexCatalog(c: SensorDesc[]): Record<string, SensorDesc> {
  const m: Record<string, SensorDesc> = {};
  for (const d of c) m[d.id] = d;
  return m;
}

/** Plain-language text for any thrown error. */
export function errorMessage(e: unknown): string {
  if (e instanceof RpcError) return e.message;
  if (e instanceof Error) return e.message;
  return 'Something went wrong.';
}

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  connected: true,
  loadError: null,
  version: '',
  platform: '',
  demo: false,
  demoGameRunning: false,
  firstRun: false,
  settings: undefined as unknown as Settings,
  profiles: [],
  layouts: [],
  rules: [],
  catalog: [],
  catalogById: {},
  status: null,
  notices: [],
  hardware: null,
  paths: { config: '', data: '' },
  live: null,
  overlays: [],
  tray: null,
  alertLog: [],
  toasts: [],
  sessionsVersion: 0,
  screenshotsVersion: 0,

  async init() {
    if (started) return;
    started = true;
    const load = async () => {
      const st = await rpc<AppStateDto>('app.getState');
      set({
        ready: true,
        loadError: null,
        version: st.version,
        platform: st.platform,
        demo: st.demo,
        demoGameRunning: !!st.demoGameRunning,
        firstRun: st.firstRun,
        settings: st.settings,
        profiles: st.profiles,
        layouts: st.layouts,
        rules: st.rules,
        catalog: st.catalog,
        catalogById: indexCatalog(st.catalog),
        status: st.status,
        notices: st.notices,
        hardware: st.hardware,
        paths: st.paths,
        live: st.snapshot,
        overlays: st.overlays,
        tray: st.tray,
      });
      const log = await rpc<AlertLogItem[]>('alerts.log').catch(() => []);
      set({ alertLog: log });
    };
    try {
      await load();
    } catch (e) {
      set({ loadError: errorMessage(e) });
      started = false;
      return;
    }
    subscribe(
      (e) => get().applyEvent(e),
      (connected) => {
        const was = get().connected;
        set({ connected });
        if (connected && !was) void load();
      },
    );
  },

  applyEvent(e) {
    switch (e.type) {
      case 'snapshot':
        series.push(e.data);
        set({ live: e.data, demoGameRunning: e.data.demo ? e.data.gameActive : get().demoGameRunning });
        break;
      case 'catalog':
        set({ catalog: e.data, catalogById: indexCatalog(e.data) });
        break;
      case 'settings':
        set({ settings: e.data });
        break;
      case 'layouts':
        set({ layouts: e.data });
        break;
      case 'profiles':
        set({ profiles: e.data });
        break;
      case 'rules':
        set({ rules: e.data });
        break;
      case 'sessions':
        set((s) => ({ sessionsVersion: s.sessionsVersion + 1 }));
        break;
      case 'screenshots':
        set((s) => ({ screenshotsVersion: s.screenshotsVersion + 1 }));
        break;
      case 'overlays':
        set({ overlays: e.data });
        break;
      case 'tray':
        set({ tray: e.data });
        break;
      case 'status':
        set({ status: e.data });
        break;
      case 'toast':
        get().toast(e.data);
        break;
      case 'alertLog':
        set((s) => ({ alertLog: [e.data, ...s.alertLog].slice(0, 200) }));
        break;
      default:
        break;
    }
  },

  toast(t, undo) {
    const id = toastId++;
    const toast: Toast = { id, level: t.level ?? 'info', message: t.message, action: t.action ?? null, undo };
    set((s) => ({ toasts: [...s.toasts.slice(-3), toast] }));
    setTimeout(() => get().dismissToast(id), undo ? 9000 : t.level === 'error' ? 9000 : 5000);
  },

  dismissToast(id) {
    set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
  },

  async updateSettings(patch) {
    try {
      const s = await rpc<Settings>('settings.update', { patch });
      set({ settings: s });
      return s;
    } catch (e) {
      get().toast({ level: 'error', message: errorMessage(e) });
      return null;
    }
  },
}));

// ── selectors ────────────────────────────────────────────────────────────

export function useSensor(id: string): SensorDesc | undefined {
  return useApp((s) => s.catalogById[id]);
}

export function currentProfile(profiles: Profile[], live: Snapshot | null): Profile | undefined {
  return profiles.find((p) => p.id === live?.profile.id) ?? profiles.find((p) => p.kind === 'default');
}

/** Run an engine call and report failure as a toast. Returns undefined on failure. */
export async function act<T = unknown>(method: string, params: Record<string, unknown> = {}, success?: string): Promise<T | undefined> {
  try {
    const r = await rpc<T>(method, params);
    if (success) useApp.getState().toast({ level: 'success', message: success });
    return r;
  } catch (e) {
    useApp.getState().toast({ level: 'error', message: errorMessage(e) });
    return undefined;
  }
}
