//! Alert evaluation with anti-noise behaviour.
//!
//! * **Sustain** – a condition must hold for `for_seconds` before an alert fires,
//!   so a one-second temperature blip does not raise a warning.
//! * **Cooldown** – after a notification, the same rule will not notify again for
//!   `cooldown_seconds` (the alert still shows as active while it is true).
//! * **Hysteresis + clear dwell** – an alert clears only after the value has moved
//!   clearly back past the threshold and stayed there for two seconds, so a value
//!   hovering around the limit does not flap.

use crate::model::{AlertRule, Comparator, Severity};
use crate::sensors::{Readings, ReadingsExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CLEAR_DWELL_MS: u64 = 2_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAlert {
    pub rule_id: String,
    pub name: String,
    pub severity: Severity,
    pub sensor_id: String,
    pub value: f64,
    pub threshold: f64,
    pub message: String,
    pub since_ms: u64,
    /// Whether this alert should appear in the overlay.
    pub overlay: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlertEvent {
    Fired { alert: ActiveAlert, toast: bool, sound: bool, suppressed: bool },
    Cleared { rule_id: String, name: String, duration_ms: u64, peak: f64 },
}

pub struct AlertContext<'a> {
    pub game_active: bool,
    pub profile_id: Option<&'a str>,
}

/// Formatting callbacks so messages use friendly names and units.
pub struct AlertFormat<'a> {
    pub sensor_name: &'a dyn Fn(&str) -> String,
    pub value: &'a dyn Fn(&str, f64) -> String,
}

#[derive(Debug, Clone)]
enum Phase {
    Idle,
    Pending { since: u64 },
    Active { since: u64, clear_since: Option<u64>, peak: f64 },
}

#[derive(Debug, Clone)]
struct RuleState {
    phase: Phase,
    last_notified: Option<u64>,
    last_value: f64,
    message: String,
}

#[derive(Default)]
pub struct AlertEngine {
    states: HashMap<String, RuleState>,
    meta: HashMap<String, ActiveMeta>,
}

#[derive(Clone)]
struct ActiveMeta {
    name: String,
    severity: Severity,
    sensor_id: String,
    threshold: f64,
    overlay: bool,
}

fn render_message(rule: &AlertRule, v: f64, fmt: &AlertFormat) -> String {
    let sensor = (fmt.sensor_name)(&rule.sensor_id);
    let value = (fmt.value)(&rule.sensor_id, v);
    let thr = (fmt.value)(&rule.sensor_id, rule.threshold);
    if rule.message.trim().is_empty() {
        let word = match rule.comparator {
            Comparator::Above => "high",
            Comparator::Below => "low",
        };
        format!("{sensor} is {word}: {value} (alert level {thr})")
    } else {
        rule.message.replace("{sensor}", &sensor).replace("{value}", &value).replace("{threshold}", &thr)
    }
}

impl AlertEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.states.clear();
        self.meta.clear();
    }

    /// Evaluate all rules against the latest readings.
    pub fn evaluate(
        &mut self,
        now_ms: u64,
        rules: &[AlertRule],
        readings: &Readings,
        ctx: &AlertContext,
        fmt: &AlertFormat,
    ) -> Vec<AlertEvent> {
        let mut events = Vec::new();
        // Forget state of deleted rules.
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        self.states.retain(|k, _| ids.contains(&k.as_str()));
        self.meta.retain(|k, _| ids.contains(&k.as_str()));

        for rule in rules {
            let in_profile = rule.profile_ids.is_empty()
                || ctx.profile_id.is_some_and(|p| rule.profile_ids.iter().any(|x| x == p));
            let in_scope = rule.enabled && in_profile && (!rule.game_only || ctx.game_active);
            let value = readings.num(&rule.sensor_id);
            let st = self.states.entry(rule.id.clone()).or_insert(RuleState {
                phase: Phase::Idle,
                last_notified: None,
                last_value: 0.0,
                message: String::new(),
            });
            self.meta.insert(
                rule.id.clone(),
                ActiveMeta {
                    name: rule.name.clone(),
                    severity: rule.severity,
                    sensor_id: rule.sensor_id.clone(),
                    threshold: rule.threshold,
                    overlay: rule.notify.overlay,
                },
            );

            let v = match (in_scope, value) {
                (true, Some(v)) => v,
                _ => {
                    // Out of scope or sensor vanished: end whatever was going on.
                    if let Phase::Active { since, peak, .. } = st.phase {
                        events.push(AlertEvent::Cleared {
                            rule_id: rule.id.clone(),
                            name: rule.name.clone(),
                            duration_ms: now_ms.saturating_sub(since),
                            peak,
                        });
                    }
                    st.phase = Phase::Idle;
                    continue;
                }
            };

            let breach = match rule.comparator {
                Comparator::Above => v >= rule.threshold,
                Comparator::Below => v <= rule.threshold,
            };
            let clearly_ok = match rule.comparator {
                Comparator::Above => v < rule.threshold - rule.hysteresis,
                Comparator::Below => v > rule.threshold + rule.hysteresis,
            };
            st.last_value = v;

            let mut promote = false;
            match &mut st.phase {
                Phase::Idle => {
                    if breach {
                        if rule.for_seconds == 0 {
                            promote = true;
                        } else {
                            st.phase = Phase::Pending { since: now_ms };
                        }
                    }
                }
                Phase::Pending { since } => {
                    if !breach {
                        st.phase = Phase::Idle;
                    } else if now_ms.saturating_sub(*since) >= rule.for_seconds as u64 * 1000 {
                        promote = true;
                    }
                }
                Phase::Active { since, clear_since, peak } => {
                    *peak = match rule.comparator {
                        Comparator::Above => peak.max(v),
                        Comparator::Below => peak.min(v),
                    };
                    st.message = render_message(rule, v, fmt);
                    if clearly_ok {
                        let cs = *clear_since.get_or_insert(now_ms);
                        if now_ms.saturating_sub(cs) >= CLEAR_DWELL_MS {
                            events.push(AlertEvent::Cleared {
                                rule_id: rule.id.clone(),
                                name: rule.name.clone(),
                                duration_ms: now_ms.saturating_sub(*since),
                                peak: *peak,
                            });
                            st.phase = Phase::Idle;
                        }
                    } else {
                        *clear_since = None;
                    }
                }
            }

            if promote {
                st.phase = Phase::Active { since: now_ms, clear_since: None, peak: v };
                st.message = render_message(rule, v, fmt);
                let cooled = st
                    .last_notified
                    .is_none_or(|t| now_ms.saturating_sub(t) >= rule.cooldown_seconds as u64 * 1000);
                if cooled {
                    st.last_notified = Some(now_ms);
                }
                events.push(AlertEvent::Fired {
                    alert: ActiveAlert {
                        rule_id: rule.id.clone(),
                        name: rule.name.clone(),
                        severity: rule.severity,
                        sensor_id: rule.sensor_id.clone(),
                        value: v,
                        threshold: rule.threshold,
                        message: st.message.clone(),
                        since_ms: now_ms,
                        overlay: rule.notify.overlay,
                    },
                    toast: cooled && rule.notify.toast,
                    sound: cooled && rule.notify.sound,
                    suppressed: !cooled,
                });
            }
        }
        events
    }

    /// Alerts that are currently active, most severe first.
    pub fn active(&self) -> Vec<ActiveAlert> {
        let mut out: Vec<ActiveAlert> = self
            .states
            .iter()
            .filter_map(|(id, st)| match st.phase {
                Phase::Active { since, .. } => {
                    let m = self.meta.get(id)?;
                    Some(ActiveAlert {
                        rule_id: id.clone(),
                        name: m.name.clone(),
                        severity: m.severity,
                        sensor_id: m.sensor_id.clone(),
                        value: st.last_value,
                        threshold: m.threshold,
                        message: st.message.clone(),
                        since_ms: since,
                        overlay: m.overlay,
                    })
                }
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.since_ms.cmp(&b.since_ms)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::Unit;

    type NameFmt = Box<dyn Fn(&str) -> String>;
    type ValueFmt = Box<dyn Fn(&str, f64) -> String>;

    fn fmt() -> (NameFmt, ValueFmt) {
        (Box::new(|id| format!("<{id}>")), Box::new(|_, v| format!("{v:.0}")))
    }

    fn rule(threshold: f64, for_s: u32, cooldown: u32) -> AlertRule {
        AlertRule {
            id: "r1".into(),
            name: "GPU hot".into(),
            sensor_id: "gpu.temp".into(),
            threshold,
            for_seconds: for_s,
            cooldown_seconds: cooldown,
            hysteresis: 3.0,
            ..Default::default()
        }
    }

    fn run(e: &mut AlertEngine, t: u64, r: &AlertRule, v: Option<f64>, game: bool) -> Vec<AlertEvent> {
        let mut rd = Readings::new();
        if let Some(v) = v {
            rd.put("gpu.temp", v, Unit::Celsius);
        }
        let (n, f) = fmt();
        e.evaluate(
            t,
            std::slice::from_ref(r),
            &rd,
            &AlertContext { game_active: game, profile_id: None },
            &AlertFormat { sensor_name: &*n, value: &*f },
        )
    }

    #[test]
    fn short_spike_does_not_fire() {
        let mut e = AlertEngine::new();
        let r = rule(90.0, 5, 120);
        assert!(run(&mut e, 0, &r, Some(95.0), false).is_empty());
        assert!(run(&mut e, 2_000, &r, Some(95.0), false).is_empty());
        assert!(run(&mut e, 3_000, &r, Some(70.0), false).is_empty()); // dropped back: pending cancelled
        assert!(run(&mut e, 9_000, &r, Some(95.0), false).is_empty());
        assert!(e.active().is_empty());
    }

    #[test]
    fn sustained_condition_fires_once_then_clears_with_hysteresis() {
        let mut e = AlertEngine::new();
        let r = rule(90.0, 5, 120);
        run(&mut e, 0, &r, Some(95.0), false);
        let ev = run(&mut e, 5_000, &r, Some(96.0), false);
        assert!(matches!(&ev[0], AlertEvent::Fired { toast: true, suppressed: false, .. }));
        assert_eq!(e.active().len(), 1);
        // Still hot: no repeated events.
        assert!(run(&mut e, 6_000, &r, Some(97.0), false).is_empty());
        // Inside the hysteresis band (88 is below 90 but not below 87): stays active.
        assert!(run(&mut e, 7_000, &r, Some(88.0), false).is_empty());
        assert_eq!(e.active().len(), 1);
        // Clearly cool, but must dwell 2 s.
        assert!(run(&mut e, 8_000, &r, Some(80.0), false).is_empty());
        let ev = run(&mut e, 10_100, &r, Some(80.0), false);
        match &ev[0] {
            AlertEvent::Cleared { peak, duration_ms, .. } => {
                assert_eq!(*peak, 97.0);
                assert!(*duration_ms >= 5_000);
            }
            other => panic!("expected clear, got {other:?}"),
        }
        assert!(e.active().is_empty());
    }

    #[test]
    fn cooldown_suppresses_repeat_notifications_but_alert_still_shows() {
        let mut e = AlertEngine::new();
        let r = rule(90.0, 1, 120);
        run(&mut e, 0, &r, Some(95.0), false);
        let first = run(&mut e, 1_000, &r, Some(95.0), false);
        assert!(matches!(&first[0], AlertEvent::Fired { toast: true, .. }));
        // Cool down and clear...
        run(&mut e, 2_000, &r, Some(60.0), false);
        run(&mut e, 4_100, &r, Some(60.0), false);
        assert!(e.active().is_empty());
        // ...then heat up again 10 s later, well inside the 120 s cooldown.
        run(&mut e, 10_000, &r, Some(95.0), false);
        let again = run(&mut e, 11_000, &r, Some(95.0), false);
        match &again[0] {
            AlertEvent::Fired { toast, sound, suppressed, .. } => {
                assert!(!toast && !sound, "must not notify again during cooldown");
                assert!(*suppressed);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(e.active().len(), 1, "but it is still visible as active");
    }

    #[test]
    fn game_only_rules_wait_for_a_game() {
        let mut e = AlertEngine::new();
        let mut r = rule(90.0, 0, 0);
        r.game_only = true;
        assert!(run(&mut e, 0, &r, Some(99.0), false).is_empty());
        assert!(!run(&mut e, 1_000, &r, Some(99.0), true).is_empty());
    }

    #[test]
    fn vanished_sensor_ends_the_alert_instead_of_sticking() {
        let mut e = AlertEngine::new();
        let r = rule(90.0, 0, 0);
        run(&mut e, 0, &r, Some(99.0), false);
        assert_eq!(e.active().len(), 1);
        let ev = run(&mut e, 1_000, &r, None, false);
        assert!(matches!(ev[0], AlertEvent::Cleared { .. }));
        assert!(e.active().is_empty());
    }

    #[test]
    fn below_comparator_for_fps_targets() {
        let mut e = AlertEngine::new();
        let r = AlertRule {
            id: "fps".into(),
            name: "Low FPS".into(),
            sensor_id: "fps".into(),
            comparator: Comparator::Below,
            threshold: 60.0,
            for_seconds: 0,
            hysteresis: 5.0,
            ..Default::default()
        };
        let (n, f) = fmt();
        let mut rd = Readings::new();
        rd.put("fps", 45.0, Unit::Fps);
        let ev = e.evaluate(
            0,
            std::slice::from_ref(&r),
            &rd,
            &AlertContext { game_active: true, profile_id: None },
            &AlertFormat { sensor_name: &*n, value: &*f },
        );
        match &ev[0] {
            AlertEvent::Fired { alert, .. } => assert!(alert.message.contains("low"), "{}", alert.message),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn disabled_rule_never_fires_and_profile_scope_is_respected() {
        let mut e = AlertEngine::new();
        let mut r = rule(50.0, 0, 0);
        r.enabled = false;
        assert!(run(&mut e, 0, &r, Some(99.0), true).is_empty());
        r.enabled = true;
        r.profile_ids = vec!["p1".into()];
        assert!(run(&mut e, 1_000, &r, Some(99.0), true).is_empty(), "wrong profile");
    }

    #[test]
    fn custom_message_placeholders() {
        let mut r = rule(90.0, 0, 0);
        r.message = "{sensor} reached {value}, limit {threshold}".into();
        let mut e = AlertEngine::new();
        let ev = run(&mut e, 0, &r, Some(95.0), false);
        match &ev[0] {
            AlertEvent::Fired { alert, .. } => assert_eq!(alert.message, "<gpu.temp> reached 95, limit 90"),
            other => panic!("{other:?}"),
        }
    }
}
