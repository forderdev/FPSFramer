import type { BottleneckReport } from './types';

export const VERDICT_LABEL: Record<BottleneckReport['verdict'], string> = {
  gpuLimited: 'Graphics-card limited',
  cpuLimited: 'Processor limited',
  frameCapped: 'Frame-rate capped',
  balanced: 'Balanced',
  unknown: 'Not enough data',
};

export const VERDICT_HELP: Record<BottleneckReport['verdict'], string> = {
  gpuLimited: 'The graphics card was working at its limit most of the time, so it is what limits your FPS. This is an estimate from usage readings.',
  cpuLimited: 'The graphics card had spare capacity while a processor core was very busy, so the CPU may be what limits your FPS. This is an estimate from usage readings.',
  frameCapped: 'FPS stayed pinned at one value while the hardware was not fully used. V-Sync or a frame limiter is probably on.',
  balanced: 'Neither the processor nor the graphics card was clearly the limit.',
  unknown: 'CPU or GPU usage readings were missing, so no estimate can be made.',
};

let n = 0;
/** Client-side id for new items (the engine re-validates). */
export function newId(prefix: string): string {
  n++;
  return `${prefix}_${Date.now().toString(36)}${n.toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

export function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}
