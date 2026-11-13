//! Follows Music and Spotify. Each player's change notification marks it dirty, and only then, or every 10 seconds
//! while playing, is it asked what's playing.

use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::NSRunningApplication;
use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};

use super::players::{Artwork, PLAYERS};
use super::script;
use crate::media::applescript::{parse_data, parse_track};
use crate::media::track::{Control, Track};
use crate::system::permissions::macos::automation;
use crate::system::permissions::{Access, Standing, may_ask};

const DELIVERY_WINDOW: f64 = 0.05;
/// A playing player is asked again this often even without a notification, to stay in sync.
const RESYNC: Duration = Duration::from_secs(10);
const PLAYER_COUNT: usize = PLAYERS.len();

type Dirty = Arc<[AtomicBool; PLAYER_COUNT]>;

pub struct Source {
    dirty: Dirty,
    tracks: [Option<Track>; PLAYER_COUNT],
    synced_at: [Option<Instant>; PLAYER_COUNT],
    current: Option<usize>,
    scriptable: [bool; PLAYER_COUNT],
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl Source {
    pub fn new() -> Self {
        let dirty: Dirty = Arc::new(std::array::from_fn(|_| AtomicBool::new(true)));
        let observers = autoreleasepool(|_| (0..PLAYER_COUNT).map(|i| observe(i, dirty.clone())).collect());
        let synced_at = [None; PLAYER_COUNT];
        Self {
            dirty,
            tracks: Default::default(),
            synced_at,
            current: None,
            scriptable: [false; PLAYER_COUNT],
            observers,
        }
    }

    pub fn poll(&mut self) -> Option<Track> {
        // Run the loop briefly so the players' notifications get delivered.
        CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, DELIVERY_WINDOW, false);
        let now = Instant::now();
        for (i, player) in PLAYERS.iter().enumerate() {
            // Asking a player that isn't running would launch it.
            if !is_running(player.bundle) || !self.may_script(i) {
                self.tracks[i] = None;
                self.synced_at[i] = None;
                continue;
            }
            let playing = self.tracks[i].as_ref().is_some_and(|track| track.playing);
            let stale = self.synced_at[i].is_none_or(|at| playing && now.duration_since(at) >= RESYNC);
            if self.dirty[i].swap(false, Ordering::Relaxed) || stale {
                self.synced_at[i] = Some(now);
                let answer = script::run(&player.now_script());
                self.tracks[i] = answer.as_deref().and_then(|text| parse_track(text, player.seconds_per_duration_unit));
            }
        }
        // A playing player wins over a paused one.
        let playing = self.tracks.iter().position(|track| track.as_ref().is_some_and(|t| t.playing));
        self.current = playing.or_else(|| self.tracks.iter().position(Option::is_some));
        self.current.and_then(|i| self.tracks[i].clone())
    }

    /// Whether we may script the player: automation is granted, or it's fine for macOS to ask now.
    fn may_script(&mut self, i: usize) -> bool {
        if !self.scriptable[i] {
            self.scriptable[i] = automation::standing(PLAYERS[i].bundle) == Some(Standing::Granted);
        }
        self.scriptable[i] || may_ask(Access::Automation)
    }

    pub fn artwork(&mut self) -> Option<Vec<u8>> {
        let player = &PLAYERS[self.current?];
        let answer = script::run(&player.artwork_script())?;
        match player.artwork {
            Artwork::Url => script::download(&answer),
            Artwork::Raw => parse_data(&answer),
        }
    }

    pub fn control(&mut self, control: Control) {
        let Some(i) = self.current else { return };
        script::run(&PLAYERS[i].control_script(control));
        self.dirty[i].store(true, Ordering::Relaxed);
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let center = NSDistributedNotificationCenter::defaultCenter();
        for observer in &self.observers {
            let observer: &AnyObject = observer.as_ref();
            unsafe { center.removeObserver(observer) };
        }
    }
}

fn observe(index: usize, dirty: Dirty) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
    let name = NSString::from_str(PLAYERS[index].notification);
    let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
        dirty[index].store(true, Ordering::Relaxed);
    });
    let center = NSDistributedNotificationCenter::defaultCenter();
    unsafe { center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block) }
}

fn is_running(bundle: &str) -> bool {
    autoreleasepool(|_| {
        !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle)).is_empty()
    })
}
