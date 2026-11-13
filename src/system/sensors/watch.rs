//! The sensors thread: it reads the camera and microphone every second and applies a mute when one is asked for.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Sensors;

use super::imp::Probe;
use crate::poll::{Latest, Shift, Worker, lock};

const THREAD: &str = "notchbase-sensors";
const EVERY: Duration = Duration::from_secs(1);
const NAP: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Shared {
    latest: Latest<Sensors>,
    mute: Mutex<Option<bool>>,
}

pub struct SensorWatch {
    worker: Worker<Shared>,
}

impl SensorWatch {
    pub fn start() -> Self {
        Self { worker: Worker::start(THREAD, NAP, Shared::default(), watch) }
    }

    pub fn take(&self) -> Option<Sensors> {
        self.worker.state().latest.take()
    }

    pub fn mute(&self, muted: bool) {
        *lock(&self.worker.state().mute) = Some(muted);
    }
}

fn watch(shift: &Shift<Shared>) {
    let mut probe = Probe::open();
    loop {
        if let Some(muted) = lock(&shift.state().mute).take() {
            probe.set_muted(muted);
        }
        if !shift.on_duty() {
            return;
        }
        shift.state().latest.publish(probe.read());
        shift.nap_until(Instant::now() + EVERY, |shared| lock(&shared.mute).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATIENCE: Duration = Duration::from_secs(5);

    fn eventually(mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while !done() {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }

    #[cfg_attr(
        any(target_os = "macos", target_os = "windows"),
        ignore = "reads this computer's real camera and microphone"
    )]
    #[test]
    fn first_reading_handed_out_once() {
        let watch = SensorWatch::start();
        let mut first = None;
        assert!(eventually(|| {
            first = watch.take();
            first.is_some()
        }));
        assert_eq!(first, Some(Sensors::default()));
        assert_eq!(watch.take(), None, "an unchanged reading is not handed out again");
    }

    #[cfg_attr(any(target_os = "macos", target_os = "windows"), ignore = "mutes this computer's real microphone")]
    #[test]
    fn mute_is_picked_up_quickly() {
        let watch = SensorWatch::start();
        assert!(eventually(|| watch.take().is_some()));
        let asked = Instant::now();
        watch.mute(true);
        assert!(eventually(|| lock(&watch.worker.state().mute).is_none()));
        assert!(asked.elapsed() < EVERY, "picked up after {:?}", asked.elapsed());
    }
}
