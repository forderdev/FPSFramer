import type { Settings, Unit } from './types';

export interface DisplayScale {
  map: (v: number) => number;
  label: string;
  decimals: number;
}

/** How to show a unit on a chart axis: converts raw engine values (°C, MB, bytes/s…) into the user's units. */
export function displayScale(unit: Unit, settings: Pick<Settings, 'units'>, max: number): DisplayScale {
  switch (unit) {
    case 'percent':
      return { map: (v) => v, label: '%', decimals: 0 };
    case 'celsius':
      return settings.units.temperature === 'fahrenheit' ? { map: (v) => (v * 9) / 5 + 32, label: '°F', decimals: 0 } : { map: (v) => v, label: '°C', decimals: 0 };
    case 'mhz':
      return max >= 1000 ? { map: (v) => v / 1000, label: 'GHz', decimals: 2 } : { map: (v) => v, label: 'MHz', decimals: 0 };
    case 'megabytes':
      return max >= 1024 ? { map: (v) => v / 1024, label: 'GB', decimals: 1 } : { map: (v) => v, label: 'MB', decimals: 0 };
    case 'bytesPerSec':
      return settings.units.network === 'megabytes' ? { map: (v) => v / 1e6, label: 'MB/s', decimals: 1 } : { map: (v) => (v * 8) / 1e6, label: 'Mbps', decimals: 1 };
    case 'watt':
      return { map: (v) => v, label: 'W', decimals: 0 };
    case 'volt':
      return { map: (v) => v, label: 'V', decimals: 2 };
    case 'rpm':
      return { map: (v) => v, label: 'RPM', decimals: 0 };
    case 'ms':
      return { map: (v) => v, label: 'ms', decimals: 1 };
    case 'fps':
      return { map: (v) => v, label: 'FPS', decimals: 0 };
    case 'seconds':
      return { map: (v) => v, label: 's', decimals: 0 };
    default:
      return { map: (v) => v, label: '', decimals: 0 };
  }
}

export const UNIT_TITLE: Record<string, string> = {
  percent: 'Usage',
  celsius: 'Temperature',
  mhz: 'Clock speed',
  megabytes: 'Memory',
  bytesPerSec: 'Speed',
  watt: 'Power',
  volt: 'Voltage',
  rpm: 'Fan speed',
  ms: 'Time per frame',
  fps: 'Frames per second',
  seconds: 'Time',
  count: 'Value',
};

/** Engine base value → the number the user types/sees (°F, Mbps…). Other units are unchanged (clocks stay in MHz, memory in MB). */
export function toDisplayValue(unit: Unit, v: number, settings: Pick<Settings, 'units'>): number {
  return displayScale(unit, settings, 0).map(v);
}

/** The inverse of `toDisplayValue`. */
export function fromDisplayValue(unit: Unit, v: number, settings: Pick<Settings, 'units'>): number {
  if (unit === 'celsius' && settings.units.temperature === 'fahrenheit') return ((v - 32) * 5) / 9;
  if (unit === 'bytesPerSec') return settings.units.network === 'megabytes' ? v * 1e6 : (v * 1e6) / 8;
  return v;
}
