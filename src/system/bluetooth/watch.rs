//! The Bluetooth thread: it polls the connected devices and queues the changes for the app.

use std::time::{Duration, Instant};

use super::imp::Scanner;
use super::tracker::Tracker;
use super::{Change, Device};
use crate::poll::{Outbox, Shift, Worker};

const THREAD: &str = "notchbase-bluetooth";
const NAP: Duration = Duration::from_millis(100);

pub struct Bluetooth {
    worker: Worker<Outbox<Change>>,
}

impl Bluetooth {
    pub fn start() -> Self {
        Self { worker: Worker::start(THREAD, NAP, Outbox::default(), watch) }
    }

    pub fn take_changes(&self) -> Vec<Change> {
        self.worker.state().take_all()
    }
}

fn watch(shift: &Shift<Outbox<Change>>) {
    let mut scanner = Scanner::new();
    let mut tracker = Tracker::default();
    while shift.on_duty() {
        if let Some(connected) = scanner.connected() {
            let changes = changes(&mut tracker, connected, Instant::now(), |id| scanner.battery(id));
            if !changes.is_empty() {
                shift.state().extend(changes);
            }
        }
        shift.nap_until(Instant::now() + crate::config::current().bluetooth.poll(), |_| false);
    }
}

fn changes(
    tracker: &mut Tracker,
    connected: Vec<Device>,
    now: Instant,
    mut battery: impl FnMut(&str) -> Option<u8>,
) -> Vec<Change> {
    let mut changes = tracker.observe(connected, now);
    for change in &mut changes {
        if let Change::Connected(device) = change {
            device.battery = battery(&device.id);
            tracker.battery(&device.id, device.battery);
        }
    }
    for id in tracker.wants_battery(now) {
        let level = battery(&id);
        changes.extend(tracker.battery(&id, level).map(Change::Updated));
    }
    changes
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::system::bluetooth::DeviceKind;

    const LATER: Duration = Duration::from_secs(1);

    fn device(id: &str) -> Device {
        Device { id: id.into(), name: format!("Device {id}"), kind: DeviceKind::Headphones, battery: None }
    }

    fn charged(id: &str, level: u8) -> Device {
        Device { battery: Some(level), ..device(id) }
    }

    fn primed(now: Instant) -> Tracker {
        let mut tracker = Tracker::default();
        changes(&mut tracker, Vec::new(), now, |_| None);
        tracker
    }

    #[test]
    fn connects_with_its_battery() {
        let now = Instant::now();
        let mut tracker = primed(now);
        let asked = RefCell::new(Vec::new());
        let found = changes(&mut tracker, vec![device("a")], now + LATER, |id| {
            asked.borrow_mut().push(id.to_owned());
            Some(60)
        });
        assert_eq!(found, [Change::Connected(charged("a", 60))]);
        assert_eq!(asked.into_inner(), ["a"], "the battery is read once and remembered");
    }

    #[test]
    fn unread_battery_still_connects() {
        let now = Instant::now();
        let mut tracker = primed(now);
        let found = changes(&mut tracker, vec![device("a")], now + LATER, |_| None);
        assert_eq!(found, [Change::Connected(device("a"))]);
    }

    #[test]
    fn late_battery_is_an_update() {
        let now = Instant::now();
        let mut tracker = primed(now);
        changes(&mut tracker, vec![device("a")], now + LATER, |_| None);
        let found = changes(&mut tracker, vec![device("a")], now + LATER * 2, |_| Some(35));
        assert_eq!(found, [Change::Updated(charged("a", 35))]);
        assert!(changes(&mut tracker, vec![device("a")], now + LATER * 3, |_| Some(35)).is_empty());
    }

    #[test]
    fn launch_devices_not_announced() {
        let now = Instant::now() + Duration::from_secs(60);
        let mut tracker = Tracker::default();
        let found = changes(&mut tracker, vec![device("a")], now, |_| Some(90));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn nothing_connected_asks_nothing() {
        let now = Instant::now();
        let mut tracker = primed(now);
        let found = changes(&mut tracker, Vec::new(), now + LATER, |id| panic!("asked about {id}"));
        assert!(found.is_empty());
    }

    #[cfg_attr(
        any(target_os = "macos", target_os = "windows"),
        ignore = "reads this computer's real Bluetooth devices"
    )]
    #[test]
    fn no_bluetooth_no_changes() {
        let bluetooth = Bluetooth::start();
        std::thread::sleep(Duration::from_millis(5));
        assert!(bluetooth.take_changes().is_empty());
    }
}
