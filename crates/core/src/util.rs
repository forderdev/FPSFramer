use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Short unique id such as `lay_k3f9a1c2`. Not cryptographic; just collision-resistant
/// enough for local file names and element ids.
pub fn gen_id(prefix: &str) -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(now_ms());
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let v = h.finish();
    format!("{prefix}_{:08x}", (v ^ (v >> 32)) as u32)
}

/// A file-system friendly version of a user-visible name.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "untitled".into()
    } else {
        out.chars().take(60).collect()
    }
}

/// Characters Windows does not allow in file names are replaced; reserved names are prefixed.
pub fn safe_filename(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    s = s.trim().trim_end_matches('.').to_string();
    let upper = s.to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    let is_com_lpt = (upper.starts_with("COM") || upper.starts_with("LPT")) && upper.len() == 4;
    if s.is_empty() || reserved.contains(&upper.as_str()) || is_com_lpt {
        s = format!("_{s}");
    }
    s.chars().take(100).collect()
}

/// Civil date-time (UTC) from unix milliseconds: (year, month, day, hour, minute, second).
pub fn civil_from_ms(ms: u64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's days -> civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

/// `20260314-181530` – sortable timestamp for file names.
pub fn stamp(ms: u64) -> String {
    let (y, mo, d, h, mi, s) = civil_from_ms(ms);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = gen_id("lay");
        let b = gen_id("lay");
        assert!(a.starts_with("lay_"));
        assert_ne!(a, b);
    }

    #[test]
    fn slug_handles_unicode_and_symbols() {
        assert_eq!(slug("My Gaming Layout!!"), "my-gaming-layout");
        assert_eq!(slug("***"), "untitled");
    }

    #[test]
    fn filenames_are_windows_safe() {
        assert_eq!(safe_filename("a:b*c?.png"), "a_b_c_.png");
        assert_eq!(safe_filename("CON"), "_CON");
        assert_eq!(safe_filename("name. "), "name");
    }

    #[test]
    fn civil_date_is_correct() {
        // 2024-02-29T12:34:56Z
        let ms = 1_709_210_096_000u64;
        assert_eq!(civil_from_ms(ms), (2024, 2, 29, 12, 34, 56));
        assert_eq!(stamp(0), "19700101-000000");
    }
}
