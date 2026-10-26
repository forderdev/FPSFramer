import { useCallback, useRef, useState } from 'react';
import type { Layout } from '../lib/types';

interface HState {
  draft: Layout;
  past: Layout[];
  future: Layout[];
}

const LIMIT = 100;

/** Editing state with undo/redo. Rapid edits with the same `key` (typing, dragging a slider) merge into one step. */
export function useLayoutHistory(initial: Layout) {
  const [state, setState] = useState<HState>({ draft: initial, past: [], future: [] });
  const last = useRef<{ key: string; t: number } | null>(null);

  const commit = useCallback((next: Layout | ((l: Layout) => Layout), key?: string) => {
    setState((s) => {
      const n = typeof next === 'function' ? next(s.draft) : next;
      if (n === s.draft) return s;
      const now = Date.now();
      const merge = !!key && last.current?.key === key && now - last.current.t < 900;
      last.current = key ? { key, t: now } : null;
      return { draft: n, past: merge ? s.past : [...s.past.slice(-LIMIT), s.draft], future: [] };
    });
  }, []);

  /** Change the draft without recording history (live dragging); call `checkpoint` when the gesture ends. */
  const replace = useCallback((next: Layout | ((l: Layout) => Layout)) => {
    setState((s) => ({ ...s, draft: typeof next === 'function' ? next(s.draft) : next }));
  }, []);

  const checkpoint = useCallback((base: Layout) => {
    last.current = null;
    setState((s) => ({ ...s, past: [...s.past.slice(-LIMIT), base], future: [] }));
  }, []);

  const undo = useCallback(() => {
    last.current = null;
    setState((s) => (s.past.length ? { draft: s.past[s.past.length - 1], past: s.past.slice(0, -1), future: [s.draft, ...s.future] } : s));
  }, []);

  const redo = useCallback(() => {
    last.current = null;
    setState((s) => (s.future.length ? { draft: s.future[0], past: [...s.past, s.draft], future: s.future.slice(1) } : s));
  }, []);

  const reset = useCallback((l: Layout) => {
    last.current = null;
    setState({ draft: l, past: [], future: [] });
  }, []);

  return { draft: state.draft, canUndo: state.past.length > 0, canRedo: state.future.length > 0, commit, replace, checkpoint, undo, redo, reset };
}
