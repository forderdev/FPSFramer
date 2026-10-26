//! Messages the engine publishes. The Tauri shell and the dev server both subscribe.

use crate::types::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use fpsframer_core::alerts::ActiveAlert;
use fpsframer_core::model::{AlertRule, Profile, Settings};
use fpsframer_core::sensors::SensorDesc;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Event {
    /// Live readings (several per second).
    Snapshot(Snapshot),
    /// The set of sensors, or their availability, changed.
    Catalog(Vec<SensorDesc>),
    Settings(Settings),
    Layouts(Vec<fpsframer_core::layouts::Layout>),
    Profiles(Vec<Profile>),
    Rules(Vec<AlertRule>),
    /// Something about the session archive or screenshots changed; refetch lists.
    Sessions,
    Screenshots,
    /// Declarative description of every overlay window that should exist right now.
    Overlays(Vec<OverlaySpec>),
    /// Native Windows notification.
    Notify(Notification),
    /// In-app toast message.
    Toast(UiToast),
    /// An alert started or ended (for the activity list).
    AlertLog(AlertLogItem),
    SessionFinished { id: String, name: String },
    Hotkeys(Vec<HotkeyBinding>),
    Tray(TrayState),
    /// Provider/capture status changed (diagnostics).
    Status(StatusInfo),
    /// The app should exit (for example after relaunching as administrator).
    Quit,
}

#[derive(Default)]
pub struct EventBus {
    subs: Mutex<Vec<Sender<Arc<Event>>>>,
}

impl EventBus {
    pub fn subscribe(&self) -> Receiver<Arc<Event>> {
        let (tx, rx) = unbounded();
        self.subs.lock().push(tx);
        rx
    }

    pub fn emit(&self, e: Event) {
        let e = Arc::new(e);
        self.subs.lock().retain(|s| s.send(e.clone()).is_ok());
    }

    pub fn subscriber_count(&self) -> usize {
        self.subs.lock().len()
    }
}

#[allow(dead_code)]
fn _assert_serializable(_: &ActiveAlert) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_subscribers_are_pruned() {
        let bus = EventBus::default();
        let a = bus.subscribe();
        let b = bus.subscribe();
        drop(b);
        bus.emit(Event::Sessions);
        assert_eq!(bus.subscriber_count(), 1);
        assert!(matches!(&*a.try_recv().unwrap(), Event::Sessions));
    }

    #[test]
    fn events_serialize_with_type_tag() {
        let j = serde_json::to_value(Event::Sessions).unwrap();
        assert_eq!(j["type"], "sessions");
        let j = serde_json::to_value(Event::SessionFinished { id: "x".into(), name: "n".into() }).unwrap();
        assert_eq!(j["type"], "sessionFinished");
        assert_eq!(j["data"]["id"], "x");
    }
}
