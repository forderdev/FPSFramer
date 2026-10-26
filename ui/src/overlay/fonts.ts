// Fonts offered in the editor. The first two are bundled with the app; the rest are standard
// Windows fonts. Anything else typed by the user is used if it is installed.

export const FONT_OPTIONS: { value: string; label: string; note?: string }[] = [
  { value: 'Inter', label: 'Inter (clean, default)' },
  { value: 'JetBrains Mono', label: 'JetBrains Mono (monospace)' },
  { value: 'Segoe UI', label: 'Segoe UI' },
  { value: 'Bahnschrift', label: 'Bahnschrift (sporty)' },
  { value: 'Consolas', label: 'Consolas (monospace)' },
  { value: 'Arial', label: 'Arial' },
  { value: 'Verdana', label: 'Verdana (wide, very readable)' },
  { value: 'Tahoma', label: 'Tahoma' },
  { value: 'Trebuchet MS', label: 'Trebuchet MS' },
  { value: 'Impact', label: 'Impact (bold)' },
  { value: 'Courier New', label: 'Courier New' },
];

export function fontStack(family: string | undefined): string {
  const f = (family ?? 'Inter').replace(/["';{}]/g, '');
  if (f === 'Inter') return `'Inter Variable', Inter, 'Segoe UI', system-ui, sans-serif`;
  if (f === 'JetBrains Mono') return `'JetBrains Mono', Consolas, monospace`;
  return `'${f}', 'Segoe UI', system-ui, sans-serif`;
}

export function textShadowCss(kind: 'none' | 'soft' | 'outline' | undefined): string | undefined {
  if (kind === 'soft') return '0 1px 3px rgb(0 0 0 / 0.7), 0 0 1px rgb(0 0 0 / 0.5)';
  if (kind === 'outline') return '-1px 0 #000, 1px 0 #000, 0 -1px #000, 0 1px #000, 0 0 4px rgb(0 0 0 / 0.85)';
  return undefined;
}

/** `#rrggbb` / `#rgb` / `#rrggbbaa` + opacity → `rgb(r g b / a)` */
export function withAlpha(hex: string | undefined, alpha: number): string {
  if (!hex) return `rgb(0 0 0 / ${alpha})`;
  if (hex === 'transparent') return 'transparent';
  let h = hex.replace('#', '');
  if (h.length === 3) h = h.split('').map((c) => c + c).join('');
  if (h.length === 8) {
    alpha *= parseInt(h.slice(6, 8), 16) / 255;
    h = h.slice(0, 6);
  }
  if (h.length !== 6) return hex;
  const n = parseInt(h, 16);
  return `rgb(${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255} / ${Math.max(0, Math.min(1, alpha))})`;
}
