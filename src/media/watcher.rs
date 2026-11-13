//! The media service the app holds: one thread follows the player and carries controls to it, another hears the
//! output and works out the levels.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use notchbase_spectrum::Bands;

use super::follower::Follower;
use super::imp::Source;
use super::listener::listen_to_output;
use super::track::{Control, NowPlaying};

// Just after a control, poll faster so the UI confirms a skip or a pause quickly.
const QUICK_POLL: Duration = Duration::from_millis(120);
const QUICK_FOR: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub(super) struct Shared {
    now: Mutex<Option<NowPlaying>>,
    levels: Mutex<Bands>,
    playing: AtomicBool,
    wanted: AtomicBool,
    present: AtomicBool,
    stop: AtomicBool,
}

impl Shared {
    pub(super) fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub(super) fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub(super) fn wanted(&self) -> bool {
        self.wanted.load(Ordering::Relaxed)
    }

    pub(super) fn publish_levels(&self, bands: Bands) {
        *self.levels.lock().unwrap_or_else(PoisonError::into_inner) = bands;
    }

    fn show(&self, now: Option<NowPlaying>) {
        self.playing.store(now.as_ref().is_some_and(|now| now.playing), Ordering::Relaxed);
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) = now;
    }
}

pub struct Media {
    shared: Arc<Shared>,
    controls: Sender<Control>,
}

impl Media {
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        let (controls, inbox) = mpsc::channel();
        let follow = shared.clone();
        let _ =
            std::thread::Builder::new().name("notchbase-media".into()).spawn(move || follow_player(&follow, &inbox));
        let listen = shared.clone();
        let _ = std::thread::Builder::new().name("notchbase-levels".into()).spawn(move || listen_to_output(&listen));
        Self { shared, controls }
    }

    pub fn control(&self, control: Control) {
        let _ = self.controls.send(control);
    }

    pub fn now_playing(&self) -> Option<NowPlaying> {
        self.shared.now.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Whether any player session exists, which decides whether to claim the media keys.
    pub fn has_player(&self) -> bool {
        self.shared.present.load(Ordering::Relaxed)
    }

    pub fn levels(&self) -> Bands {
        *self.shared.levels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a notch is dancing, which tells the levels thread to capture.
    pub fn listen(&self, wanted: bool) {
        self.shared.wanted.store(wanted, Ordering::Relaxed);
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn follow_player(shared: &Shared, inbox: &Receiver<Control>) {
    let mut source = Source::new();
    let mut follower = Follower::default();
    let mut quick_until: Option<Instant> = None;
    while !shared.stopped() {
        let track = source.poll();
        shared.present.store(track.is_some(), Ordering::Relaxed);
        shared.show(follower.update(track, Instant::now()));
        if follower.wants_artwork() {
            shared.show(follower.attach_artwork(source.artwork(), Instant::now()));
        }
        let quick = quick_until.is_some_and(|until| Instant::now() < until);
        let poll = if quick { QUICK_POLL } else { crate::config::current().media.poll() };
        match inbox.recv_timeout(poll) {
            Ok(control) => {
                for control in std::iter::once(control).chain(inbox.try_iter()) {
                    source.control(control);
                }
                quick_until = Some(Instant::now() + QUICK_FOR);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
