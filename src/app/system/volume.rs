//! The volume and brightness indicators, and passing volume changes on to the tools.

use std::time::Instant;

use crate::app::App;
use crate::app::preview::previewing;
use crate::audio::{AppId, MASTER};
use crate::config;

const NOT_SAVED: &str = "Couldn't save the volume indicator setting";
const BRIGHTNESS_NOT_SAVED: &str = "Couldn't save the brightness indicator setting";

impl App {
    pub(in crate::app) fn start_volume_indicator(&mut self) {
        let config = self.config.config();
        self.volume.set_enabled(config.volume.indicator && !previewing());
        self.volume.set_brightness_enabled(config.brightness.indicator && !previewing());
    }

    pub(in crate::app) fn set_volume_indicator(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.volume.indicator = on) {
            self.report(NOT_SAVED, err);
        }
        self.start_volume_indicator();
        self.redraw_notch();
    }

    pub(in crate::app) fn set_brightness_indicator(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.brightness.indicator = on) {
            self.report(BRIGHTNESS_NOT_SAVED, err);
        }
        self.start_volume_indicator();
        self.redraw_notch();
    }

    pub(in crate::app) fn quiet_volume_indicator(&mut self, id: AppId) {
        if id == MASTER {
            self.volume_quiet_until = Some(Instant::now() + config::current().volume.quiet_after_mixer());
        }
    }

    pub(in crate::app) fn sync_volume(&mut self, now: Instant) {
        self.volume.claim_media(self.panel.now_playing.is_some() || self.media.has_player());
        for key in self.volume.take_media_keys() {
            self.press_media_key(key);
        }
        if let Some(level) = self.volume.take_brightness() {
            self.update_notch(|notch| notch.show_brightness(level));
        }
        let Some(level) = self.volume.take_change() else { return };
        self.feed_volume(level);
        if self.volume_quiet_until.is_some_and(|until| now < until) {
            return;
        }
        self.update_notch(|notch| notch.show_volume(level.volume, level.muted));
    }
}
