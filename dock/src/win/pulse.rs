//! The music pulse on Windows: while a music player plays (Spotify, Apple Music, Media Player and the like, as the
//! media controls report it), its icon swells gently on the beat. The beat comes from what the speakers play,
//! through a WASAPI loopback, and only while the player plays. When it can't be heard, the icon breathes instead.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ::windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

use crate::pulse::{Envelope, breath, rms};

/// Players followed, by words in the name the media controls give them or in their program's name.
const PLAYERS: [&str; 9] =
    ["spotify", "applemusic", "itunes", "zunemusic", "tidal", "deezer", "amazonmusic", "musicbee", "foobar2000"];
const CHECK: Duration = Duration::from_millis(400);
/// The loudness is read in slices this long.
const SLICE: Duration = Duration::from_millis(16);

#[derive(Default)]
struct Shared {
    enabled: AtomicBool,
    /// The player playing, or 0.
    playing: AtomicI32,
    level: AtomicU32,
    heard: AtomicBool,
}

pub struct Pulse {
    shared: Arc<Shared>,
}

impl Pulse {
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        let watch = Arc::clone(&shared);
        let _ = std::thread::Builder::new().name("music pulse".into()).spawn(move || run(&watch));
        Self { shared }
    }

    /// The player to pulse and its beat now, or nothing. `enabled` off stops listening.
    pub fn step(&mut self, enabled: bool, time: f32) -> Option<(i32, f32)> {
        self.shared.enabled.store(enabled, Ordering::Relaxed);
        if !enabled {
            return None;
        }
        let pid = self.shared.playing.load(Ordering::Relaxed);
        if pid <= 0 {
            return None;
        }
        if !self.shared.heard.load(Ordering::Relaxed) {
            return Some((pid, breath(time)));
        }
        Some((pid, f32::from_bits(self.shared.level.load(Ordering::Relaxed))))
    }
}

fn run(shared: &Shared) {
    let _apartment = notchbase_wasapi::Apartment::join();
    let mut manager: Option<Manager> = None;
    let mut loopback: Option<notchbase_wasapi::Loopback> = None;
    let mut envelope = Envelope::default();
    let mut samples = Vec::new();
    let mut checked: Option<Instant> = None;
    let mut last = Instant::now();
    loop {
        if !shared.enabled.load(Ordering::Relaxed) {
            loopback = None;
            shared.playing.store(0, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        if checked.is_none_or(|at| at.elapsed() >= CHECK) {
            checked = Some(Instant::now());
            if manager.is_none() {
                manager = Manager::RequestAsync().and_then(|op| op.join()).ok();
            }
            let pid = manager.as_ref().and_then(playing_player).unwrap_or(0);
            shared.playing.store(pid, Ordering::Relaxed);
            if pid == 0 {
                loopback = None;
                shared.heard.store(false, Ordering::Relaxed);
            } else if loopback.is_none() {
                loopback = notchbase_wasapi::Loopback::open().ok();
                shared.heard.store(loopback.is_some(), Ordering::Relaxed);
            }
        }
        let Some(audio) = loopback.as_mut() else {
            std::thread::sleep(CHECK);
            continue;
        };
        samples.clear();
        if audio.read(&mut samples).is_err() {
            loopback = None;
            shared.heard.store(false, Ordering::Relaxed);
            continue;
        }
        let dt = last.elapsed().as_secs_f32();
        last = Instant::now();
        let level = envelope.feed(rms(&samples), dt);
        shared.level.store(level.to_bits(), Ordering::Relaxed);
        std::thread::sleep(SLICE);
    }
}

/// The running player whose media session is playing, by its pid.
fn playing_player(manager: &Manager) -> Option<i32> {
    let sessions = manager.GetSessions().ok()?;
    for session in sessions {
        let playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .is_ok_and(|status| status == Status::Playing);
        if !playing {
            continue;
        }
        let app = session.SourceAppUserModelId().map(|id| id.to_string_lossy().to_lowercase()).unwrap_or_default();
        let squashed: String = app.chars().filter(|c| c.is_alphanumeric()).collect();
        if !PLAYERS.iter().any(|p| squashed.contains(p)) {
            continue;
        }
        if let Some(pid) = pid_for(&squashed) {
            return Some(pid);
        }
    }
    None
}

/// The running app whose program name a media session's app id names.
fn pid_for(app: &str) -> Option<i32> {
    super::os::running_apps().into_iter().find_map(|r| {
        let stem = r.path.as_ref()?.file_stem()?.to_string_lossy().to_lowercase();
        let stem: String = stem.chars().filter(|c| c.is_alphanumeric()).collect();
        (!stem.is_empty() && (app.contains(&stem) || PLAYERS.iter().any(|p| app.contains(p) && stem.contains(p))))
            .then_some(r.pid)
    })
}
