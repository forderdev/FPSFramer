//! Standard sensor descriptors with beginner-friendly names and explanations.
//! Providers build their catalogs from these so wording stays consistent.

use fpsframer_core::sensors::{ids, Category, SensorDesc, StatusRule, Unavailable, UnavailableCode, Unit};

pub fn fps_family(unavailable: Option<&Unavailable>) -> Vec<SensorDesc> {
    let mk = |id: &str, name: &str, short: &str, unit: Unit, desc: &str| {
        let mut d = SensorDesc::new(id, name, Category::Fps, unit).short(short).desc(desc).source("capture");
        if let Some(u) = unavailable {
            d = d.unavailable(u.code, u.message.clone(), u.fix.as_deref());
        }
        d
    };
    vec![
        mk(ids::FPS, "FPS (frames per second)", "FPS", Unit::Fps,
            "How many pictures the game draws every second. Higher is smoother. 60 is a common goal; 144 or more feels great in fast games.")
            .kw(&["fps", "frames", "framerate", "frame rate"]),
        mk(ids::FRAMETIME, "Frame time", "Frame time", Unit::Ms,
            "How long each picture takes to draw, in milliseconds. Lower is better. 16.7 ms equals 60 FPS and 6.9 ms equals 144 FPS. Big jumps show up as stutter.")
            .kw(&["frametime", "ms", "latency"]),
        mk(ids::FRAMETIME_MAX, "Slowest frame (last second)", "Slowest", Unit::Ms,
            "The longest a single picture took to draw during the last second. A high number means a visible hitch.")
            .kw(&["spike", "stutter", "hitch"]),
        mk(ids::FPS_AVG, "Average FPS", "Avg", Unit::Fps,
            "Average frame rate over the statistics window (set in Settings → Monitoring).")
            .kw(&["average", "mean"]),
        mk(ids::FPS_MIN, "Minimum FPS", "Min", Unit::Fps,
            "The frame rate of the single slowest frame in the window. One hitch can make this very low, so 1% Low is usually a fairer measure.")
            .kw(&["minimum", "lowest"]),
        mk(ids::FPS_MAX, "Maximum FPS", "Max", Unit::Fps,
            "The frame rate of the single fastest frame in the window.")
            .kw(&["maximum", "highest", "peak"]),
        mk(ids::FPS_LOW1, "1% Low FPS", "1% Low", Unit::Fps,
            "The average FPS of your slowest 1% of frames. It shows how bad the dips feel. If it is close to your average FPS the game feels smooth; if it is far lower you will notice stutter. Needs about 100 frames of data.")
            .kw(&["1 percent low", "one percent", "lows", "stutter"]),
        mk(ids::FPS_LOW01, "0.1% Low FPS", "0.1% Low", Unit::Fps,
            "The average FPS of your slowest 0.1% of frames, the very worst hitches. Needs about 1,000 frames of data, so it appears after a short while.")
            .kw(&["0.1 percent low", "point one", "lows"]),
    ]
}

pub fn game_family(unavailable: Option<&Unavailable>) -> Vec<SensorDesc> {
    let mut v = vec![
        SensorDesc::new(ids::GAME_NAME, "Game / application", Category::System, Unit::Text)
            .short("Game")
            .desc("The program FPSFramer is currently measuring.")
            .source("capture"),
        SensorDesc::new(ids::GAME_API, "Graphics API", Category::System, Unit::Text)
            .short("API")
            .desc("The graphics technology the game uses to draw (for example DirectX 11 or 12).")
            .source("capture"),
        SensorDesc::new(ids::SESSION_TIME, "Session time", Category::System, Unit::Seconds)
            .short("Session")
            .desc("How long the current recording has been running.")
            .source("engine"),
    ];
    if let Some(u) = unavailable {
        for d in v.iter_mut().take(2) {
            d.availability = fpsframer_core::sensors::Availability::no(u.code, u.message.clone(), u.fix.as_deref());
        }
    }
    v
}

pub fn no_active_game() -> Unavailable {
    Unavailable {
        code: UnavailableCode::NoActiveGame,
        message: "Shown while a game is running and being measured.".into(),
        fix: Some("Start a game, or pick a program under Settings → Monitoring.".into()),
    }
}

// ───────────────────────────── CPU ─────────────────────────────

pub fn cpu_load(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_LOAD, "CPU usage", Category::Cpu, Unit::Percent)
        .short("CPU")
        .desc("How busy the processor is overall, from 0 to 100%. Games often use a few cores heavily, so check the busiest core as well.")
        .rule(StatusRule::higher(90.0, 98.0))
        .range(0.0, 100.0)
        .rec()
        .source(source)
        .kw(&["cpu", "processor", "load", "utilization", "usage"])
}

pub fn cpu_core_max(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_CORE_MAX, "Busiest CPU core", Category::Cpu, Unit::Percent)
        .short("Top core")
        .desc("The usage of the single busiest processor core. A game can be held back by one overloaded core even when overall CPU usage looks low.")
        .rule(StatusRule::higher(90.0, 98.0))
        .range(0.0, 100.0)
        .rec()
        .source(source)
        .kw(&["core", "single thread", "bottleneck"])
}

pub fn cpu_core_load(i: usize, source: &str) -> SensorDesc {
    SensorDesc::new(format!("cpu.core.{i}.load"), format!("CPU core {} usage", i + 1), Category::Cpu, Unit::Percent)
        .short(format!("C{}", i + 1))
        .group(format!("Core {}", i + 1))
        .desc("How busy this processor core (or hyper-thread) is.")
        .rule(StatusRule::higher(90.0, 98.0))
        .range(0.0, 100.0)
        .source(source)
        .kw(&["core", "thread"])
}

pub fn cpu_core_clock(i: usize, source: &str) -> SensorDesc {
    SensorDesc::new(format!("cpu.core.{i}.clock"), format!("CPU core {} speed", i + 1), Category::Cpu, Unit::Mhz)
        .short(format!("C{} MHz", i + 1))
        .group(format!("Core {}", i + 1))
        .desc("Current speed of this core in MHz.")
        .source(source)
}

pub fn cpu_clock(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_CLOCK, "CPU speed", Category::Cpu, Unit::Mhz)
        .short("CPU clock")
        .desc("Current processor speed in MHz. It rises under load and falls at idle to save power.")
        .rec()
        .source(source)
        .kw(&["clock", "frequency", "mhz", "ghz"])
}

pub fn cpu_name(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_NAME, "CPU model", Category::Cpu, Unit::Text)
        .short("CPU model")
        .desc("The name of your processor.")
        .source(source)
}

pub fn needs_companion(what: &str, sensor_hint: &str) -> (UnavailableCode, String, Option<String>) {
    (
        UnavailableCode::NeedsCompanion,
        format!("Windows does not let apps read {what} without a special hardware driver, and FPSFramer does not install drivers."),
        Some(format!("Run HWiNFO (turn on \"Shared Memory Support\") or LibreHardwareMonitor next to FPSFramer. {sensor_hint} will then appear automatically.")),
    )
}

pub fn cpu_temp_unavailable(source: &str) -> SensorDesc {
    let (c, m, f) = needs_companion("CPU temperature", "CPU temperature");
    cpu_temp(source).unavailable(c, m, f.as_deref())
}

pub fn cpu_temp(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_TEMP, "CPU temperature", Category::Thermal, Unit::Celsius)
        .short("CPU temp")
        .desc("How hot the processor is. Under about 80 °C is comfortable for most CPUs. Much higher can make the CPU slow itself down to protect itself.")
        .rule(StatusRule::higher(80.0, 92.0))
        .rec()
        .source(source)
        .kw(&["temperature", "heat", "thermal", "hot", "cpu"])
}

pub fn cpu_power(source: &str) -> SensorDesc {
    SensorDesc::new(ids::CPU_POWER, "CPU power", Category::Power, Unit::Watt)
        .short("CPU power")
        .desc("How much electrical power the processor is using, in watts.")
        .rec()
        .source(source)
        .kw(&["watt", "wattage", "power", "cpu"])
}

// ───────────────────────────── memory ─────────────────────────────

pub fn ram_set(source: &str) -> Vec<SensorDesc> {
    vec![
        SensorDesc::new(ids::RAM_USED, "Memory (RAM) in use", Category::Memory, Unit::Megabytes)
            .short("RAM")
            .desc("How much of your computer's main memory is being used by Windows and all running programs.")
            .rec()
            .source(source)
            .kw(&["ram", "memory"]),
        SensorDesc::new(ids::RAM_TOTAL, "Memory (RAM) installed", Category::Memory, Unit::Megabytes)
            .short("RAM total")
            .desc("Total main memory installed in this PC.")
            .source(source),
        SensorDesc::new(ids::RAM_AVAILABLE, "Memory (RAM) available", Category::Memory, Unit::Megabytes)
            .short("RAM free")
            .desc("Memory that programs can use right now without Windows having to free something first.")
            .source(source),
        SensorDesc::new(ids::RAM_PERCENT, "Memory (RAM) usage", Category::Memory, Unit::Percent)
            .short("RAM %")
            .desc("Percentage of main memory in use. Above about 90% Windows may slow down by using the disk as extra memory.")
            .rule(StatusRule::higher(80.0, 92.0))
            .range(0.0, 100.0)
            .rec()
            .source(source)
            .kw(&["ram", "memory", "usage"]),
    ]
}

// ───────────────────────────── storage / network / power ─────────────────────────────

pub fn disk_set(source: &str) -> Vec<SensorDesc> {
    vec![
        SensorDesc::new(ids::DISK_READ, "Disk read speed", Category::Storage, Unit::BytesPerSec)
            .short("Read")
            .desc("How fast data is being read from your drives, for example while a game loads levels.")
            .rec()
            .source(source)
            .kw(&["disk", "ssd", "hdd", "storage", "read"]),
        SensorDesc::new(ids::DISK_WRITE, "Disk write speed", Category::Storage, Unit::BytesPerSec)
            .short("Write")
            .desc("How fast data is being written to your drives.")
            .rec()
            .source(source)
            .kw(&["disk", "ssd", "hdd", "storage", "write"]),
        SensorDesc::new(ids::DISK_ACTIVE, "Disk activity", Category::Storage, Unit::Percent)
            .short("Disk busy")
            .desc("How much of the time your busiest drive is working. Near 100% means the drive is a bottleneck and things may feel slow.")
            .range(0.0, 100.0)
            .rec()
            .source(source)
            .kw(&["disk", "active time", "busy"]),
        SensorDesc::new(ids::DISK_LATENCY, "Disk response time", Category::Storage, Unit::Ms)
            .short("Disk latency")
            .desc("How long the drive takes to answer a request. SSDs are usually under 1 ms; a hard disk 10 to 20 ms. Very high values cause stutter while loading.")
            .rule(StatusRule::higher(25.0, 100.0))
            .source(source)
            .kw(&["disk", "latency", "response"]),
    ]
}

pub fn net_set(source: &str) -> Vec<SensorDesc> {
    vec![
        SensorDesc::new(ids::NET_DOWN, "Download speed", Category::Network, Unit::BytesPerSec)
            .short("Down")
            .desc("Data arriving from the internet or your network right now (all apps together).")
            .rec()
            .source(source)
            .kw(&["network", "download", "internet", "bandwidth"]),
        SensorDesc::new(ids::NET_UP, "Upload speed", Category::Network, Unit::BytesPerSec)
            .short("Up")
            .desc("Data leaving your PC right now (all apps together).")
            .rec()
            .source(source)
            .kw(&["network", "upload", "internet", "bandwidth"]),
    ]
}

pub fn battery_set(source: &str) -> Vec<SensorDesc> {
    vec![
        SensorDesc::new(ids::BATTERY_PERCENT, "Battery charge", Category::Power, Unit::Percent)
            .short("Battery")
            .desc("How full the laptop battery is.")
            .rule(StatusRule::lower(30.0, 10.0))
            .range(0.0, 100.0)
            .rec()
            .source(source)
            .kw(&["battery", "laptop"]),
        SensorDesc::new("battery.discharging", "Running on battery", Category::Power, Unit::Count)
            .short("On battery")
            .desc("1 when the laptop is running on battery power, 0 when plugged in. Many laptops reduce performance on battery.")
            .rec()
            .source(source)
            .kw(&["battery", "plugged", "ac"]),
    ]
}

// ───────────────────────────── GPU ─────────────────────────────

/// `prefix` is `"gpu"` for the primary GPU and `"gpu2"`, `"gpu3"`… for others.
pub fn gpu_id(prefix: &str, field: &str) -> String {
    format!("{prefix}.{field}")
}

pub fn gpu_prefix(index: usize) -> String {
    if index == 0 {
        "gpu".into()
    } else {
        format!("gpu{}", index + 1)
    }
}

pub struct GpuLabel {
    pub group: Option<String>,
    pub name_prefix: String,
}

pub fn gpu_label(index: usize, gpu_name: &str, multi: bool) -> GpuLabel {
    if multi {
        GpuLabel { group: Some(format!("GPU {} · {}", index + 1, gpu_name)), name_prefix: format!("GPU {} ", index + 1) }
    } else {
        GpuLabel { group: None, name_prefix: "GPU ".into() }
    }
}

#[derive(Clone, Copy)]
pub struct GpuField {
    pub field: &'static str,
    pub name: &'static str,
    pub short: &'static str,
    pub unit: Unit,
    pub desc: &'static str,
    pub rule: Option<StatusRule>,
    pub rec: bool,
}

pub const GPU_FIELDS: &[GpuField] = &[
    GpuField { field: "load", name: "usage", short: "GPU", unit: Unit::Percent, rule: None, rec: true,
        desc: "How busy the graphics card is. In games, 90 to 100% means the card is working at full capacity. That is normal, and also means the graphics card is what limits your FPS." },
    GpuField { field: "temp", name: "temperature", short: "GPU temp", unit: Unit::Celsius, rule: Some(StatusRule::higher(78.0, 88.0)), rec: true,
        desc: "How hot the graphics chip is. Most cards are happy below about 80 °C; much hotter and they slow themselves down." },
    GpuField { field: "clock", name: "core speed", short: "GPU clock", unit: Unit::Mhz, rule: None, rec: true,
        desc: "Current speed of the graphics chip in MHz. It rises under load; a sudden drop while hot can mean thermal throttling." },
    GpuField { field: "memclock", name: "memory speed", short: "VRAM clock", unit: Unit::Mhz, rule: None, rec: true,
        desc: "Current speed of the graphics card's memory in MHz." },
    GpuField { field: "power", name: "power", short: "GPU power", unit: Unit::Watt, rule: None, rec: true,
        desc: "How much electrical power the graphics card is using, in watts." },
    GpuField { field: "fan", name: "fan speed", short: "GPU fan", unit: Unit::Percent, rule: None, rec: true,
        desc: "How fast the graphics card fan is spinning, as a percentage of its maximum. 0% can be normal: many cards stop their fans when cool." },
    GpuField { field: "vram.used", name: "video memory (VRAM) in use", short: "VRAM", unit: Unit::Megabytes, rule: None, rec: true,
        desc: "How much of the graphics card's own memory is in use. Games store textures here. If it fills up, you may see stutter." },
    GpuField { field: "vram.total", name: "video memory (VRAM) installed", short: "VRAM total", unit: Unit::Megabytes, rule: None, rec: false,
        desc: "Total memory on the graphics card." },
    GpuField { field: "vram.percent", name: "video memory (VRAM) usage", short: "VRAM %", unit: Unit::Percent, rule: Some(StatusRule::higher(85.0, 95.0)), rec: true,
        desc: "Percentage of the graphics card's memory in use. Above about 95% games may stutter; lowering texture quality helps." },
    GpuField { field: "throttle.thermal", name: "heat slowdown active", short: "Heat limit", unit: Unit::Count, rule: None, rec: true,
        desc: "1 while the graphics driver reports slowing the card down because it is too hot, otherwise 0." },
    GpuField { field: "throttle.power", name: "power slowdown active", short: "Power limit", unit: Unit::Count, rule: None, rec: true,
        desc: "1 while the graphics driver reports slowing the card down because it hit its power limit, otherwise 0." },
];

pub fn gpu_desc(index: usize, gpu_name: &str, multi: bool, f: &GpuField, source: &str) -> SensorDesc {
    let prefix = gpu_prefix(index);
    let l = gpu_label(index, gpu_name, multi);
    let mut d = SensorDesc::new(gpu_id(&prefix, f.field), format!("{}{}", l.name_prefix, f.name), Category::Gpu, f.unit)
        .short(if multi { format!("{} {}", l.name_prefix.trim(), f.short.trim_start_matches("GPU ")) } else { f.short.to_string() })
        .desc(f.desc)
        .source(source)
        .kw(&["gpu", "graphics", "video card", f.field]);
    if matches!(f.field, "temp") {
        d.category = Category::Thermal;
    }
    if matches!(f.field, "power") {
        d.category = Category::Power;
    }
    if matches!(f.field, "fan") {
        d.category = Category::Fan;
    }
    if matches!(f.field, "vram.used" | "vram.total" | "vram.percent") {
        d.category = Category::Gpu;
    }
    if let Some(r) = f.rule {
        d = d.rule(r);
    }
    if f.unit == Unit::Percent {
        d = d.range(0.0, 100.0);
    }
    if f.rec {
        d = d.rec();
    }
    if let Some(g) = l.group {
        d = d.group(g);
    }
    d
}

pub fn gpu_text_desc(index: usize, gpu_name: &str, multi: bool, field: &str, name: &str, desc: &str, source: &str) -> SensorDesc {
    let l = gpu_label(index, gpu_name, multi);
    let mut d = SensorDesc::new(gpu_id(&gpu_prefix(index), field), format!("{}{}", l.name_prefix, name), Category::Gpu, Unit::Text)
        .short(name.to_string())
        .desc(desc)
        .source(source);
    if let Some(g) = l.group {
        d = d.group(g);
    }
    d
}

pub fn gpu_unavailable_hint(vendor: &str) -> (UnavailableCode, String, Option<String>) {
    (
        UnavailableCode::NeedsCompanion,
        format!("Windows only reports usage and memory for {vendor} graphics cards. Temperature, clocks, power and fan speed need the card maker's driver interface, which FPSFramer does not include yet."),
        Some("Run HWiNFO (Shared Memory Support on) or LibreHardwareMonitor next to FPSFramer to add them.".into()),
    )
}
