//! Tiny logger: keeps recent lines in memory for the in-app log viewer and diagnostics,
//! and appends to a size-limited file.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

const RING: usize = 600;
const MAX_FILE: u64 = 1_000_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub t: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

struct Logger {
    ring: Mutex<VecDeque<LogLine>>,
    file: Mutex<Option<(PathBuf, File)>>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = LogLine {
            t: fpsframer_core::util::now_ms(),
            level: r.level().to_string(),
            target: r.target().to_string(),
            message: r.args().to_string(),
        };
        if let Some((path, f)) = self.file.lock().as_mut() {
            let _ = writeln!(f, "{} {:<5} {} {}", line.t, line.level, line.target, line.message);
            if f.metadata().is_ok_and(|m| m.len() > MAX_FILE) {
                let old = path.with_extension("old.log");
                let _ = fs::rename(&*path, old);
                if let Ok(nf) = File::create(&*path) {
                    *f = nf;
                }
            }
        }
        let mut ring = self.ring.lock();
        if ring.len() >= RING {
            ring.pop_front();
        }
        ring.push_back(line);
    }
    fn flush(&self) {}
}

/// Install the logger once. Safe to call repeatedly.
pub fn init(log_dir: PathBuf) {
    let _ = fs::create_dir_all(&log_dir);
    let path = log_dir.join("fpsframer.log");
    let file = OpenOptions::new().create(true).append(true).open(&path).ok().map(|f| (path, f));
    let l = LOGGER.get_or_init(|| Logger { ring: Mutex::new(VecDeque::new()), file: Mutex::new(file) });
    let _ = log::set_logger(l);
    log::set_max_level(log::LevelFilter::Info);
}

pub fn recent(n: usize) -> Vec<LogLine> {
    LOGGER.get().map(|l| l.ring.lock().iter().rev().take(n).cloned().collect::<Vec<_>>()).map(|mut v| {
        v.reverse();
        v
    }).unwrap_or_default()
}

pub fn recent_problem_count() -> usize {
    LOGGER.get().map_or(0, |l| l.ring.lock().iter().filter(|x| x.level == "ERROR" || x.level == "WARN").count())
}

/// Remove personal information (the Windows user name in paths) before a report is shared.
pub fn redact(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    while let Some(p) = lower[pos..].find("\\users\\") {
        let name_start = pos + p + 7;
        out.push_str(&s[pos..name_start]);
        let after = &s[name_start..];
        let end = after.find(['\\', '/', '"', '\'']).unwrap_or(after.len());
        out.push_str("<user>");
        pos = name_start + end;
    }
    out.push_str(&s[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_windows_user_names() {
        assert_eq!(redact(r"C:\Users\Alice\AppData\Roaming\FPSFramer"), r"C:\Users\<user>\AppData\Roaming\FPSFramer");
        assert_eq!(redact("nothing to hide"), "nothing to hide");
        assert_eq!(redact(r"a C:\Users\Bob\x and D:\Users\Carol Jones\x"), r"a C:\Users\<user>\x and D:\Users\<user>\x");
    }
}
