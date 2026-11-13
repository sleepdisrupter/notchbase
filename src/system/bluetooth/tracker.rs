//! Turns each list of connected devices into connects and disconnects. Devices there at launch stay quiet, a device
//! must be gone a moment before it counts as gone, and a new one's battery is watched for a few seconds.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{Change, Device};

const SETTLE: Duration = Duration::from_millis(2500);
const BATTERY_WATCH: Duration = Duration::from_secs(12);

struct Seen {
    device: Device,
    since: Instant,
}

#[derive(Default)]
pub(super) struct Tracker {
    primed: bool,
    present: HashMap<String, Seen>,
    gone: HashMap<String, (Device, Instant)>,
}

impl Tracker {
    pub(super) fn observe(&mut self, connected: Vec<Device>, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let ids: Vec<String> = connected.iter().map(|device| device.id.clone()).collect();
        let watched_since = now.checked_sub(BATTERY_WATCH).unwrap_or(now);
        for device in connected {
            if let Some(seen) = self.present.get_mut(&device.id) {
                seen.device = Device { battery: seen.device.battery, ..device };
            } else if let Some((known, _)) = self.gone.remove(&device.id) {
                let device = Device { battery: known.battery, ..device };
                self.present.insert(device.id.clone(), Seen { device, since: watched_since });
            } else if self.primed {
                changes.push(Change::Connected(device.clone()));
                self.present.insert(device.id.clone(), Seen { device, since: now });
            } else {
                self.present.insert(device.id.clone(), Seen { device, since: watched_since });
            }
        }
        self.primed = true;
        let missing: Vec<String> = self.present.keys().filter(|id| !ids.contains(id)).cloned().collect();
        for id in missing {
            if let Some(seen) = self.present.remove(&id) {
                self.gone.insert(id, (seen.device, now));
            }
        }
        let settled: Vec<String> = self
            .gone
            .iter()
            .filter(|(_, (_, at))| now.duration_since(*at) >= SETTLE)
            .map(|(id, _)| id.clone())
            .collect();
        for id in settled {
            if let Some((device, _)) = self.gone.remove(&id) {
                changes.push(Change::Disconnected(device));
            }
        }
        changes
    }

    pub(super) fn wants_battery(&self, now: Instant) -> Vec<String> {
        self.present
            .values()
            .filter(|seen| seen.device.battery.is_none() && now.duration_since(seen.since) < BATTERY_WATCH)
            .map(|seen| seen.device.id.clone())
            .collect()
    }

    pub(super) fn battery(&mut self, id: &str, level: Option<u8>) -> Option<Device> {
        let seen = self.present.get_mut(id)?;
        if level.is_none() || seen.device.battery == level {
            return None;
        }
        seen.device.battery = level;
        Some(seen.device.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::bluetooth::DeviceKind;

    fn device(id: &str) -> Device {
        Device { id: id.into(), name: format!("Device {id}"), kind: DeviceKind::Headphones, battery: None }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn launch_devices_stay_quiet() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now() + Duration::from_secs(60);
        assert!(tracker.observe(vec![device("a")], t0).is_empty());
        assert!(tracker.wants_battery(t0).is_empty());
        let joined = tracker.observe(vec![device("a"), device("b")], t0 + ms(1500));
        assert_eq!(joined, vec![Change::Connected(device("b"))]);
        assert_eq!(tracker.wants_battery(t0 + ms(1500)), vec!["b".to_owned()]);
    }

    #[test]
    fn blip_is_not_a_disconnect() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(vec![device("a")], t0);
        assert!(tracker.observe(Vec::new(), t0 + ms(1500)).is_empty());
        assert!(tracker.observe(vec![device("a")], t0 + ms(3000)).is_empty());
        assert!(tracker.observe(Vec::new(), t0 + ms(4500)).is_empty());
        assert_eq!(tracker.observe(Vec::new(), t0 + ms(7500)), vec![Change::Disconnected(device("a"))]);
    }

    #[test]
    fn late_battery_updates_once() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(Vec::new(), t0);
        tracker.observe(vec![device("a")], t0 + ms(1000));
        assert_eq!(tracker.battery("a", None), None);
        let charged = tracker.battery("a", Some(80)).expect("an update");
        assert_eq!(charged.battery, Some(80));
        assert_eq!(tracker.battery("a", Some(80)), None);
        assert!(tracker.wants_battery(t0 + ms(2000)).is_empty());
        tracker.observe(vec![device("a")], t0 + ms(2500));
        assert_eq!(tracker.battery("a", Some(80)), None, "the battery survives a refresh");
    }
}
