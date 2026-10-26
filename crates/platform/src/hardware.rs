//! A short description of this PC, stored with sessions and shown in diagnostics.

use fpsframer_core::session::HardwareInfo;

pub fn collect(gpu_names: &[String]) -> HardwareInfo {
    #[cfg(windows)]
    {
        crate::windows::hardware::collect(gpu_names)
    }
    #[cfg(not(windows))]
    {
        let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        let cpuinfo = read("/proc/cpuinfo");
        let model = cpuinfo
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown CPU".into());
        let threads = cpuinfo.lines().filter(|l| l.starts_with("processor")).count();
        let mem_kb: f64 = read("/proc/meminfo")
            .lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
            .unwrap_or(0.0);
        let os = read("/etc/os-release")
            .lines()
            .find(|l| l.starts_with("PRETTY_NAME="))
            .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
            .unwrap_or_else(|| "Linux".into());
        HardwareInfo {
            cpu: format!("{model} ({threads} threads)"),
            gpus: gpu_names.to_vec(),
            ram_gb: (mem_kb / 1_048_576.0 * 10.0).round() / 10.0,
            os,
            displays: vec![],
            app_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    #[test]
    fn collects_something_plausible() {
        let h = super::collect(&["GPU X".into()]);
        assert!(!h.cpu.is_empty() && h.ram_gb > 0.1);
        assert_eq!(h.gpus, vec!["GPU X".to_string()]);
    }
}
