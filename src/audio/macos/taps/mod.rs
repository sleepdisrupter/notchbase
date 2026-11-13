//! Per-app taps. A tap opens when an app goes below full volume or is muted and closes once it's back at 100%. A tap
//! that fails to open isn't tried again for five seconds.

mod aggregate;
mod api;
mod description;
mod io_proc;
mod listener;
mod process_tap;
mod tap;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;

use self::api::TapApi;
use self::tap::Tap;
use super::devices::Output;
use super::permission::{Permission, audio_capture_permission};
use super::processes::AppProcesses;
use crate::audio::AppId;
use crate::audio::core_audio::gain::{gain, needs_tap};

pub use self::aggregate::is_mixer_aggregate;
pub use self::listener::Listener;

const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapError {
    Permission,
    Failed,
}

#[derive(Clone, Copy)]
pub struct Mix {
    pub volume: f32,
    pub muted: bool,
}

impl Mix {
    pub const UNITY: Self = Self { volume: 1.0, muted: false };

    pub fn needs_tap(self) -> bool {
        needs_tap(self.volume, self.muted)
    }

    fn gain(self) -> f32 {
        gain(self.volume, self.muted)
    }
}

pub struct Taps {
    api: Option<TapApi>,
    permission: Permission,
    desired: HashMap<AppId, Mix>,
    active: HashMap<AppId, Tap>,
    failed: HashMap<AppId, Instant>,
}

impl Taps {
    pub fn new() -> Self {
        Self {
            api: TapApi::load(),
            permission: Permission::Unknown,
            desired: HashMap::new(),
            active: HashMap::new(),
            failed: HashMap::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.api.is_some()
    }

    pub fn is_denied(&self) -> bool {
        self.permission == Permission::Denied
    }

    pub fn refresh_permission(&mut self) {
        self.permission = audio_capture_permission();
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn is_active(&self, id: AppId) -> bool {
        self.active.contains_key(&id)
    }

    pub fn is_missing(&self, id: AppId) -> bool {
        self.desired(id).needs_tap() && !self.is_active(id)
    }

    pub fn desired(&self, id: AppId) -> Mix {
        self.desired.get(&id).copied().unwrap_or(Mix::UNITY)
    }

    pub fn take_peak(&self, id: AppId) -> f32 {
        self.active.get(&id).map_or(0.0, Tap::take_peak)
    }

    pub fn desire(&mut self, id: AppId, current: Mix, volume: Option<f32>, muted: Option<bool>) -> Mix {
        let entry = self.desired.entry(id).or_insert(current);
        if let Some(volume) = volume {
            entry.volume = volume;
        }
        if let Some(muted) = muted {
            entry.muted = muted;
        }
        *entry
    }

    pub fn retune(&self, id: AppId, mix: Mix) -> bool {
        let Some(tap) = self.active.get(&id) else { return false };
        tap.set_gain(mix.gain());
        true
    }

    pub fn reset(&mut self) {
        self.active.clear();
        self.desired.clear();
    }

    pub fn close_all(&mut self) {
        self.active.clear();
    }

    pub fn forget_unlisted(&mut self, listed: &HashSet<AppId>) {
        self.active.retain(|id, _| listed.contains(id));
        self.failed.retain(|id, _| listed.contains(id));
    }

    pub fn drop_unneeded(&mut self, id: AppId, owner: &AppProcesses, output_uid: &str) {
        if !self.desired(id).needs_tap() {
            self.active.remove(&id);
            self.desired.remove(&id);
            return;
        }
        if self.active.get(&id).is_some_and(|tap| !tap.serves(owner, output_uid)) {
            self.active.remove(&id);
        }
    }

    pub fn start(&mut self, id: AppId, owner: &AppProcesses, output: Option<Output<'_>>) -> bool {
        let now = Instant::now();
        if self.failed.get(&id).is_some_and(|&t| now.duration_since(t) < RETRY_AFTER) {
            return self.abandon(id);
        }
        let (Some(api), Some(output)) = (self.api, output) else { return self.abandon(id) };
        if self.is_denied() {
            return self.abandon(id);
        }
        let gain = self.desired(id).gain();
        match autoreleasepool(|_| Tap::create(api, owner, output, gain)) {
            Ok(tap) => {
                self.failed.remove(&id);
                self.active.insert(id, tap);
                true
            }
            Err(error) => {
                if error == TapError::Permission {
                    self.permission = Permission::Denied;
                }
                self.failed.insert(id, now);
                self.abandon(id)
            }
        }
    }

    fn abandon(&mut self, id: AppId) -> bool {
        self.desired.remove(&id);
        false
    }
}
