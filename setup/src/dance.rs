//! The installer's bottom edge dances to whatever is playing. Only Windows can listen in, so
//! elsewhere the edge stays still.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use notchbase_paint::liquid::Liquid;
use notchbase_paint::wave::Wave;
use notchbase_spectrum::{BANDS, Bands};

use crate::install::Wake;

pub const DEPTH: f32 = 6.5;
pub const ROOM: f32 = 9.0; // the strip below the card the wave can dip into

pub struct Dance {
    heard: Arc<Mutex<Bands>>,
    stopped: Arc<AtomicBool>,
    liquid: Liquid,
}

impl Dance {
    pub fn start(wake: Wake) -> Self {
        let heard = Arc::new(Mutex::new([0.0; BANDS]));
        let stopped = Arc::new(AtomicBool::new(false));
        ear::listen(Arc::clone(&heard), Arc::clone(&stopped), wake);
        Self { heard, stopped, liquid: Liquid::default() }
    }

    pub fn still() -> Self {
        let stopped = Arc::new(AtomicBool::new(true));
        Self { heard: Arc::new(Mutex::new([0.0; BANDS])), stopped, liquid: Liquid::default() }
    }

    pub fn wave(&mut self, dt: f32) -> Option<Wave> {
        self.liquid.set(*self.heard.lock().unwrap_or_else(PoisonError::into_inner));
        self.liquid.step(dt);
        self.liquid.is_moving().then(|| self.liquid.wave(DEPTH))
    }
}

impl Drop for Dance {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

#[cfg(windows)]
mod ear {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::{Duration, Instant};

    use notchbase_spectrum::{Analyzer, BANDS, Bands};
    use notchbase_wasapi::{Apartment, Loopback};

    use crate::install::Wake;

    const READ_EVERY: Duration = Duration::from_millis(12);
    const REOPEN_AFTER: Duration = Duration::from_secs(2);
    const HUSH: Duration = Duration::from_millis(120); // silence this long drops the bands to zero

    pub fn listen(heard: Arc<Mutex<Bands>>, stopped: Arc<AtomicBool>, wake: Wake) {
        std::thread::spawn(move || {
            let _apartment = Apartment::join();
            let publish = |bands: Bands| {
                let mut shared = heard.lock().unwrap_or_else(PoisonError::into_inner);
                if *shared != bands {
                    *shared = bands;
                    wake();
                }
            };
            let mut mono = Vec::new();
            while !stopped.load(Ordering::SeqCst) {
                let Ok(mut loopback) = Loopback::open() else {
                    std::thread::sleep(REOPEN_AFTER);
                    continue;
                };
                let mut analyzer = Analyzer::new(loopback.rate());
                let mut heard_at = Instant::now();
                while !stopped.load(Ordering::SeqCst) {
                    mono.clear();
                    if loopback.read(&mut mono).is_err() {
                        break;
                    }
                    let now = Instant::now();
                    if !mono.is_empty() {
                        heard_at = now;
                        if let Some(bands) = analyzer.push(&mono) {
                            publish(bands);
                        }
                    } else if now.duration_since(heard_at) > HUSH {
                        publish([0.0; BANDS]);
                    }
                    std::thread::sleep(READ_EVERY);
                }
                publish([0.0; BANDS]);
            }
        });
    }
}

#[cfg(not(windows))]
mod ear {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use notchbase_spectrum::Bands;

    use crate::install::Wake;

    pub fn listen(_heard: Arc<Mutex<Bands>>, _stopped: Arc<AtomicBool>, _wake: Wake) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    #[test]
    fn still_dance_never_waves() {
        let mut dance = Dance::still();
        assert!(dance.stopped.load(Ordering::SeqCst));
        assert!((0..10).all(|_| dance.wave(FRAME).is_none()));
    }

    #[test]
    fn music_waves_until_quiet() {
        let mut dance = Dance::still();
        *dance.heard.lock().unwrap() = [1.0; BANDS];
        assert!(dance.wave(FRAME).is_some_and(|wave| wave.deepest() > 0.0));
        *dance.heard.lock().unwrap() = [0.0; BANDS];
        assert_eq!((0..600).map(|_| dance.wave(FRAME)).last().flatten(), None);
    }

    #[test]
    fn drop_stops_the_ear() {
        let stopped = Arc::new(AtomicBool::new(false));
        let dance = Dance {
            heard: Arc::new(Mutex::new([0.0; BANDS])),
            stopped: Arc::clone(&stopped),
            liquid: Liquid::default(),
        };
        drop(dance);
        assert!(stopped.load(Ordering::SeqCst));
    }
}
