//! Overlay layouts: the data format, validation of imported files, and the
//! built-in presets (Minimal, Gaming, Detailed, Advanced, …).
//!
//! A layout is JSON that the UI's single `LayoutRenderer` draws – the same code
//! renders the real overlay window, the editor preview and the preset thumbnails,
//! which is what makes the preview exact. See `docs/LAYOUT_FORMAT.md`.
//!
//! The element list is kept as `serde_json::Value` here: Rust only needs to
//! *validate* it (types, ranges, sizes), while the UI owns the typed model.

use crate::model::is_hex_color;
use crate::util::{gen_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub const LAYOUT_SCHEMA: u32 = 1;
pub const MAX_ELEMENTS: usize = 150;
pub const MAX_IMAGE_BYTES: usize = 1_500_000;
pub const MAX_TOTAL_IMAGE_BYTES: usize = 5_000_000;

const ELEMENT_TYPES: [&str; 10] =
    ["metric", "cores", "text", "clock", "image", "shape", "graph", "alerts", "divider", "spacer"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub tags: Vec<String>,
    pub canvas: Value,
    pub elements: Vec<Value>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            schema_version: LAYOUT_SCHEMA,
            id: String::new(),
            name: String::new(),
            description: String::new(),
            builtin: false,
            tags: Vec::new(),
            canvas: default_canvas(),
            elements: Vec::new(),
            created_at: 0,
            updated_at: 0,
        }
    }
}

pub fn default_canvas() -> Value {
    json!({
        "arrangement": "stack",
        "direction": "column",
        "columns": 1,
        "gap": 5,
        "padding": 10,
        "unavailable": "hide",
        "background": { "color": "#0a0e14", "opacity": 0.66, "radius": 12, "borderColor": "#ffffff", "borderOpacity": 0.08, "borderWidth": 1 },
        "font": { "family": "Inter", "size": 14, "weight": 600 },
        "textColor": "#f1f5f9",
        "labelColor": "#9fb3c8",
        "accent": "#3ddcff",
        "textShadow": "soft",
        "colors": { "elevated": "#fbbf24", "critical": "#ff6b6b" }
    })
}

// ───────────────────────────── validation ─────────────────────────────

fn num_in(o: &mut Map<String, Value>, key: &str, lo: f64, hi: f64) {
    if let Some(v) = o.get(key) {
        match v.as_f64() {
            Some(n) if n.is_finite() => {
                let c = n.clamp(lo, hi);
                if c != n {
                    o.insert(key.into(), json!(c));
                }
            }
            _ => {
                o.remove(key);
            }
        }
    }
}

fn str_max(o: &mut Map<String, Value>, key: &str, max: usize) {
    if let Some(v) = o.get(key) {
        match v.as_str() {
            Some(s) if s.chars().count() > max => {
                o.insert(key.into(), json!(s.chars().take(max).collect::<String>()));
            }
            Some(_) => {}
            None => {
                o.remove(key);
            }
        }
    }
}

fn color(o: &mut Map<String, Value>, key: &str) {
    if let Some(v) = o.get(key) {
        let ok = v.as_str().is_some_and(|s| is_hex_color(s) || s == "transparent");
        if !ok {
            o.remove(key);
        }
    }
}

fn one_of(o: &mut Map<String, Value>, key: &str, allowed: &[&str]) {
    if let Some(v) = o.get(key) {
        if !v.as_str().is_some_and(|s| allowed.contains(&s)) {
            o.remove(key);
        }
    }
}

fn boolean(o: &mut Map<String, Value>, key: &str) {
    if o.get(key).is_some_and(|v| !v.is_boolean()) {
        o.remove(key);
    }
}

fn font_family(o: &mut Map<String, Value>) {
    if let Some(v) = o.get("family") {
        let ok = v.as_str().is_some_and(|s| {
            !s.is_empty()
                && s.chars().count() <= 60
                && s.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
        });
        if !ok {
            o.remove("family");
        }
    }
}

fn sanitize_style(style: &mut Value) {
    let Some(o) = style.as_object_mut() else {
        *style = json!({});
        return;
    };
    font_family(o);
    num_in(o, "fontSize", 6.0, 200.0);
    num_in(o, "fontWeight", 100.0, 900.0);
    color(o, "color");
    color(o, "labelColor");
    one_of(o, "align", &["left", "center", "right"]);
    num_in(o, "padding", 0.0, 80.0);
    num_in(o, "radius", 0.0, 200.0);
    one_of(o, "textShadow", &["none", "soft", "outline"]);
    if let Some(bg) = o.get_mut("background") {
        match bg.as_object_mut() {
            Some(b) => {
                color(b, "color");
                num_in(b, "opacity", 0.0, 1.0);
            }
            None => *bg = json!({}),
        }
    }
}

fn sanitize_element(e: &mut Value, image_bytes: &mut usize) -> bool {
    let Some(o) = e.as_object_mut() else { return false };
    let ty = o.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    if !ELEMENT_TYPES.contains(&ty.as_str()) {
        return false;
    }
    str_max(o, "id", 40);
    str_max(o, "name", 80);
    boolean(o, "visible");
    boolean(o, "locked");
    boolean(o, "showWhenUnavailable");
    for k in ["x", "y"] {
        num_in(o, k, -20_000.0, 20_000.0);
    }
    for k in ["w", "h"] {
        num_in(o, k, 1.0, 8_000.0);
    }
    num_in(o, "span", 1.0, 12.0);
    num_in(o, "opacity", 0.0, 1.0);
    if let Some(s) = o.get_mut("style") {
        sanitize_style(s);
    }

    match ty.as_str() {
        "metric" => {
            one_of(o, "display", &["text", "bar", "gauge", "spark"]);
            one_of(o, "arrange", &["inline", "stacked"]);
            num_in(o, "barWidth", 20.0, 1000.0);
            if let Some(l) = o.get_mut("label") {
                match l.as_object_mut() {
                    Some(lo) => {
                        one_of(lo, "mode", &["auto", "text", "none"]);
                        str_max(lo, "text", 60);
                    }
                    None => *l = json!({ "mode": "auto" }),
                }
            }
            match o.get_mut("values").and_then(Value::as_array_mut) {
                Some(vals) => {
                    vals.truncate(8);
                    vals.retain_mut(|v| {
                        let Some(vo) = v.as_object_mut() else { return false };
                        str_max(vo, "sensorId", 120);
                        if vo.get("sensorId").and_then(Value::as_str).is_none_or(str::is_empty) {
                            return false;
                        }
                        one_of(vo, "stat", &["live", "min", "max", "avg"]);
                        one_of(vo, "colorMode", &["status", "fixed", "inherit"]);
                        num_in(vo, "decimals", 0.0, 3.0);
                        boolean(vo, "hideUnit");
                        color(vo, "color");
                        str_max(vo, "prefix", 12);
                        str_max(vo, "suffix", 12);
                        true
                    });
                }
                None => {
                    o.insert("values".into(), json!([]));
                }
            }
        }
        "cores" => {
            one_of(o, "metric", &["load", "clock", "temp"]);
            num_in(o, "columns", 1.0, 16.0);
            num_in(o, "barHeight", 2.0, 60.0);
            boolean(o, "showNumbers");
        }
        "text" => str_max(o, "text", 400),
        "clock" => {
            one_of(o, "mode", &["time", "date", "datetime", "session"]);
            boolean(o, "hour12");
            boolean(o, "seconds");
        }
        "image" => {
            one_of(o, "fit", &["contain", "cover", "fill"]);
            let ok = o.get("src").and_then(Value::as_str).is_some_and(|s| {
                let prefixes = [
                    "data:image/png;base64,",
                    "data:image/jpeg;base64,",
                    "data:image/gif;base64,",
                    "data:image/webp;base64,",
                    "data:image/svg+xml;base64,",
                ];
                prefixes.iter().any(|p| s.starts_with(p)) && s.len() <= MAX_IMAGE_BYTES
            });
            if !ok {
                o.remove("src");
            } else {
                *image_bytes += o.get("src").and_then(Value::as_str).map_or(0, str::len);
            }
        }
        "shape" => {
            one_of(o, "shape", &["rect", "ellipse", "line"]);
            color(o, "fill");
            color(o, "stroke");
            num_in(o, "fillOpacity", 0.0, 1.0);
            num_in(o, "strokeWidth", 0.0, 50.0);
            num_in(o, "radius", 0.0, 500.0);
        }
        "graph" => {
            num_in(o, "seconds", 5.0, 300.0);
            num_in(o, "frames", 30.0, 1200.0);
            one_of(o, "source", &["history", "frames"]);
            num_in(o, "min", -10_000.0, 100_000.0);
            num_in(o, "max", -10_000.0, 100_000.0);
            num_in(o, "thickness", 0.5, 8.0);
            for k in ["fill", "grid", "showLabel", "showValue", "showTarget"] {
                boolean(o, k);
            }
            str_max(o, "title", 60);
            match o.get_mut("series").and_then(Value::as_array_mut) {
                Some(series) => {
                    series.truncate(4);
                    series.retain_mut(|s| {
                        let Some(so) = s.as_object_mut() else { return false };
                        str_max(so, "sensorId", 120);
                        color(so, "color");
                        str_max(so, "label", 40);
                        so.get("sensorId").and_then(Value::as_str).is_some_and(|x| !x.is_empty())
                    });
                }
                None => {
                    o.insert("series".into(), json!([]));
                }
            }
        }
        "alerts" => num_in(o, "maxVisible", 1.0, 8.0),
        "divider" | "spacer" => {
            num_in(o, "size", 1.0, 400.0);
        }
        _ => {}
    }
    true
}

impl Layout {
    /// Validate and repair in place. Returns an error only when the layout is
    /// unusable (e.g. too large); individual bad elements are dropped.
    pub fn sanitize(&mut self) -> Result<(), String> {
        self.schema_version = LAYOUT_SCHEMA;
        self.name = self.name.trim().chars().take(80).collect();
        if self.name.is_empty() {
            self.name = "Untitled layout".into();
        }
        self.description = self.description.chars().take(400).collect();
        self.tags.truncate(8);
        self.tags.iter_mut().for_each(|t| *t = t.chars().take(24).collect());
        if self.elements.len() > MAX_ELEMENTS {
            return Err(format!("A layout can have at most {MAX_ELEMENTS} elements."));
        }
        // Canvas.
        if !self.canvas.is_object() {
            self.canvas = default_canvas();
        }
        let c = self.canvas.as_object_mut().unwrap();
        one_of(c, "arrangement", &["stack", "free"]);
        one_of(c, "direction", &["column", "row"]);
        one_of(c, "unavailable", &["show", "hide"]);
        one_of(c, "textShadow", &["none", "soft", "outline"]);
        num_in(c, "columns", 1.0, 8.0);
        num_in(c, "gap", 0.0, 100.0);
        num_in(c, "padding", 0.0, 100.0);
        num_in(c, "width", 40.0, 8000.0);
        num_in(c, "height", 20.0, 8000.0);
        for k in ["textColor", "labelColor", "accent"] {
            color(c, k);
        }
        if let Some(f) = c.get_mut("font").and_then(Value::as_object_mut) {
            font_family(f);
            num_in(f, "size", 6.0, 200.0);
            num_in(f, "weight", 100.0, 900.0);
        }
        if let Some(b) = c.get_mut("background").and_then(Value::as_object_mut) {
            color(b, "color");
            color(b, "borderColor");
            num_in(b, "opacity", 0.0, 1.0);
            num_in(b, "borderOpacity", 0.0, 1.0);
            num_in(b, "radius", 0.0, 200.0);
            num_in(b, "borderWidth", 0.0, 20.0);
        }
        if let Some(cs) = c.get_mut("colors").and_then(Value::as_object_mut) {
            color(cs, "elevated");
            color(cs, "critical");
            color(cs, "normal");
        }
        // Elements.
        let mut image_bytes = 0usize;
        self.elements.retain_mut(|e| sanitize_element(e, &mut image_bytes));
        if image_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err("The images in this layout are too large (5 MB total limit).".into());
        }
        // Unique, non-empty element ids.
        let mut seen = HashSet::new();
        for e in &mut self.elements {
            let o = e.as_object_mut().unwrap();
            let id = o.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            if id.is_empty() || !seen.insert(id.clone()) {
                let fresh = gen_id("el");
                seen.insert(fresh.clone());
                o.insert("id".into(), json!(fresh));
            }
        }
        Ok(())
    }

    /// All sensor ids the layout refers to (used to warn about unavailable ones).
    pub fn sensor_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        for e in &self.elements {
            if let Some(vals) = e.get("values").and_then(Value::as_array) {
                for v in vals {
                    if let Some(s) = v.get("sensorId").and_then(Value::as_str) {
                        out.push(s.to_string());
                    }
                }
            }
            if let Some(series) = e.get("series").and_then(Value::as_array) {
                for v in series {
                    if let Some(s) = v.get("sensorId").and_then(Value::as_str) {
                        out.push(s.to_string());
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// A user-editable copy with a fresh id.
    pub fn duplicate(&self, name: Option<String>) -> Layout {
        let mut l = self.clone();
        l.id = gen_id("lay");
        l.builtin = false;
        l.tags.retain(|t| t != "preset");
        l.name = name.unwrap_or_else(|| format!("{} (copy)", self.name));
        l.created_at = now_ms();
        l.updated_at = l.created_at;
        l
    }
}

// ───────────────────────────── presets ─────────────────────────────

fn val(sensor: &str) -> Value {
    json!({ "sensorId": sensor, "colorMode": "status" })
}
fn val_d(sensor: &str, decimals: u32) -> Value {
    json!({ "sensorId": sensor, "colorMode": "status", "decimals": decimals })
}
fn metric(id: &str, label: &str, values: Vec<Value>) -> Value {
    json!({ "id": id, "type": "metric", "visible": true, "label": { "mode": "text", "text": label }, "values": values })
}
fn metric_big(id: &str, label: &str, values: Vec<Value>, size: u32) -> Value {
    let mut m = metric(id, label, values);
    m["style"] = json!({ "fontSize": size, "fontWeight": 700 });
    m
}
fn with(mut v: Value, extra: Value) -> Value {
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    v
}
fn alerts_el() -> Value {
    json!({ "id": "alerts", "type": "alerts", "visible": true, "maxVisible": 3, "span": 12 })
}
fn frametime_graph(id: &str, w: u32, h: u32) -> Value {
    json!({ "id": id, "type": "graph", "visible": true, "source": "frames", "frames": 240,
            "series": [{ "sensorId": "frametime", "color": "#3ddcff" }],
            "w": w, "h": h, "fill": true, "grid": false, "showLabel": true, "title": "Frame time", "showValue": false, "span": 12 })
}
fn history_graph(id: &str, title: &str, sensors: &[(&str, &str)], w: u32, h: u32, min: Option<f64>, max: Option<f64>) -> Value {
    let series: Vec<Value> = sensors.iter().map(|(s, c)| json!({ "sensorId": s, "color": c })).collect();
    let mut g = json!({ "id": id, "type": "graph", "visible": true, "source": "history", "seconds": 60,
            "series": series, "w": w, "h": h, "fill": false, "grid": true, "showLabel": true, "title": title, "span": 12 });
    if let Some(m) = min {
        g["min"] = json!(m);
    }
    if let Some(m) = max {
        g["max"] = json!(m);
    }
    g
}

fn preset(id: &str, name: &str, description: &str, canvas_patch: Value, elements: Vec<Value>) -> Layout {
    let mut canvas = default_canvas();
    if let (Some(c), Some(p)) = (canvas.as_object_mut(), canvas_patch.as_object()) {
        for (k, v) in p {
            c.insert(k.clone(), v.clone());
        }
    }
    Layout {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        builtin: true,
        tags: vec!["preset".into()],
        canvas,
        elements,
        created_at: 0,
        updated_at: 0,
        ..Default::default()
    }
}

/// The ready-to-use layouts shipped with the app. They are read-only; users duplicate to customise.
pub fn presets() -> Vec<Layout> {
    use crate::sensors::ids::*;
    let minimal = preset(
        "preset.minimal",
        "Minimal",
        "Just FPS, GPU and CPU load in one small line. Stays out of the way.",
        json!({ "direction": "row", "gap": 14, "padding": 8, "background": { "color": "#05080c", "opacity": 0.5, "radius": 10, "borderOpacity": 0.0, "borderWidth": 0 } }),
        vec![
            metric_big("fps", "FPS", vec![val_d(FPS, 0)], 20),
            metric("gpu", "GPU", vec![val_d(GPU_LOAD, 0)]),
            metric("cpu", "CPU", vec![val_d(CPU_LOAD, 0)]),
            alerts_el(),
        ],
    );
    let gaming = preset(
        "preset.gaming",
        "Gaming",
        "FPS, smoothness, temperatures and memory, the essentials for playing.",
        json!({ "gap": 4, "width": 190 }),
        vec![
            metric_big("fps", "FPS", vec![val_d(FPS, 0)], 28),
            metric("ft", "Frame time", vec![val_d(FRAMETIME, 1)]),
            metric("low1", "1% Low", vec![val_d(FPS_LOW1, 0)]),
            frametime_graph("ftg", 170, 34),
            metric("cpu", "CPU", vec![val_d(CPU_LOAD, 0), val_d(CPU_TEMP, 0)]),
            metric("gpu", "GPU", vec![val_d(GPU_LOAD, 0), val_d(GPU_TEMP, 0)]),
            metric("vram", "VRAM", vec![val_d(GPU_VRAM_USED, 1)]),
            metric("ram", "RAM", vec![val_d(RAM_USED, 1)]),
            alerts_el(),
        ],
    );
    let detailed = preset(
        "preset.detailed",
        "Detailed",
        "Two-column readout with FPS lows, clocks, power and a frame-time graph.",
        json!({ "columns": 2, "gap": 5, "padding": 12, "width": 380 }),
        vec![
            metric_big("fps", "FPS", vec![val_d(FPS, 0)], 24),
            metric("ft", "Frame time", vec![val_d(FRAMETIME, 1)]),
            metric("avg", "Average", vec![val_d(FPS_AVG, 0)]),
            metric("low1", "1% Low", vec![val_d(FPS_LOW1, 0)]),
            metric("low01", "0.1% Low", vec![val_d(FPS_LOW01, 0)]),
            metric("min", "Min", vec![val_d(FPS_MIN, 0)]),
            metric("cpu", "CPU", vec![val_d(CPU_LOAD, 0), val_d(CPU_TEMP, 0)]),
            metric("cpuclk", "CPU clock", vec![val_d(CPU_CLOCK, 0), val_d(CPU_POWER, 0)]),
            metric("gpu", "GPU", vec![val_d(GPU_LOAD, 0), val_d(GPU_TEMP, 0)]),
            metric("gpuclk", "GPU clock", vec![val_d(GPU_CLOCK, 0), val_d(GPU_POWER, 0)]),
            metric("vram", "VRAM", vec![val_d(GPU_VRAM_USED, 1), val_d(GPU_VRAM_PERCENT, 0)]),
            metric("ram", "RAM", vec![val_d(RAM_USED, 1), val_d(RAM_PERCENT, 0)]),
            metric("disk", "Disk", vec![val_d(DISK_READ, 1), val_d(DISK_WRITE, 1)]),
            metric("net", "Network", vec![val_d(NET_DOWN, 1), val_d(NET_UP, 1)]),
            frametime_graph("ftg", 350, 44),
            alerts_el(),
        ],
    );
    let advanced = preset(
        "preset.advanced",
        "Advanced",
        "Everything: every FPS statistic, per-core CPU load, power, fans and live graphs.",
        json!({ "columns": 3, "gap": 6, "padding": 14, "width": 560 }),
        vec![
            metric_big("fps", "FPS", vec![val_d(FPS, 0)], 26),
            metric("ft", "Frame time", vec![val_d(FRAMETIME, 1), val_d(FRAMETIME_MAX, 1)]),
            metric("sess", "Session", vec![val(SESSION_TIME)]),
            metric("avg", "Average", vec![val_d(FPS_AVG, 0)]),
            metric("low1", "1% Low", vec![val_d(FPS_LOW1, 0)]),
            metric("low01", "0.1% Low", vec![val_d(FPS_LOW01, 0)]),
            metric("min", "Min", vec![val_d(FPS_MIN, 0)]),
            metric("max", "Max", vec![val_d(FPS_MAX, 0)]),
            metric("api", "API", vec![val(GAME_API)]),
            with(json!({ "id": "cores", "type": "cores", "visible": true, "metric": "load", "columns": 8, "barHeight": 18, "showNumbers": true }), json!({ "span": 3 })),
            metric("cpu", "CPU", vec![val_d(CPU_LOAD, 0), val_d(CPU_TEMP, 0)]),
            metric("cpuclk", "CPU clock", vec![val_d(CPU_CLOCK, 0)]),
            metric("cpupw", "CPU power", vec![val_d(CPU_POWER, 0)]),
            metric("gpu", "GPU", vec![val_d(GPU_LOAD, 0), val_d(GPU_TEMP, 0)]),
            metric("gpuclk", "GPU clock", vec![val_d(GPU_CLOCK, 0), val_d(GPU_MEMCLOCK, 0)]),
            metric("gpupw", "GPU power", vec![val_d(GPU_POWER, 0), val_d(GPU_FAN, 0)]),
            metric("vram", "VRAM", vec![val_d(GPU_VRAM_USED, 1), val_d(GPU_VRAM_PERCENT, 0)]),
            metric("ram", "RAM", vec![val_d(RAM_USED, 1), val_d(RAM_PERCENT, 0)]),
            metric("net", "Network", vec![val_d(NET_DOWN, 1), val_d(NET_UP, 1)]),
            metric("disk", "Disk", vec![val_d(DISK_READ, 1), val_d(DISK_WRITE, 1)]),
            metric("diskact", "Disk busy", vec![val_d(DISK_ACTIVE, 0)]),
            with(json!({ "id": "clock", "type": "clock", "visible": true, "mode": "time", "seconds": true, "hour12": false }), json!({ "span": 1 })),
            frametime_graph("ftg", 530, 50),
            history_graph("load", "CPU & GPU load", &[(GPU_LOAD, "#3ddcff"), (CPU_LOAD, "#ffb454")], 530, 56, Some(0.0), Some(100.0)),
            history_graph("temps", "Temperatures", &[(GPU_TEMP, "#ff6b6b"), (CPU_TEMP, "#ffd166")], 530, 56, None, None),
            alerts_el(),
        ],
    );
    let fps_only = preset(
        "preset.fps-only",
        "FPS only",
        "One big, readable FPS number and nothing else.",
        json!({ "padding": 4, "textShadow": "outline", "background": { "color": "#000000", "opacity": 0.0, "radius": 0, "borderOpacity": 0.0, "borderWidth": 0 } }),
        vec![
            with(metric_big("fps", "FPS", vec![val_d(FPS, 0)], 56), json!({ "arrange": "stacked" })),
            alerts_el(),
        ],
    );
    let thermals = preset(
        "preset.thermals",
        "Temperatures",
        "Keep an eye on heat, power and fans while you play.",
        json!({ "gap": 5, "width": 200 }),
        vec![
            metric("cput", "CPU temp", vec![val_d(CPU_TEMP, 0)]),
            metric("gput", "GPU temp", vec![val_d(GPU_TEMP, 0)]),
            metric("cpup", "CPU power", vec![val_d(CPU_POWER, 0)]),
            metric("gpup", "GPU power", vec![val_d(GPU_POWER, 0)]),
            metric("fan", "GPU fan", vec![val_d(GPU_FAN, 0)]),
            metric("thr", "GPU throttle", vec![val(GPU_THROTTLE)]),
            history_graph("tg", "Temperatures", &[(GPU_TEMP, "#ff6b6b"), (CPU_TEMP, "#ffd166")], 180, 40, None, None),
            alerts_el(),
        ],
    );
    let widget = preset(
        "preset.widget",
        "Desktop widget",
        "A compact card for your desktop: clock, load bars and network.",
        json!({ "gap": 7, "padding": 14, "width": 230, "background": { "color": "#0e131b", "opacity": 0.92, "radius": 16, "borderColor": "#ffffff", "borderOpacity": 0.1, "borderWidth": 1 } }),
        vec![
            with(json!({ "id": "clock", "type": "clock", "visible": true, "mode": "time", "seconds": false, "hour12": false, "style": { "fontSize": 30, "fontWeight": 700 } }), json!({})),
            with(metric("cpu", "CPU", vec![val_d(CPU_LOAD, 0)]), json!({ "display": "bar" })),
            with(metric("gpu", "GPU", vec![val_d(GPU_LOAD, 0)]), json!({ "display": "bar" })),
            with(metric("ram", "RAM", vec![val_d(RAM_PERCENT, 0)]), json!({ "display": "bar" })),
            metric("net", "Network", vec![val_d(NET_DOWN, 1), val_d(NET_UP, 1)]),
            metric("cput", "CPU temp", vec![val_d(CPU_TEMP, 0)]),
            metric("gput", "GPU temp", vec![val_d(GPU_TEMP, 0)]),
        ],
    );
    let blank = {
        let mut l = preset(
            "preset.blank",
            "Blank canvas",
            "An empty free-form canvas: place anything anywhere.",
            json!({ "arrangement": "free", "width": 360, "height": 180, "padding": 0 }),
            vec![with(
                metric_big("fps", "FPS", vec![val_d(FPS, 0)], 32),
                json!({ "x": 16, "y": 16, "w": 160, "h": 44 }),
            )],
        );
        l.tags.push("template".into());
        l
    };
    vec![minimal, gaming, detailed, advanced, fps_only, thermals, widget, blank]
}

pub fn preset_by_id(id: &str) -> Option<Layout> {
    presets().into_iter().find(|l| l.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_are_valid_and_survive_sanitize_unchanged() {
        for p in presets() {
            let mut copy = p.clone();
            copy.sanitize().unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert_eq!(copy.elements.len(), p.elements.len(), "{} lost elements", p.id);
            assert!(p.builtin && p.tags.contains(&"preset".to_string()));
            let mut ids = HashSet::new();
            for e in &p.elements {
                assert!(ids.insert(e["id"].as_str().unwrap().to_string()), "duplicate id in {}", p.id);
            }
        }
        let names: Vec<_> = presets().iter().map(|p| p.name.clone()).collect();
        for want in ["Minimal", "Gaming", "Detailed", "Advanced"] {
            assert!(names.iter().any(|n| n == want), "missing preset {want}");
        }
    }

    #[test]
    fn hostile_layout_is_neutralised() {
        let raw = json!({
            "id": "x", "name": "  ",
            "canvas": { "arrangement": "evil", "padding": 1e9, "textColor": "url(javascript:1)", "font": { "family": "a;}body{display:none" } },
            "elements": [
                { "type": "script", "src": "x" },
                { "type": "text", "text": "ok", "x": f64::NAN, "style": { "color": "red;position:fixed", "fontSize": 1e6 } },
                { "type": "image", "src": "https://evil.example/track.png" },
                { "type": "image", "src": "data:text/html;base64,PHNjcmlwdD4=" },
                { "type": "metric", "values": [{ "sensorId": "" }, { "sensorId": "cpu.load", "stat": "hax" }] },
                "not an object"
            ]
        });
        let mut l: Layout = serde_json::from_value(raw).unwrap();
        l.sanitize().unwrap();
        assert_eq!(l.name, "Untitled layout");
        let c = l.canvas.as_object().unwrap();
        assert!(!c.contains_key("arrangement"));
        assert_eq!(c["padding"], json!(100.0));
        assert!(!c.contains_key("textColor"));
        assert!(!c["font"].as_object().unwrap().contains_key("family"));
        assert_eq!(l.elements.len(), 4, "unknown type + non-object dropped");
        let text = &l.elements[0];
        assert!(!text.get("x").is_some_and(|v| v.is_number() && v.as_f64().unwrap().is_nan()));
        assert!(text["style"].get("color").is_none());
        assert_eq!(text["style"]["fontSize"], json!(200.0));
        assert!(l.elements[1].get("src").is_none(), "remote image URLs are not allowed");
        assert!(l.elements[2].get("src").is_none(), "non-image data URLs are not allowed");
        let m = &l.elements[3];
        assert_eq!(m["values"].as_array().unwrap().len(), 1, "empty sensor ids dropped");
        assert!(m["values"][0].get("stat").is_none());
    }

    #[test]
    fn data_image_is_kept_and_size_limited() {
        let ok = format!("data:image/png;base64,{}", "A".repeat(100));
        let big = format!("data:image/png;base64,{}", "A".repeat(MAX_IMAGE_BYTES));
        let mut l = Layout {
            name: "img".into(),
            elements: vec![json!({ "type": "image", "src": ok }), json!({ "type": "image", "src": big })],
            ..Default::default()
        };
        l.sanitize().unwrap();
        assert!(l.elements[0].get("src").is_some());
        assert!(l.elements[1].get("src").is_none());
    }

    #[test]
    fn too_many_elements_is_rejected() {
        let mut l = Layout {
            name: "x".into(),
            elements: (0..MAX_ELEMENTS + 1).map(|_| json!({ "type": "divider" })).collect(),
            ..Default::default()
        };
        assert!(l.sanitize().is_err());
    }

    #[test]
    fn duplicate_ids_are_repaired() {
        let mut l = Layout {
            name: "x".into(),
            elements: vec![json!({ "type": "divider", "id": "a" }), json!({ "type": "divider", "id": "a" }), json!({ "type": "divider" })],
            ..Default::default()
        };
        l.sanitize().unwrap();
        let ids: HashSet<_> = l.elements.iter().map(|e| e["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn duplicate_makes_editable_copy() {
        let g = preset_by_id("preset.gaming").unwrap();
        let d = g.duplicate(None);
        assert!(!d.builtin && d.id != g.id && d.name == "Gaming (copy)");
        assert!(!d.tags.contains(&"preset".to_string()));
    }

    #[test]
    fn sensor_ids_are_collected() {
        let g = preset_by_id("preset.gaming").unwrap();
        let ids = g.sensor_ids();
        assert!(ids.contains(&"fps".to_string()) && ids.contains(&"frametime".to_string()) && ids.contains(&"cpu.load".to_string()));
    }
}
