// The content of one overlay window. Kept tiny on purpose: it loads only the renderer.

import { useEffect, useMemo, useRef } from 'react';
import { isTauri, rpcNative } from '../lib/bridge';
import { useApp } from '../lib/store';
import { LayoutRenderer, type RenderCtx } from './LayoutRenderer';

export function OverlayApp() {
  const id = new URLSearchParams(window.location.search).get('id') ?? '';
  const { ready, init, overlays, layouts, catalogById, live, settings } = useApp();
  const wrap = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void init();
  }, [init]);

  const spec = overlays.find((o) => o.id === id);
  const layout = layouts.find((l) => l.id === spec?.layoutId);

  const ctx: RenderCtx | null = useMemo(
    () =>
      settings
        ? {
            catalog: catalogById,
            values: live?.values ?? {},
            stats: live?.stats ?? {},
            settings,
            alerts: live?.alerts ?? [],
            targetFps: live?.profile.targetFps ?? null,
            sample: false,
          }
        : null,
    [catalogById, live, settings],
  );

  // Tell the native shell how big the content is so the window fits it exactly.
  useEffect(() => {
    const el = wrap.current;
    if (!el || !isTauri) return;
    let last = '';
    const report = () => {
      const r = el.getBoundingClientRect();
      const key = `${Math.ceil(r.width)}x${Math.ceil(r.height)}`;
      if (key !== last && r.width > 0) {
        last = key;
        void rpcNative('overlay_size', { id, width: Math.ceil(r.width), height: Math.ceil(r.height) }).catch(() => undefined);
      }
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, [id, layout?.id, spec?.scale, ready]);

  if (!ready || !spec || !layout || !ctx) return null;
  const edit = spec.editMode && spec.kind === 'overlay';
  const startDrag = async () => {
    if (!edit || !isTauri) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().startDragging();
  };

  return (
    <div ref={wrap} style={{ display: 'inline-block' }}>
      <div className={edit ? 'ov-edit-frame' : undefined} onPointerDown={edit ? () => void startDrag() : undefined}>
        <LayoutRenderer layout={layout} ctx={ctx} scale={spec.scale} opacity={spec.opacity} />
      </div>
      {edit && <span className="ov-edit-chip">Drag to move · press the shortcut again to lock</span>}
      {live?.demo && <span className="ov-demo-chip">DEMO DATA</span>}
    </div>
  );
}
