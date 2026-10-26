// The only place that knows whether we are inside the desktop app (Tauri) or a plain browser
// talking to the development server. Everything else uses `rpc()` and `subscribe()`.

import type { EngineEvent } from './types';

export class RpcError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function toError(e: unknown): RpcError {
  if (e instanceof RpcError) return e;
  if (e && typeof e === 'object' && 'message' in e) {
    const o = e as { code?: string; message: string };
    return new RpcError(o.code ?? 'failed', o.message);
  }
  if (typeof e === 'string') return new RpcError('failed', e);
  return new RpcError('failed', 'Something went wrong.');
}

export async function rpc<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      return (await invoke('rpc', { method, params })) as T;
    } catch (e) {
      throw toError(e);
    }
  }
  let res: Response;
  try {
    res = await fetch('/api/rpc', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ method, params }) });
  } catch {
    throw new RpcError('offline', 'Cannot reach the FPSFramer engine. Is it still running?');
  }
  const body = await res.json().catch(() => ({ error: { code: 'failed', message: 'The engine sent an unreadable answer.' } }));
  if (body.error) throw toError(body.error);
  return body.ok as T;
}

/** Subscribe to engine events. Returns an unsubscribe function. */
export function subscribe(handler: (e: EngineEvent) => void, onConnection?: (connected: boolean) => void): () => void {
  if (isTauri) {
    let un: (() => void) | undefined;
    let cancelled = false;
    import('@tauri-apps/api/event').then(({ listen }) =>
      listen<EngineEvent>('engine-event', (ev) => handler(ev.payload)).then((u) => {
        if (cancelled) u();
        else un = u;
      }),
    );
    onConnection?.(true);
    return () => {
      cancelled = true;
      un?.();
    };
  }
  const es = new EventSource('/api/events');
  es.onopen = () => onConnection?.(true);
  es.onerror = () => onConnection?.(false);
  es.onmessage = (m) => {
    try {
      handler(JSON.parse(m.data) as EngineEvent);
    } catch {
      /* ignore malformed event */
    }
  };
  return () => es.close();
}

export interface MonitorInfo {
  name: string;
  label: string;
  primary: boolean;
  width: number;
  height: number;
}

export async function listMonitors(): Promise<MonitorInfo[]> {
  if (isTauri) {
    try {
      return await rpcNative<MonitorInfo[]>('list_monitors');
    } catch {
      /* fall through */
    }
  }
  return [{ name: 'primary', label: 'Main screen', primary: true, width: window.screen.width, height: window.screen.height }];
}

/** Native shell commands that are not engine RPCs (windows, dialogs). No-ops in the browser. */
export async function rpcNative<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri) throw new RpcError('unsupported', 'Only available in the desktop app.');
  const { invoke } = await import('@tauri-apps/api/core');
  try {
    return (await invoke(cmd, args)) as T;
  } catch (e) {
    throw toError(e);
  }
}
