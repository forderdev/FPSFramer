import { useSyncExternalStore } from 'react';

function subscribe(cb: () => void): () => void {
  window.addEventListener('hashchange', cb);
  return () => window.removeEventListener('hashchange', cb);
}

function current(): string {
  const h = window.location.hash.replace(/^#/, '');
  return h === '' ? '/' : h;
}

/** Current hash route, e.g. `/sessions/ses_ab12?x=1`. */
export function useRoute(): { path: string; segments: string[]; query: URLSearchParams } {
  const raw = useSyncExternalStore(subscribe, current);
  const [path, qs = ''] = raw.split('?');
  return { path, segments: path.split('/').filter(Boolean), query: new URLSearchParams(qs) };
}

let guard: (() => Promise<boolean>) | null = null;

/** Ask before leaving the current screen (used by the editor when there are unsaved changes). */
export function setNavigationGuard(g: (() => Promise<boolean>) | null): void {
  guard = g;
}

export function navigate(to: string): void {
  const target = to.startsWith('/') ? to : `/${to}`;
  if (current() === target) return;
  if (guard) {
    void guard().then((ok) => {
      if (ok) window.location.hash = target;
    });
  } else {
    window.location.hash = target;
  }
}

/** Navigate without asking (after the guard has been satisfied, e.g. after saving). */
export function navigateNow(to: string): void {
  window.location.hash = to.startsWith('/') ? to : `/${to}`;
}

/** Turn "Ctrl+Shift+F9" into a key-combination from a keyboard event, or null for modifier-only presses. */
export function acceleratorFromEvent(e: KeyboardEvent): string | null {
  const key = e.key;
  if (['Control', 'Shift', 'Alt', 'Meta', 'AltGraph'].includes(key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Super');
  let k = key;
  if (k === ' ') k = 'Space';
  else if (k.length === 1) k = k.toUpperCase();
  else if (k.startsWith('Arrow')) k = k.slice(5);
  else if (k === 'Escape') k = 'Esc';
  parts.push(k);
  return parts.join('+');
}

const MODIFIERS = ['Ctrl', 'Alt', 'Shift', 'Super'];

/** A usable shortcut is one or more modifiers followed by exactly one real key. */
export function acceleratorHasModifier(a: string): boolean {
  const parts = a.split('+');
  const key = parts[parts.length - 1];
  return parts.length > 1 && key.length > 0 && !MODIFIERS.includes(key) && parts.slice(0, -1).every((p) => MODIFIERS.includes(p));
}
