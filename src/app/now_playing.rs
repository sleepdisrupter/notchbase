//! Keeps the notch's music in step with what's playing, and sends the media keys to the player.

use crate::media::Control;
use crate::system::volume::MediaKey;

use super::App;

impl App {
    pub(super) fn sync_now_playing(&mut self) {
        let sample = self.notches.iter().find_map(|nw| nw.notch.previewed_music());
        self.panel.now_playing = sample.unwrap_or_else(|| self.media.now_playing());
        let (now, levels) = (self.panel.now_playing.as_ref(), self.media.levels());
        for (_, nw) in self.notches.windows_mut() {
            if nw.notch.set_now_playing(now) {
                nw.redraw();
            }
            nw.notch.set_music_levels(levels);
        }
        self.media.listen(self.notches.iter().any(|nw| nw.notch.dances()));
    }

    pub(super) fn control_media(&mut self, control: Control) {
        self.update_notch(|notch| notch.anticipate(control));
        self.media.control(control);
    }

    pub(super) fn press_media_key(&mut self, key: MediaKey) {
        self.control_media(match key {
            MediaKey::PlayPause => Control::PlayPause,
            MediaKey::Next => Control::Next,
            MediaKey::Previous => Control::Previous,
        });
    }
}
