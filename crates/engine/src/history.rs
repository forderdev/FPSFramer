//! A rolling one-sample-per-second history of every numeric sensor, so graphs have
//! something to draw the moment a window opens (not only from then on).

use fpsframer_core::sensors::{Reading, Readings};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};

pub const HISTORY_SECONDS: usize = 1800;

pub struct History {
    cap: usize,
    len: usize,
    end_ms: u64,
    series: HashMap<String, VecDeque<Option<f32>>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryData {
    /// Unix ms of the newest sample.
    pub end_ms: u64,
    pub interval_ms: u32,
    pub series: BTreeMap<String, Vec<Option<f32>>>,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self { cap, len: 0, end_ms: 0, series: HashMap::new() }
    }

    pub fn push_second(&mut self, t_ms: u64, readings: &Readings) {
        for (id, r) in readings {
            if let Reading::Num(v) = r {
                let len = self.len;
                self.series.entry(id.clone()).or_insert_with(|| VecDeque::from(vec![None; len]));
                let _ = v;
            }
        }
        for (id, s) in self.series.iter_mut() {
            let v = match readings.get(id) {
                Some(Reading::Num(v)) => Some(*v as f32),
                _ => None,
            };
            s.push_back(v);
        }
        self.len += 1;
        self.end_ms = t_ms;
        if self.len > self.cap {
            for s in self.series.values_mut() {
                s.pop_front();
            }
            self.len -= 1;
        }
        // Forget sensors that have been absent for the whole window.
        if self.len == self.cap {
            self.series.retain(|_, s| s.iter().any(Option::is_some));
        }
    }

    pub fn get(&self, ids: &[String], seconds: usize) -> HistoryData {
        let n = seconds.min(self.len);
        let series = ids
            .iter()
            .filter_map(|id| {
                let s = self.series.get(id)?;
                Some((id.clone(), s.iter().skip(s.len() - n).cloned().collect()))
            })
            .collect();
        HistoryData { end_ms: self.end_ms, interval_ms: 1000, series }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpsframer_core::sensors::{ReadingsExt, Unit};

    fn r(pairs: &[(&str, f64)]) -> Readings {
        let mut m = Readings::new();
        for (k, v) in pairs {
            m.put(k, *v, Unit::Count);
        }
        m
    }

    #[test]
    fn late_sensors_are_padded_with_gaps_not_zeros() {
        let mut h = History::new(100);
        h.push_second(1000, &r(&[("a", 1.0)]));
        h.push_second(2000, &r(&[("a", 2.0)]));
        h.push_second(3000, &r(&[("a", 3.0), ("b", 9.0)]));
        let d = h.get(&["a".into(), "b".into(), "zzz".into()], 10);
        assert_eq!(d.series["a"], vec![Some(1.0), Some(2.0), Some(3.0)]);
        assert_eq!(d.series["b"], vec![None, None, Some(9.0)]);
        assert!(!d.series.contains_key("zzz"));
        assert_eq!(d.end_ms, 3000);
    }

    #[test]
    fn window_is_capped_and_requests_are_trimmed() {
        let mut h = History::new(5);
        for i in 0..20 {
            h.push_second(i * 1000, &r(&[("a", i as f64)]));
        }
        let d = h.get(&["a".into()], 100);
        assert_eq!(d.series["a"].len(), 5);
        assert_eq!(d.series["a"].last().unwrap(), &Some(19.0));
        assert_eq!(h.get(&["a".into()], 2).series["a"], vec![Some(18.0), Some(19.0)]);
    }

    #[test]
    fn text_readings_are_ignored() {
        let mut h = History::new(5);
        let mut m = Readings::new();
        m.insert("name".into(), Reading::Text("x".into()));
        h.push_second(1, &m);
        assert!(h.get(&["name".into()], 5).series.is_empty());
    }
}
