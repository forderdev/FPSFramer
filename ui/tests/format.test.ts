import { describe, expect, it } from 'vitest';
import { describeCondition, formatBytes, formatClock, formatDuration, formatNumber, formatReading, percentOfRange, relativeTime, statusOf, unavailableText } from '../src/lib/format';
import { acceleratorFromEvent, acceleratorHasModifier } from '../src/lib/router';
import { displayScale, fromDisplayValue, toDisplayValue } from '../src/lib/units';
import type { SensorDesc } from '../src/lib/types';

const metric = { units: { temperature: 'celsius', network: 'megabits' } } as const;
const imperial = { units: { temperature: 'fahrenheit', network: 'megabytes' } } as const;

describe('number formatting', () => {
  it('shows percent, temperature and clock speed with the right unit', () => {
    expect(formatNumber('percent', 42.4, metric)).toEqual({ text: '42', unit: '%' });
    expect(formatNumber('celsius', 70, metric)).toEqual({ text: '70', unit: '°C' });
    expect(formatNumber('celsius', 100, imperial)).toEqual({ text: '212', unit: '°F' });
    expect(formatNumber('mhz', 4650, metric)).toEqual({ text: '4.65', unit: 'GHz' });
    expect(formatNumber('mhz', 900, metric)).toEqual({ text: '900', unit: 'MHz' });
  });
  it('switches memory between MB and GB, and network between Mbps and MB/s', () => {
    expect(formatNumber('megabytes', 512, metric).unit).toBe('MB');
    expect(formatNumber('megabytes', 16384, metric)).toEqual({ text: '16.0', unit: 'GB' });
    expect(formatNumber('bytesPerSec', 12_500_000, metric)).toEqual({ text: '100', unit: 'Mbps' });
    expect(formatNumber('bytesPerSec', 12_500_000, imperial)).toEqual({ text: '12.5', unit: 'MB/s' });
  });
  it('honours an explicit number of decimals', () => {
    expect(formatNumber('ms', 9.87654, metric, 2).text).toBe('9.88');
  });
  it('never invents a value for a missing reading', () => {
    expect(formatReading(undefined, undefined, metric)).toBeNull();
    expect(formatReading({ unit: 'celsius' } as SensorDesc, 'text value', metric)).toEqual({ text: 'text value', unit: '' });
  });
});

describe('time formatting', () => {
  it('formats clocks and durations', () => {
    expect(formatClock(65)).toBe('1:05');
    expect(formatClock(3725)).toBe('1:02:05');
    expect(formatDuration(45)).toBe('45 s');
    expect(formatDuration(720)).toBe('12 min');
    expect(formatDuration(7500)).toBe('2 h 05 min');
  });
  it('describes elapsed time in words', () => {
    const now = 1_000_000_000_000;
    expect(relativeTime(now - 10_000, now)).toBe('just now');
    expect(relativeTime(now - 5 * 60_000, now)).toBe('5 min ago');
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe('3 h ago');
  });
  it('formats sizes', () => {
    expect(formatBytes(900)).toBe('900 B');
    expect(formatBytes(2048)).toBe('2 KB');
    expect(formatBytes(5 * 1048576)).toBe('5.0 MB');
  });
});

describe('status colours', () => {
  const temp = { id: 'cpu.temp', statusRule: { direction: 'higherIsWorse', elevated: 80, critical: 90 } } as SensorDesc;
  const fps = { id: 'fps', statusRule: { direction: 'lowerIsWorse', elevated: 45, critical: 30 } } as SensorDesc;
  it('uses thresholds in the right direction', () => {
    expect(statusOf(temp, 70)).toBe('normal');
    expect(statusOf(temp, 85)).toBe('elevated');
    expect(statusOf(temp, 95)).toBe('critical');
    expect(statusOf(fps, 20)).toBe('critical');
  });
  it('judges FPS against the user’s target when there is one', () => {
    expect(statusOf(fps, 60, 144)).toBe('critical'); // 60 < 0.6 × 144
    expect(statusOf(fps, 140, 144)).toBe('normal');
  });
  it('treats a missing reading as normal rather than alarming', () => {
    expect(statusOf(temp, undefined)).toBe('normal');
  });
});

describe('plain-language helpers', () => {
  it('explains why a sensor is unavailable, or says so generically', () => {
    const d = { availability: { available: false, reason: { code: 'needsCompanion', message: 'Needs HWiNFO.', fix: null } } } as SensorDesc;
    expect(unavailableText(d)).toBe('Needs HWiNFO.');
    expect(unavailableText(undefined)).toMatch(/not available/i);
  });
  it('describes an alert condition', () => {
    expect(describeCondition('GPU temperature', 'above', '85 °C', 8)).toBe('GPU temperature above 85 °C for 8 s');
    expect(describeCondition('FPS', 'below', '30', 0)).toBe('FPS below 30');
  });
  it('maps a value into a bar percentage safely', () => {
    expect(percentOfRange({ unit: 'percent', min: 0, max: 100 } as SensorDesc, 150)).toBe(100);
    expect(percentOfRange({ unit: 'percent', min: 0, max: 100 } as SensorDesc, -5)).toBe(0);
  });
});

describe('display units', () => {
  it('converts to the shown unit and back without drift', () => {
    for (const c of [0, 37, 85, 100]) expect(fromDisplayValue('celsius', toDisplayValue('celsius', c, imperial), imperial)).toBeCloseTo(c, 9);
    expect(toDisplayValue('celsius', 85, imperial)).toBeCloseTo(185, 9);
    expect(fromDisplayValue('bytesPerSec', 100, metric)).toBeCloseTo(12_500_000, 3);
  });
  it('labels chart axes in the user’s units', () => {
    expect(displayScale('celsius', imperial, 0).label).toBe('°F');
    expect(displayScale('mhz', metric, 4500).label).toBe('GHz');
  });
});

describe('keyboard shortcuts', () => {
  const key = (init: Partial<KeyboardEvent> & { key: string }) => init as KeyboardEvent;
  it('turns a key press into an accelerator string', () => {
    expect(acceleratorFromEvent(key({ key: 'f9', ctrlKey: true, shiftKey: true }))).toBe('Ctrl+Shift+f9');
    expect(acceleratorFromEvent(key({ key: 'k', ctrlKey: true, altKey: true }))).toBe('Ctrl+Alt+K');
    expect(acceleratorFromEvent(key({ key: 'ArrowUp', ctrlKey: true }))).toBe('Ctrl+Up');
    expect(acceleratorFromEvent(key({ key: ' ', altKey: true }))).toBe('Alt+Space');
  });
  it('ignores modifier-only presses', () => {
    expect(acceleratorFromEvent(key({ key: 'Control', ctrlKey: true }))).toBeNull();
  });
  it('requires a modifier', () => {
    expect(acceleratorHasModifier('F9')).toBe(false);
    expect(acceleratorHasModifier('Ctrl+Shift+F9')).toBe(true);
    expect(acceleratorHasModifier('Ctrl+')).toBe(false);
    expect(acceleratorHasModifier('Ctrl+Shift')).toBe(false);
  });
});
