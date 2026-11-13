//! The volume thread. It reads the output, applies key presses and publishes the level,
//! and keeps the key tap installed while the setting is on.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use crate::system::brightness::Display;

use super::detector::Detector;
use super::imp::{KeyTap, Output, STEPS};
use super::steps::{Steps, after, apply, brighten};
use super::tap::TapState;
use super::{BrightnessPress, Key, KeyPress, Level, MediaKey, Reading};

const WATCH_EVERY: Duration = Duration::from_millis(30);
const RESTING: Duration = Duration::from_millis(250);
const RETRY_KEYS: Duration = Duration::from_secs(3);
const BRIGHTNESS_STEPS: Steps = Steps { coarse: 16.0, fine: 64.0 };

#[derive(Default)]
struct Shared {
    enabled: AtomicBool,
    brightness_enabled: AtomicBool,
    stop: AtomicBool,
    seq: AtomicU64,
    latest: Mutex<Option<Level>>,
    brightness: Mutex<Option<f32>>,
    media: Mutex<Vec<MediaKey>>,
    tap: Arc<TapState>,
}

impl Shared {
    fn publish(&self, level: Level) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(level);
        self.seq.fetch_add(1, Ordering::Release);
    }
}

pub struct Volume {
    shared: Arc<Shared>,
    seen: u64,
}

impl Volume {
    pub fn start(enabled: bool) -> Self {
        let shared = Arc::new(Shared::default());
        shared.enabled.store(enabled, Ordering::Relaxed);
        let watching = shared.clone();
        let _ = std::thread::Builder::new().name("notchbase-volume".into()).spawn(move || watch(&watching));
        Self { shared, seen: 0 }
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
    }

    pub fn set_brightness_enabled(&self, on: bool) {
        self.shared.brightness_enabled.store(on, Ordering::Relaxed);
    }

    pub fn claim_media(&self, on: bool) {
        self.shared.tap.claim_media(on);
    }

    pub fn take_media_keys(&self) -> Vec<MediaKey> {
        std::mem::take(&mut *self.shared.media.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn take_change(&mut self) -> Option<Level> {
        let seq = self.shared.seq.load(Ordering::Acquire);
        if seq == self.seen {
            return None;
        }
        self.seen = seq;
        *self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn take_brightness(&self) -> Option<f32> {
        self.shared.brightness.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn watch(shared: &Shared) {
    let (sender, keys) = mpsc::channel();
    let mut output = Output::open();
    let mut display = Display::open();
    let mut detector = Detector::default();
    let mut tap: Option<KeyTap> = None;
    let mut retry_at = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let volume_on = shared.enabled.load(Ordering::Relaxed);
        let brightness_on = shared.brightness_enabled.load(Ordering::Relaxed) && display.is_adjustable();
        shared.tap.claim_volume(volume_on);
        shared.tap.claim_brightness(brightness_on);
        if !volume_on && !brightness_on {
            tap = None;
            detector.settle(None);
            keys.try_iter().for_each(drop);
            std::thread::sleep(RESTING);
            continue;
        }
        let now = Instant::now();
        if tap.is_none() && now >= retry_at {
            retry_at = now + RETRY_KEYS;
            tap = KeyTap::install(sender.clone(), shared.tap.clone()).ok();
        }
        let pressed: Vec<Key> = keys.recv_timeout(WATCH_EVERY).ok().into_iter().chain(keys.try_iter()).collect();
        let (mut volume, mut brightness) = (Vec::new(), Vec::new());
        for key in pressed {
            match key {
                Key::Volume(press) => volume.push(press),
                Key::Media(media) => shared.media.lock().unwrap_or_else(PoisonError::into_inner).push(media),
                Key::Brightness(press) => brightness.push(press),
            }
        }
        if let Some(level) = dim(&mut display, &brightness) {
            *shared.brightness.lock().unwrap_or_else(PoisonError::into_inner) = Some(level);
        }
        if !volume_on {
            detector.settle(None);
            continue;
        }
        if let Some(level) = press(&mut output, &mut detector, &volume) {
            shared.publish(level);
        }
        if let Some(level) = detector.observe(output.read()) {
            shared.publish(level);
        }
    }
}

fn press(output: &mut Output, detector: &mut Detector, presses: &[KeyPress]) -> Option<Level> {
    if presses.is_empty() {
        return None;
    }
    let reading = output.read()?;
    let mut level = reading.level;
    for &key in presses {
        let change = apply(key, level, STEPS);
        output.apply(change);
        level = after(level, change);
    }
    detector.settle(Some(Reading { device: reading.device, level }));
    Some(level)
}

fn dim(display: &mut Display, presses: &[BrightnessPress]) -> Option<f32> {
    if presses.is_empty() {
        return None;
    }
    let level = presses.iter().fold(display.read()?, |level, &key| brighten(key, level, BRIGHTNESS_STEPS));
    display.set(level);
    Some(level)
}
