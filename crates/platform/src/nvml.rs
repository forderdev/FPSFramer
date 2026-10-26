//! NVIDIA GPU sensors through NVML (`nvml.dll` / `libnvidia-ml.so`), loaded at
//! run time. If the library is missing (no NVIDIA driver) loading simply fails
//! and the GPU falls back to the generic sources.

use fpsframer_core::sensors::{Readings, ReadingsExt, Unit};
use libloading::Library;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint, c_ulonglong, c_void};

type Device = *mut c_void;

#[repr(C)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}
#[repr(C)]
struct MemoryInfo {
    total: c_ulonglong,
    free: c_ulonglong,
    used: c_ulonglong,
}

const NVML_SUCCESS: c_int = 0;
const TEMPERATURE_GPU: c_uint = 0;
const CLOCK_GRAPHICS: c_uint = 0;
const CLOCK_MEM: c_uint = 2;

const THROTTLE_SW_POWER_CAP: u64 = 0x4;
const THROTTLE_HW_SLOWDOWN: u64 = 0x8;
const THROTTLE_SW_THERMAL: u64 = 0x20;
const THROTTLE_HW_THERMAL: u64 = 0x40;
const THROTTLE_HW_POWER_BRAKE: u64 = 0x80;

type FnInit = unsafe extern "C" fn() -> c_int;
type FnShutdown = unsafe extern "C" fn() -> c_int;
type FnCount = unsafe extern "C" fn(*mut c_uint) -> c_int;
type FnHandle = unsafe extern "C" fn(c_uint, *mut Device) -> c_int;
type FnName = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> c_int;
type FnTemp = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> c_int;
type FnUtil = unsafe extern "C" fn(Device, *mut Utilization) -> c_int;
type FnMem = unsafe extern "C" fn(Device, *mut MemoryInfo) -> c_int;
type FnClock = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> c_int;
type FnU32 = unsafe extern "C" fn(Device, *mut c_uint) -> c_int;
type FnU64 = unsafe extern "C" fn(Device, *mut c_ulonglong) -> c_int;
type FnDriver = unsafe extern "C" fn(*mut c_char, c_uint) -> c_int;

pub struct Nvml {
    _lib: Library,
    shutdown: FnShutdown,
    count: FnCount,
    handle: FnHandle,
    name: FnName,
    temp: FnTemp,
    util: FnUtil,
    mem: FnMem,
    clock: FnClock,
    power: FnU32,
    fan: FnU32,
    throttle: Option<FnU64>,
    driver: Option<FnDriver>,
}

// SAFETY: NVML is documented as thread-safe; we only hold function pointers.
unsafe impl Send for Nvml {}

pub struct NvmlDevice {
    handle: Device,
    pub name: String,
}
unsafe impl Send for NvmlDevice {}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NvmlSample {
    pub temp_c: Option<f64>,
    pub util_pct: Option<f64>,
    pub vram_used_mb: Option<f64>,
    pub vram_total_mb: Option<f64>,
    pub clock_gfx: Option<f64>,
    pub clock_mem: Option<f64>,
    pub power_w: Option<f64>,
    pub fan_pct: Option<f64>,
    pub throttle_thermal: Option<bool>,
    pub throttle_power: Option<bool>,
}

fn candidate_paths() -> Vec<String> {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
        // Absolute paths only: never search the application folder for a DLL called nvml.dll.
        vec![
            format!("{root}\\System32\\nvml.dll"),
            format!("{pf}\\NVIDIA Corporation\\NVSMI\\nvml.dll"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec!["libnvidia-ml.so.1".into(), "libnvidia-ml.so".into()]
    }
}

impl Nvml {
    pub fn load() -> Result<Nvml, String> {
        let mut last = String::from("NVML not found");
        for p in candidate_paths() {
            // SAFETY: loading a vendor library from a fixed system path.
            match unsafe { Library::new(&p) } {
                Ok(lib) => return Self::bind(lib),
                Err(e) => last = format!("{p}: {e}"),
            }
        }
        Err(last)
    }

    fn bind(lib: Library) -> Result<Nvml, String> {
        // SAFETY: symbol names and signatures follow nvml.h.
        unsafe {
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *lib.get::<$ty>(concat!($name, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $name))?
                };
            }
            let init: FnInit = sym!("nvmlInit_v2", FnInit);
            let rc = init();
            if rc != NVML_SUCCESS {
                return Err(format!("nvmlInit failed ({rc})"));
            }
            let throttle = lib
                .get::<FnU64>(b"nvmlDeviceGetCurrentClocksThrottleReasons\0")
                .or_else(|_| lib.get::<FnU64>(b"nvmlDeviceGetCurrentClocksEventReasons\0"))
                .ok()
                .map(|s| *s);
            let driver = lib.get::<FnDriver>(b"nvmlSystemGetDriverVersion\0").ok().map(|s| *s);
            Ok(Nvml {
                shutdown: sym!("nvmlShutdown", FnShutdown),
                count: sym!("nvmlDeviceGetCount_v2", FnCount),
                handle: sym!("nvmlDeviceGetHandleByIndex_v2", FnHandle),
                name: sym!("nvmlDeviceGetName", FnName),
                temp: sym!("nvmlDeviceGetTemperature", FnTemp),
                util: sym!("nvmlDeviceGetUtilizationRates", FnUtil),
                mem: sym!("nvmlDeviceGetMemoryInfo", FnMem),
                clock: sym!("nvmlDeviceGetClockInfo", FnClock),
                power: sym!("nvmlDeviceGetPowerUsage", FnU32),
                fan: sym!("nvmlDeviceGetFanSpeed", FnU32),
                throttle,
                driver,
                _lib: lib,
            })
        }
    }

    pub fn driver_version(&self) -> Option<String> {
        let f = self.driver?;
        let mut buf = [0 as c_char; 96];
        // SAFETY: buffer is valid for 96 bytes.
        unsafe {
            if f(buf.as_mut_ptr(), buf.len() as c_uint) != NVML_SUCCESS {
                return None;
            }
            Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string())
        }
    }

    pub fn devices(&self) -> Vec<NvmlDevice> {
        let mut n: c_uint = 0;
        // SAFETY: out-parameters are valid.
        unsafe {
            if (self.count)(&mut n) != NVML_SUCCESS {
                return Vec::new();
            }
            (0..n)
                .filter_map(|i| {
                    let mut h: Device = std::ptr::null_mut();
                    if (self.handle)(i, &mut h) != NVML_SUCCESS || h.is_null() {
                        return None;
                    }
                    let mut buf = [0 as c_char; 96];
                    let name = if (self.name)(h, buf.as_mut_ptr(), buf.len() as c_uint) == NVML_SUCCESS {
                        CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string()
                    } else {
                        format!("NVIDIA GPU {i}")
                    };
                    Some(NvmlDevice { handle: h, name })
                })
                .collect()
        }
    }

    pub fn sample(&self, d: &NvmlDevice) -> NvmlSample {
        let mut s = NvmlSample::default();
        // SAFETY: handle came from NVML; all out-parameters are valid locals.
        unsafe {
            let mut t: c_uint = 0;
            if (self.temp)(d.handle, TEMPERATURE_GPU, &mut t) == NVML_SUCCESS {
                s.temp_c = Some(t as f64);
            }
            let mut u = Utilization { gpu: 0, memory: 0 };
            if (self.util)(d.handle, &mut u) == NVML_SUCCESS {
                s.util_pct = Some(u.gpu as f64);
            }
            let mut m = MemoryInfo { total: 0, free: 0, used: 0 };
            if (self.mem)(d.handle, &mut m) == NVML_SUCCESS && m.total > 0 {
                s.vram_used_mb = Some(m.used as f64 / 1_048_576.0);
                s.vram_total_mb = Some(m.total as f64 / 1_048_576.0);
            }
            let mut c: c_uint = 0;
            if (self.clock)(d.handle, CLOCK_GRAPHICS, &mut c) == NVML_SUCCESS {
                s.clock_gfx = Some(c as f64);
            }
            if (self.clock)(d.handle, CLOCK_MEM, &mut c) == NVML_SUCCESS {
                s.clock_mem = Some(c as f64);
            }
            let mut p: c_uint = 0;
            if (self.power)(d.handle, &mut p) == NVML_SUCCESS {
                s.power_w = Some(p as f64 / 1000.0);
            }
            let mut f: c_uint = 0;
            if (self.fan)(d.handle, &mut f) == NVML_SUCCESS {
                s.fan_pct = Some(f as f64);
            }
            if let Some(th) = self.throttle {
                let mut r: c_ulonglong = 0;
                if th(d.handle, &mut r) == NVML_SUCCESS {
                    let (thermal, power) = decode_throttle(r);
                    s.throttle_thermal = Some(thermal);
                    s.throttle_power = Some(power);
                }
            }
        }
        s
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: matches the nvmlInit in `bind`.
        unsafe {
            (self.shutdown)();
        }
    }
}

/// `(thermal slowdown, power slowdown)` from the NVML throttle-reason bitmask.
pub fn decode_throttle(reasons: u64) -> (bool, bool) {
    let thermal = reasons & (THROTTLE_SW_THERMAL | THROTTLE_HW_THERMAL | THROTTLE_HW_SLOWDOWN) != 0;
    let power = reasons & (THROTTLE_SW_POWER_CAP | THROTTLE_HW_POWER_BRAKE) != 0;
    (thermal, power)
}

/// Write a sample into `out` using the GPU's sensor prefix (`gpu`, `gpu2`…).
pub fn write_sample(out: &mut Readings, prefix: &str, s: &NvmlSample) {
    let id = |f: &str| format!("{prefix}.{f}");
    if let Some(v) = s.util_pct {
        out.put(&id("load"), v, Unit::Percent);
    }
    if let Some(v) = s.temp_c {
        out.put(&id("temp"), v, Unit::Celsius);
    }
    if let Some(v) = s.clock_gfx {
        out.put(&id("clock"), v, Unit::Mhz);
    }
    if let Some(v) = s.clock_mem {
        out.put(&id("memclock"), v, Unit::Mhz);
    }
    if let Some(v) = s.power_w {
        out.put(&id("power"), v, Unit::Watt);
    }
    if let Some(v) = s.fan_pct {
        out.put(&id("fan"), v, Unit::Percent);
    }
    if let (Some(u), Some(t)) = (s.vram_used_mb, s.vram_total_mb) {
        out.put(&id("vram.used"), u, Unit::Megabytes);
        out.put(&id("vram.total"), t, Unit::Megabytes);
        out.put(&id("vram.percent"), u * 100.0 / t, Unit::Percent);
    }
    if let (Some(th), Some(pw)) = (s.throttle_thermal, s.throttle_power) {
        out.put(&id("throttle.thermal"), th as u8 as f64, Unit::Count);
        out.put(&id("throttle.power"), pw as u8 as f64, Unit::Count);
        let text = match (th, pw) {
            (false, false) => "None",
            (true, false) => "Heat",
            (false, true) => "Power limit",
            (true, true) => "Heat + power limit",
        };
        out.put_text(&id("throttle"), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpsframer_core::sensors::ReadingsExt;

    #[test]
    fn throttle_bitmask_decoding() {
        assert_eq!(decode_throttle(0), (false, false));
        assert_eq!(decode_throttle(0x1), (false, false), "GPU idle is not a slowdown");
        assert_eq!(decode_throttle(0x40), (true, false));
        assert_eq!(decode_throttle(0x4), (false, true));
        assert_eq!(decode_throttle(0x44), (true, true));
    }

    #[test]
    fn sample_writes_only_what_was_read() {
        let mut out = Readings::new();
        let s = NvmlSample { util_pct: Some(0.0), temp_c: Some(55.0), vram_used_mb: Some(2048.0), vram_total_mb: Some(8192.0), ..Default::default() };
        write_sample(&mut out, "gpu", &s);
        assert_eq!(out.num("gpu.load"), Some(0.0), "0% load is a real reading");
        assert_eq!(out.num("gpu.vram.percent"), Some(25.0));
        assert_eq!(out.num("gpu.fan"), None, "an unsupported fan sensor must stay absent");
        assert_eq!(out.num("gpu.power"), None);
    }

    #[test]
    fn missing_library_is_a_clean_error() {
        // CI machines have no NVIDIA driver; this must not panic.
        if let Err(e) = Nvml::load() {
            assert!(!e.is_empty());
        }
    }
}
