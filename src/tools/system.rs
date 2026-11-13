//! The system feeds tools can ask for: power, screen lock, sensors, focus and downloads.

use notchbase_sdk::protocol::{DownloadChange, Event, Focus, Power, Sensors, Wants};

use super::{Feeds, ToolHost, Tools};

impl Tools {
    pub fn wants_power(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().power)
    }

    pub fn wants_screen_lock(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().screen_lock)
    }

    pub fn wants_sensors(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().sensors)
    }

    pub fn wants_downloads(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().downloads)
    }

    pub fn feed_power(&mut self, power: Power) {
        if self.feeds.power == Some(power) {
            return;
        }
        self.feeds.power = Some(power);
        self.broadcast(&Event::Power { power }, |wants| wants.power);
    }

    pub fn feed_screen_lock(&mut self, locked: bool) {
        if self.feeds.locked == Some(locked) {
            return;
        }
        self.feeds.locked = Some(locked);
        self.broadcast(&Event::ScreenLock { locked }, |wants| wants.screen_lock);
    }

    pub fn feed_sensors(&mut self, sensors: Sensors) {
        if self.feeds.sensors.as_ref() == Some(&sensors) {
            return;
        }
        self.feeds.sensors = Some(sensors.clone());
        self.broadcast(&Event::Sensors { sensors }, |wants| wants.sensors);
    }

    pub fn wants_focus(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().focus)
    }

    pub fn feed_focus(&mut self, focus: Focus) {
        if self.feeds.focus.as_ref() == Some(&focus) {
            return;
        }
        self.feeds.focus = Some(focus.clone());
        self.broadcast(&Event::Focus { focus }, |wants| wants.focus);
    }

    pub fn feed_download(&mut self, change: DownloadChange) {
        self.broadcast(&Event::Download { change }, |wants| wants.downloads);
    }
}

/// Gives a tool that just started wanting a feed its last value.
pub(super) fn catch_up(feeds: &Feeds, host: &mut ToolHost, before: &Wants) {
    let now = host.wants().clone();
    if now.power
        && !before.power
        && let Some(power) = feeds.power
    {
        host.queue(Event::Power { power });
    }
    if now.screen_lock
        && !before.screen_lock
        && let Some(locked) = feeds.locked
    {
        host.queue(Event::ScreenLock { locked });
    }
    if now.sensors
        && !before.sensors
        && let Some(sensors) = &feeds.sensors
    {
        host.queue(Event::Sensors { sensors: sensors.clone() });
    }
    if now.focus
        && !before.focus
        && let Some(focus) = &feeds.focus
    {
        host.queue(Event::Focus { focus: focus.clone() });
    }
}
