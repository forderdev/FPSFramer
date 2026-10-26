# Layout file format

Layouts are plain JSON (`*.fpsframer-layout.json` when exported). They are validated by
`crates/core/src/layouts.rs` (Rust) and drawn by `ui/src/overlay/LayoutRenderer.tsx` (TypeScript). The
editor, the overlay window, the Layouts page and the wizard all use that one renderer, so what you see in the
preview is what the overlay shows.

## Export wrapper

```json
{ "format": "fpsframer-layout", "version": 1, "layout": { … } }
```

Profiles export as `{ "format": "fpsframer-profile", "version": 1, "profile": {…}, "layouts": [{…}] }` and
include the layouts they use. Imported items always receive fresh ids and are never `builtin`.

## Layout

```jsonc
{
  "schemaVersion": 1,
  "id": "lay_x7k2…",          // generated; built-ins are "preset.*" and read-only
  "name": "My layout",
  "description": "",
  "tags": [],
  "canvas": {
    "arrangement": "stack",    // "stack": items flow in a list; "free": items have x/y/w/h
    "direction": "column", "columns": 1, "gap": 4, "padding": 10,
    "unavailable": "hide",     // "hide" or "show" N/A rows for sensors this PC can't measure
    "background": { "color": "#0b0e14", "opacity": 0.72, "radius": 10, "borderColor": "#ffffff", "borderOpacity": 0.1, "borderWidth": 1 },
    "font": { "family": "Inter", "size": 13, "weight": 500 },
    "textColor": "#ffffff", "labelColor": "#b8c2d6", "accent": "#3ddcff",
    "textShadow": "soft",      // none | soft | outline
    "colors": { "elevated": "#ffb454", "critical": "#ff5d5d" }
  },
  "elements": [ /* see below */ ]
}
```

Limits (enforced on every save and import): at most 150 elements; images must be `data:image/(png|jpeg|gif|webp|svg+xml)`
URLs of at most 1.5 MB each and 5 MB in total; colours must be hex; numbers are clamped to sane ranges.
Unknown element types are rejected.

## Elements

Every element can have `id`, `name`, `visible`, `locked`, `opacity`, `style` (font, colour, alignment, padding,
radius, background) and, in free layouts, `x`, `y`, `w`, `h`. In stack layouts `span` sets how many columns it covers.

| `type` | Purpose | Main fields |
| --- | --- | --- |
| `metric` | one or more sensor values | `values: [{ sensorId, stat: live|min|max|avg, colorMode: status|fixed|inherit, decimals, prefix, suffix, hideUnit }]`, `label: {mode: auto|text|none, text}`, `display: text|bar|gauge|spark`, `arrange: inline|stacked`, `showWhenUnavailable` |
| `cores` | per-core CPU bars | `metric: load|clock|temp`, `columns`, `barHeight`, `showNumbers` |
| `graph` | line graph | `source: history|frames`, `series: [{ sensorId, color, label }]`, `min`, `max`, `frames`, `thickness`, `grid`, `fill`, `showTarget`, `showLabel`, `showValue`, `title` |
| `text` | free text | `text` |
| `clock` | time | `mode: time|date|datetime|session`, `hour12`, `seconds` |
| `image` | picture | `src` (data URL), `fit: contain|cover|fill` |
| `shape` | rectangle / ellipse / line | `shape`, `fill`, `fillOpacity`, `stroke`, `strokeWidth`, `radius` |
| `alerts` | area where active alerts appear | `maxVisible` |
| `divider`, `spacer` | spacing | `size`, `thickness` |

### Sensor ids

`fps`, `fps.avg`, `fps.min`, `fps.max`, `fps.low1`, `fps.low01`, `frametime`, `frametime.max`,
`cpu.load`, `cpu.core.max`, `cpu.core.<n>.load`, `cpu.core.<n>.clock`, `cpu.clock`, `cpu.temp`, `cpu.power`, `gpu.load`, `gpu.temp`,
`gpu.clock`, `gpu.power`, `gpu.fan`, `gpu.vram.used|total|percent`, `ram.used|percent`, `disk.read|write|active|latency`,
`net.down|up`, `battery.percent`, … The full, current list (with units, descriptions and availability on this PC)
is what the app's "Choose what to measure" picker shows. A layout may reference a sensor that doesn't exist on
another PC; it is then hidden (or shown as N/A, per `canvas.unavailable`), never drawn as zero.

## Placement (which layout is shown where)

A *profile* has *scenes*; a scene holds *placements*: `{ layoutId, monitor, anchor, offsetX, offsetY, scale,
opacity, clickThrough, visibility: game|always, alwaysOnTop, enabled }`. `monitor` is `"game"` (the monitor the
game is on), `"primary"` or a monitor name. `anchor` is one of nine positions (`top-left` … `bottom-right`);
offsets are logical pixels from that anchor. The desktop widget is a placement kept in settings.
