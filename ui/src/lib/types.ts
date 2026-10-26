// Shapes exchanged with the engine. They mirror the Rust structs (camelCase JSON).

export type Category = 'fps' | 'cpu' | 'gpu' | 'memory' | 'storage' | 'network' | 'power' | 'thermal' | 'fan' | 'voltage' | 'system' | 'other';
export type Unit = 'percent' | 'celsius' | 'mhz' | 'megabytes' | 'bytesPerSec' | 'watt' | 'volt' | 'rpm' | 'ms' | 'fps' | 'seconds' | 'count' | 'text';
export type Status = 'normal' | 'elevated' | 'critical';
export type UnavailableCode = 'noHardware' | 'needsCompanion' | 'needsPermission' | 'noActiveGame' | 'notSupported' | 'warming' | 'providerError';

export interface StatusRule {
  direction: 'higherIsWorse' | 'lowerIsWorse';
  elevated: number;
  critical: number;
}

export interface Unavailable {
  code: UnavailableCode;
  message: string;
  fix: string | null;
}

export interface SensorDesc {
  id: string;
  name: string;
  short: string;
  description: string;
  category: Category;
  unit: Unit;
  group: string | null;
  source: string;
  statusRule: StatusRule | null;
  availability: { available: boolean; reason: Unavailable | null };
  recordable: boolean;
  min: number | null;
  max: number | null;
  keywords: string[];
}

export type Reading = number | string;
export type Readings = Record<string, Reading>;

export interface ActiveAlert {
  ruleId: string;
  name: string;
  severity: 'info' | 'warning' | 'critical';
  sensorId: string;
  value: number;
  threshold: number;
  message: string;
  sinceMs: number;
  overlay: boolean;
}

export interface CaptureInfo {
  state: 'searching' | 'tracking' | 'stalled' | 'unsupportedApi' | 'needsPermission' | 'unsupported' | 'error' | 'stopped';
  message: string | null;
  pid: number | null;
  exe: string | null;
  name: string | null;
  api: string | null;
}

export interface Snapshot {
  seq: number;
  t: number;
  demo: boolean;
  values: Readings;
  stats: Record<string, [number, number, number]>;
  frames: number[];
  capture: CaptureInfo;
  alerts: ActiveAlert[];
  session: { id: string; elapsedS: number; manual: boolean } | null;
  profile: { id: string; name: string; sceneId: string; sceneName: string; targetFps: number | null };
  overlayEnabled: boolean;
  editMode: boolean;
  gameActive: boolean;
  exclusiveFullscreen: boolean;
}

export type Anchor = 'top-left' | 'top-center' | 'top-right' | 'middle-left' | 'center' | 'middle-right' | 'bottom-left' | 'bottom-center' | 'bottom-right';

export interface Placement {
  id: string;
  layoutId: string;
  kind: 'overlay' | 'widget';
  enabled: boolean;
  monitor: string;
  anchor: Anchor;
  offsetX: number;
  offsetY: number;
  scale: number;
  opacity: number;
  clickThrough: boolean;
  visibility: 'game' | 'always';
  alwaysOnTop: boolean;
}

export interface Scene {
  id: string;
  name: string;
  overlays: Placement[];
}

export interface Profile {
  id: string;
  name: string;
  kind: 'default' | 'game';
  executables: string[];
  notes: string;
  scenes: Scene[];
  activeSceneId: string;
  targetFps: number | null;
  createdAt: number;
  updatedAt: number;
}

export interface Hotkeys {
  toggleOverlay: string | null;
  nextScene: string | null;
  prevScene: string | null;
  toggleEdit: string | null;
  toggleRecording: string | null;
  screenshot: string | null;
  resetStats: string | null;
  openApp: string | null;
}

export interface Settings {
  schemaVersion: number;
  firstRunCompleted: boolean;
  uiMode: 'simple' | 'advanced';
  theme: 'dark' | 'light' | 'system' | 'high-contrast';
  accent: string;
  uiScale: number;
  reduceMotion: boolean;
  units: { temperature: 'celsius' | 'fahrenheit'; network: 'megabits' | 'megabytes' };
  updateIntervalMs: number;
  fpsWindowMs: number;
  statsWindow: 'session' | 'last30s' | 'last60s' | 'last5m';
  lowsMethod: 'average' | 'percentile';
  overlayEnabled: boolean;
  overlay: { reassertTopmost: boolean; excludeFromCapture: boolean };
  widgets: Placement[];
  hotkeys: Hotkeys;
  launchAtStartup: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  recording: { autoRecord: boolean; minDurationS: number; maxSessions: number; recordAllSensors: boolean };
  notifications: { toast: boolean; sound: boolean; sessionSummary: boolean };
  capture: { mode: 'auto' | 'manual'; manualProcess: string | null; ignored: string[] };
  companions: { hwinfo: boolean; libreHardwareMonitor: boolean };
  screenshots: { folder: string | null };
  primaryGpu: number | null;
  sensorOverrides: Record<string, { elevated: number | null; critical: number | null }>;
  pinnedProfile: string | null;
  autoBackup: boolean;
  dismissedTips: string[];
}

export interface AlertRule {
  id: string;
  name: string;
  enabled: boolean;
  sensorId: string;
  comparator: 'above' | 'below';
  threshold: number;
  forSeconds: number;
  cooldownSeconds: number;
  hysteresis: number;
  severity: 'info' | 'warning' | 'critical';
  notify: { overlay: boolean; toast: boolean; sound: boolean };
  profileIds: string[];
  gameOnly: boolean;
  message: string;
}

export interface AlertLogItem {
  t: number;
  ruleId: string;
  name: string;
  severity: 'info' | 'warning' | 'critical';
  message: string;
  kind: 'fired' | 'cleared' | 'suppressed';
}

// ── layouts ──────────────────────────────────────────────────────────────

export interface LayoutCanvas {
  arrangement: 'stack' | 'free';
  direction: 'column' | 'row';
  columns: number;
  gap: number;
  padding: number;
  width?: number;
  height?: number;
  unavailable: 'show' | 'hide';
  background: { color: string; opacity: number; radius: number; borderColor?: string; borderOpacity?: number; borderWidth?: number };
  font: { family: string; size: number; weight: number };
  textColor: string;
  labelColor: string;
  accent: string;
  textShadow: 'none' | 'soft' | 'outline';
  colors: { elevated: string; critical: string; normal?: string };
}

export interface ElStyle {
  fontFamily?: string;
  fontSize?: number;
  fontWeight?: number;
  color?: string;
  labelColor?: string;
  align?: 'left' | 'center' | 'right';
  padding?: number;
  radius?: number;
  textShadow?: 'none' | 'soft' | 'outline';
  background?: { color?: string; opacity?: number };
}

export interface MetricValue {
  sensorId: string;
  stat?: 'live' | 'min' | 'max' | 'avg';
  colorMode?: 'status' | 'fixed' | 'inherit';
  color?: string;
  decimals?: number;
  hideUnit?: boolean;
  prefix?: string;
  suffix?: string;
}

export interface LayoutEl {
  id: string;
  type: 'metric' | 'cores' | 'text' | 'clock' | 'image' | 'shape' | 'graph' | 'alerts' | 'divider' | 'spacer';
  name?: string;
  visible?: boolean;
  locked?: boolean;
  showWhenUnavailable?: boolean;
  x?: number;
  y?: number;
  w?: number;
  h?: number;
  span?: number;
  opacity?: number;
  style?: ElStyle;
  // metric
  label?: { mode: 'auto' | 'text' | 'none'; text?: string };
  values?: MetricValue[];
  display?: 'text' | 'bar' | 'gauge' | 'spark';
  arrange?: 'inline' | 'stacked';
  barWidth?: number;
  // cores
  metric?: 'load' | 'clock' | 'temp';
  columns?: number;
  barHeight?: number;
  showNumbers?: boolean;
  // text
  text?: string;
  // clock
  mode?: 'time' | 'date' | 'datetime' | 'session';
  hour12?: boolean;
  seconds?: boolean | number;
  // image
  src?: string;
  fit?: 'contain' | 'cover' | 'fill';
  // shape
  shape?: 'rect' | 'ellipse' | 'line';
  /** Shapes: fill colour. Graphs: whether to fill under the line. */
  fill?: string | boolean;
  fillOpacity?: number;
  stroke?: string;
  strokeWidth?: number;
  radius?: number;
  // graph
  series?: { sensorId: string; color?: string; label?: string }[];
  source?: 'history' | 'frames';
  frames?: number;
  min?: number;
  max?: number;
  thickness?: number;
  grid?: boolean;
  showLabel?: boolean;
  showValue?: boolean;
  showTarget?: boolean;
  title?: string;
  // alerts
  maxVisible?: number;
  // divider / spacer
  size?: number;
  [k: string]: unknown;
}

export interface Layout {
  schemaVersion: number;
  id: string;
  name: string;
  description: string;
  builtin: boolean;
  tags: string[];
  canvas: LayoutCanvas;
  elements: LayoutEl[];
  createdAt: number;
  updatedAt: number;
}

export interface OverlaySpec {
  id: string;
  layoutId: string;
  kind: 'overlay' | 'widget';
  monitor: string;
  anchor: Anchor;
  offsetX: number;
  offsetY: number;
  scale: number;
  opacity: number;
  clickThrough: boolean;
  alwaysOnTop: boolean;
  editMode: boolean;
  excludeFromCapture: boolean;
  reassertTopmost: boolean;
}

// ── sessions ─────────────────────────────────────────────────────────────

export interface HardwareInfo {
  cpu: string;
  gpus: string[];
  ramGb: number;
  os: string;
  displays: string[];
  appVersion: string;
}

export interface SessionMeta {
  id: string;
  name: string;
  kind: 'game' | 'manual';
  startedAt: number;
  endedAt: number;
  durationS: number;
  gameName: string | null;
  exe: string | null;
  api: string | null;
  profileId: string | null;
  profileName: string | null;
  demo: boolean;
  hardware: HardwareInfo;
  notes: string;
  starred: boolean;
}

export interface BottleneckReport {
  analyzedSeconds: number;
  verdict: 'gpuLimited' | 'cpuLimited' | 'frameCapped' | 'balanced' | 'unknown';
  confidence: 'low' | 'moderate' | 'high';
  gpuBoundPct: number | null;
  cpuBoundPct: number | null;
  vramPressurePct: number | null;
  ramPressurePct: number | null;
  hotPct: number | null;
  throttlePct: number | null;
  onBatteryPct: number | null;
  capFps: number | null;
}

export interface Finding {
  id: string;
  level: 'good' | 'info' | 'notice' | 'warning';
  certainty: 'measured' | 'likely' | 'possible';
  title: string;
  detail: string;
  evidence: string[];
}

export interface ColumnStat {
  min: number;
  avg: number;
  max: number;
  samples: number;
}

export interface SessionSummary {
  durationS: number;
  frames: number;
  fpsAvg: number | null;
  fpsMin: number | null;
  fpsMax: number | null;
  low1: number | null;
  low01: number | null;
  medianMs: number | null;
  p99Ms: number | null;
  worstMs: number | null;
  consistency: number | null;
  spikeCount: number;
  dropCount: number;
  targetFps: number | null;
  belowTargetPct: number | null;
  sensorStats: Record<string, ColumnStat>;
  bottleneck: BottleneckReport | null;
  findings: Finding[];
}

export interface Spike {
  tS: number;
  frameMs: number;
  baselineMs: number;
  frames: number;
}

export interface FpsDrop {
  startS: number;
  durationS: number;
  minFps: number;
  baselineFps: number;
  context: Record<string, number | null>;
  hint: string | null;
}

export interface SessionData {
  schemaVersion: number;
  meta: SessionMeta;
  summary: SessionSummary;
  intervalMs: number;
  columns: Record<string, (number | null)[]>;
  hist: { bins: [number, number][]; sumMs: number };
  spikes: Spike[];
  drops: FpsDrop[];
  alerts: { tS: number; ruleId: string; name: string; severity: string; message: string; kind: string }[];
}

export interface SessionListItem {
  meta: SessionMeta;
  frames: number;
  fpsAvg: number | null;
  low1: number | null;
  low01: number | null;
  spikeCount: number;
  dropCount: number;
  verdict: BottleneckReport['verdict'] | null;
  warnings: number;
}

// ── misc ─────────────────────────────────────────────────────────────────

export interface ProviderStatus {
  id: string;
  label: string;
  health: 'ok' | 'degraded' | 'failed';
  message: string | null;
  sensors: number;
}

export interface StatusInfo {
  capture: CaptureInfo;
  providers: ProviderStatus[];
  elevated: boolean;
  captureEvents: number;
}

export interface DiagCheck {
  id: string;
  title: string;
  status: 'ok' | 'info' | 'warn' | 'fail';
  summary: string;
  details: string[];
  fix: { label: string; action: string; arg: string | null } | null;
}

export interface TrayState {
  overlayEnabled: boolean;
  recording: boolean;
  profileId: string;
  profileName: string;
  scenes: { id: string; name: string; active: boolean }[];
  game: string | null;
  alertCount: number;
}

export interface UiToast {
  level: 'info' | 'success' | 'warning' | 'error';
  message: string;
  action: { label: string; route: string } | null;
}

export interface HistoryData {
  endMs: number;
  intervalMs: number;
  series: Record<string, (number | null)[]>;
}

export interface AppStateDto {
  version: string;
  platform: string;
  demo: boolean;
  demoGameRunning: boolean | null;
  firstRun: boolean;
  settings: Settings;
  profiles: Profile[];
  layouts: Layout[];
  rules: AlertRule[];
  catalog: SensorDesc[];
  status: StatusInfo;
  notices: string[];
  hotkeys: { action: string; accelerator: string }[];
  snapshot: Snapshot | null;
  hardware: HardwareInfo | null;
  paths: { config: string; data: string };
  presetIds: string[];
  tray: TrayState | null;
  overlays: OverlaySpec[];
}

export type EngineEvent =
  | { type: 'snapshot'; data: Snapshot }
  | { type: 'catalog'; data: SensorDesc[] }
  | { type: 'settings'; data: Settings }
  | { type: 'layouts'; data: Layout[] }
  | { type: 'profiles'; data: Profile[] }
  | { type: 'rules'; data: AlertRule[] }
  | { type: 'sessions' }
  | { type: 'screenshots' }
  | { type: 'overlays'; data: OverlaySpec[] }
  | { type: 'notify'; data: { title: string; body: string; level: string; sound: boolean } }
  | { type: 'toast'; data: UiToast }
  | { type: 'alertLog'; data: AlertLogItem }
  | { type: 'sessionFinished'; data: { id: string; name: string } }
  | { type: 'hotkeys'; data: { action: string; accelerator: string }[] }
  | { type: 'tray'; data: TrayState }
  | { type: 'status'; data: StatusInfo }
  | { type: 'quit' };
