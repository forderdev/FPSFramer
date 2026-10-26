import type { SensorDesc, Settings, Status } from './types';

export interface Formatted {
  /** The number (or text), ready to show. */
  text: string;
  /** Unit suffix such as `%`, `°C`, `GHz`; may be empty. */
  unit: string;
}

export interface FormatOpts {
  decimals?: number;
  hideUnit?: boolean;
}

function fixed(v: number, d: number): string {
  return v.toFixed(Math.max(0, d));
}

/** Format a numeric reading using the sensor's unit and the user's unit preferences. */
export function formatNumber(unit: SensorDesc['unit'], v: number, s: Pick<Settings, 'units'>, decimals?: number): Formatted {
  switch (unit) {
    case 'percent':
      return { text: fixed(v, decimals ?? 0), unit: '%' };
    case 'celsius': {
      const f = s.units.temperature === 'fahrenheit';
      return { text: fixed(f ? (v * 9) / 5 + 32 : v, decimals ?? 0), unit: f ? '°F' : '°C' };
    }
    case 'mhz':
      return v >= 1000 ? { text: fixed(v / 1000, decimals ?? 2), unit: 'GHz' } : { text: fixed(v, decimals ?? 0), unit: 'MHz' };
    case 'megabytes':
      return v >= 1024 ? { text: fixed(v / 1024, decimals ?? 1), unit: 'GB' } : { text: fixed(v, decimals ?? 0), unit: 'MB' };
    case 'bytesPerSec':
      if (s.units.network === 'megabytes') return { text: fixed(v / 1e6, decimals ?? 1), unit: 'MB/s' };
      {
        const mbps = (v * 8) / 1e6;
        return { text: fixed(mbps, decimals ?? (mbps >= 100 ? 0 : 1)), unit: 'Mbps' };
      }
    case 'watt':
      return { text: fixed(v, decimals ?? 0), unit: 'W' };
    case 'volt':
      return { text: fixed(v, decimals ?? 2), unit: 'V' };
    case 'rpm':
      return { text: fixed(v, decimals ?? 0), unit: 'RPM' };
    case 'ms':
      return { text: fixed(v, decimals ?? 1), unit: 'ms' };
    case 'fps':
      return { text: fixed(v, decimals ?? 0), unit: 'FPS' };
    case 'seconds':
      return { text: formatClock(v), unit: '' };
    case 'count':
      return { text: fixed(v, decimals ?? 0), unit: '' };
    default:
      return { text: String(v), unit: '' };
  }
}

export function formatReading(d: SensorDesc | undefined, v: number | string | undefined, s: Pick<Settings, 'units'>, o: FormatOpts = {}): Formatted | null {
  if (v === undefined || v === null) return null;
  if (typeof v === 'string') return { text: v, unit: '' };
  const f = formatNumber(d?.unit ?? 'count', v, s, o.decimals);
  if (o.hideUnit || d?.unit === 'fps') return { text: f.text, unit: o.hideUnit ? '' : f.unit };
  return f;
}

/** `1:02:05` or `2:05` */
export function formatClock(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}` : `${m}:${String(r).padStart(2, '0')}`;
}

/** "2 h 05 min", "12 min", "45 s" */
export function formatDuration(secs: number): string {
  const s = Math.round(secs);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} min`;
}

export function formatDateTime(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { dateStyle: 'medium' });
}

export function relativeTime(ms: number, now = Date.now()): string {
  const d = Math.round((now - ms) / 1000);
  if (d < 45) return 'just now';
  if (d < 3600) return `${Math.round(d / 60)} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  if (d < 7 * 86400) return `${Math.round(d / 86400)} d ago`;
  return formatDate(ms);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1048576) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1048576).toFixed(1)} MB`;
}

/** Colour-independent health of one reading. */
export function statusOf(d: SensorDesc | undefined, v: number | undefined, targetFps?: number | null): Status {
  if (v === undefined || !d) return 'normal';
  if (d.id === 'fps' && targetFps) {
    if (v < targetFps * 0.6) return 'critical';
    if (v < targetFps * 0.9) return 'elevated';
    return 'normal';
  }
  const r = d.statusRule;
  if (!r) return 'normal';
  if (r.direction === 'higherIsWorse') return v >= r.critical ? 'critical' : v >= r.elevated ? 'elevated' : 'normal';
  return v <= r.critical ? 'critical' : v <= r.elevated ? 'elevated' : 'normal';
}

export const STATUS_LABEL: Record<Status, string> = { normal: 'Normal', elevated: 'Elevated', critical: 'Critical' };

export function unavailableText(d: SensorDesc | undefined): string {
  if (!d) return 'This measurement is not available on this PC.';
  return d.availability.reason?.message ?? 'Not available on this PC.';
}

/** Codes for which a permanently missing sensor should simply not be drawn in the overlay. */
export const HIDE_IN_OVERLAY: ReadonlySet<string> = new Set(['noHardware', 'needsCompanion', 'notSupported']);

export function percentOfRange(d: SensorDesc | undefined, v: number): number {
  const min = d?.min ?? 0;
  const max = d?.max ?? (d?.unit === 'percent' ? 100 : Math.max(v, 1));
  return Math.max(0, Math.min(100, ((v - min) / (max - min || 1)) * 100));
}

/** Plain-language sentence for an alert rule: "GPU temperature above 85 °C for 8 s". */
export function describeCondition(name: string, comparator: 'above' | 'below', threshold: string, forSeconds: number): string {
  const base = `${name} ${comparator} ${threshold}`;
  return forSeconds > 0 ? `${base} for ${forSeconds} s` : base;
}
