//! The levels thread. While something plays and a notch dances to it, it captures the system output and publishes
//! six spectrum bands, and when capture can't open a made-up groove keeps the notch dancing.

use std::time::{Duration, Instant};

use notchbase_spectrum::{Analyzer, BANDS};

use super::groove::groove;
use super::imp::Capture;
use super::watcher::Shared;

const IDLE_CHECK: Duration = Duration::from_millis(80);
/// Capture stays open this long after the dance stops, so a gap between two dances doesn't reopen it.
const LINGER: Duration = Duration::from_secs(2);
const READ_EVERY: Duration = Duration::from_millis(12);
/// A capture that couldn't open is tried again after this, with the groove playing meanwhile.
const REOPEN_AFTER: Duration = Duration::from_secs(2);
/// A working capture that delivers nothing for this long drops the bands to zero.
const HUSH: Duration = Duration::from_millis(120);

enum Ear {
    Listening(Box<(Capture, Analyzer)>),
    Deaf { retry_at: Instant },
}

pub(super) fn listen_to_output(shared: &Shared) {
    let mut ear: Option<Ear> = None;
    let mut mono = Vec::new();
    let started = Instant::now();
    let mut heard_at = started;
    let mut wanted_until = started;
    while !shared.stopped() {
        let now = Instant::now();
        if shared.wanted() {
            wanted_until = now + LINGER;
        }
        // No capture while it isn't needed, which on a Mac keeps the System Audio Recording dot away.
        if !shared.playing() || now >= wanted_until {
            ear = None;
            shared.publish_levels([0.0; BANDS]);
            std::thread::sleep(IDLE_CHECK);
            continue;
        }
        match &mut ear {
            None => ear = Some(open(now)),
            Some(Ear::Deaf { retry_at }) => {
                shared.publish_levels(groove(now.duration_since(started).as_secs_f32()));
                if now >= *retry_at {
                    ear = None;
                }
            }
            Some(Ear::Listening(listening)) => {
                let (capture, analyzer) = &mut **listening;
                mono.clear();
                match capture.read(&mut mono) {
                    Ok(()) if mono.is_empty() => {
                        if now.duration_since(heard_at) > HUSH {
                            shared.publish_levels([0.0; BANDS]);
                        }
                    }
                    Ok(()) => {
                        heard_at = now;
                        if let Some(bands) = analyzer.push(&mono) {
                            shared.publish_levels(bands);
                        }
                    }
                    Err(_) => ear = None,
                }
            }
        }
        std::thread::sleep(READ_EVERY);
    }
}

fn open(now: Instant) -> Ear {
    match Capture::open() {
        Ok(capture) => {
            let rate = capture.rate();
            Ear::Listening(Box::new((capture, Analyzer::new(rate))))
        }
        Err(_) => Ear::Deaf { retry_at: now + REOPEN_AFTER },
    }
}
