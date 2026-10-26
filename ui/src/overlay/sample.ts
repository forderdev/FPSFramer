// Plausible example values used ONLY for previews (editor, preset thumbnails) when no real
// reading exists. Anything drawn from this is labelled "Sample values" in the UI and is
// never used by the live overlay.

import type { ActiveAlert, Readings, SensorDesc } from '../lib/types';

const FIXED: Record<string, number | string> = {
  fps: 144,
  frametime: 6.9,
  'frametime.max': 9.4,
  'fps.avg': 139,
  'fps.min': 98,
  'fps.max': 171,
  'fps.low1': 112,
  'fps.low01': 94,
  'cpu.load': 34,
  'cpu.core.max': 71,
  'cpu.clock': 4650,
  'cpu.temp': 62,
  'cpu.power': 68,
  'gpu.load': 97,
  'gpu.temp': 71,
  'gpu.hotspot': 82,
  'gpu.clock': 2610,
  'gpu.memclock': 10500,
  'gpu.power': 212,
  'gpu.fan': 48,
  'gpu.vram.used': 6840,
  'gpu.vram.total': 12288,
  'gpu.vram.percent': 56,
  'ram.used': 14500,
  'ram.total': 32768,
  'ram.available': 18200,
  'ram.percent': 44,
  'disk.read': 18_000_000,
  'disk.write': 4_000_000,
  'disk.active': 12,
  'disk.latency': 0.6,
  'net.down': 2_400_000,
  'net.up': 380_000,
  'battery.percent': 82,
  'session.time': 1425,
  'game.name': 'Sample Game',
  'game.api': 'DirectX 12',
  'gpu.name': 'Sample GPU',
  'cpu.name': 'Sample CPU',
  'gpu.throttle': 'None',
};

function fallback(d: SensorDesc): number | string {
  switch (d.unit) {
    case 'percent':
      return 40;
    case 'celsius':
      return 55;
    case 'mhz':
      return 3200;
    case 'megabytes':
      return 4096;
    case 'bytesPerSec':
      return 1_000_000;
    case 'watt':
      return 45;
    case 'volt':
      return 1.12;
    case 'rpm':
      return 1100;
    case 'ms':
      return 5;
    case 'fps':
      return 120;
    case 'seconds':
      return 600;
    case 'text':
      return 'Sample';
    default:
      return 0;
  }
}

export function sampleValue(id: string, d?: SensorDesc): number | string {
  if (id in FIXED) return FIXED[id];
  const core = /^cpu\.core\.(\d+)\.load$/.exec(id);
  if (core) return 20 + ((Number(core[1]) * 37) % 60);
  return d ? fallback(d) : 0;
}

export function sampleReadings(catalog: Record<string, SensorDesc>): Readings {
  const out: Readings = {};
  for (const id of Object.keys(FIXED)) out[id] = FIXED[id];
  for (const d of Object.values(catalog)) {
    if (!(d.id in out)) out[d.id] = sampleValue(d.id, d);
  }
  return out;
}

export const SAMPLE_ALERT: ActiveAlert = {
  ruleId: 'sample',
  name: 'Sample alert',
  severity: 'warning',
  sensorId: 'gpu.temp',
  value: 86,
  threshold: 85,
  message: 'GPU temperature is high: 86°C',
  sinceMs: 0,
  overlay: true,
};

/** Deterministic wobble for sample graphs. */
export function sampleWave(n: number, base: number, amp: number, seed = 1): number[] {
  return Array.from({ length: n }, (_, i) => base + amp * Math.sin(i / 7 + seed) * 0.6 + amp * Math.sin(i / 2.3 + seed * 2) * 0.4);
}
