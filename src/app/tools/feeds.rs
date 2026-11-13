//! Feeds the tools what they subscribed to: audio inputs, now playing, the notch's state, Bluetooth and volume.

use notchbase_sdk::protocol::{AudioInput, Device, DeviceChange, Event, NotchState, NowPlaying, VolumeLevel};

use crate::app::App;
use crate::audio::DeviceKind;
use crate::bridge::{Activity, Page};
use crate::media;
use crate::system::bluetooth::Change;
use crate::system::volume::Level;

const MOST_ACTIVITIES: usize = 3;
const UPDATE: &str = "update";
const ASSISTANT: &str = "assistant";

impl App {
    pub(super) fn feed_tools(&mut self) {
        if self.tools.wants_audio_inputs() {
            let inputs = self.panel.mixer.devices(DeviceKind::Input).iter().map(|device| AudioInput {
                id: device.id.clone(),
                name: device.name.clone(),
                is_default: device.is_default,
            });
            self.tools.feed_inputs(inputs.collect());
        }
        self.tools.feed_media(self.panel.now_playing.as_ref().map(now_playing));
        let page = self.visible_page();
        let key = page.and_then(|page| match page {
            Page::Tool(slot) => self.tools.host(slot).map(|host| host.id().to_owned()),
            core => core.core_key().map(str::to_owned),
        });
        let activities =
            self.activities().iter().take(MOST_ACTIVITIES).map(|activity| self.activity_id(activity)).collect();
        let shape = self.notches.active().map(|nw| nw.notch.state()).unwrap_or_default();
        self.tools.feed_notch(NotchState { open: page.is_some(), page: key, activities, ..shape });
    }

    fn activity_id(&self, activity: &Activity) -> String {
        match activity {
            Activity::Tool(slot, _) => self.tools.host(*slot).map_or(UPDATE, |host| host.id()).to_owned(),
            Activity::Agent(_) => ASSISTANT.to_owned(),
            Activity::Video(_) => "video".to_owned(),
            Activity::Lock(_) => "lock".to_owned(),
        }
    }

    pub(in crate::app) fn feed_bluetooth(&mut self, change: &Change) {
        let (device, wrap): (_, fn(Device) -> DeviceChange) = match change {
            Change::Connected(device) => (device, |device| DeviceChange::Connected { device }),
            Change::Disconnected(device) => (device, |device| DeviceChange::Disconnected { device }),
            Change::Updated(device) => (device, |device| DeviceChange::Updated { device }),
        };
        let device = Device {
            id: device.id.clone(),
            name: device.name.clone(),
            kind: device.kind.key().to_owned(),
            battery: device.battery,
        };
        self.tools.broadcast(&Event::Bluetooth { change: wrap(device) }, |wants| wants.bluetooth);
    }

    pub(in crate::app) fn feed_volume(&mut self, level: Level) {
        let level = VolumeLevel { volume: level.volume, muted: level.muted };
        self.tools.broadcast(&Event::Volume { level }, |wants| wants.volume);
    }
}

fn now_playing(now: &media::NowPlaying) -> NowPlaying {
    let (position, duration) = now.timeline.map_or((0.0, 0.0), |timeline| (timeline.position, timeline.duration));
    NowPlaying { title: now.title.clone(), artist: now.artist.clone(), playing: now.playing, position, duration }
}
