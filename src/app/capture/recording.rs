//! Screen recording: starting and stopping the recorder, and its live activity in the notch.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use notchbase_sdk::now_ms;
use notchbase_sdk::protocol::{RecordingOptions, RecordingState, RecordingUpdate};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::capture::CaptureMode;
use crate::record::{Options, Region, Saved};

const PREVIEWED_FOR: Duration = Duration::from_secs(42);

impl App {
    pub(in crate::app) fn record_for(&mut self, event_loop: &ActiveEventLoop, options: RecordingOptions) {
        if self.recorder.is_busy() || self.choosing_area() {
            return;
        }
        self.recording_options = Some(options);
        self.request_capture(event_loop, CaptureMode::Area);
    }

    pub(super) fn begin_screen_recording(&mut self, region: Region) {
        let wanted = self.recording_options.take().unwrap_or_default();
        let options = Options {
            microphone: wanted.microphone,
            desktop: wanted.desktop_audio,
            microphone_id: wanted.microphone_id,
            folder: wanted.folder.map(PathBuf::from),
            max_fps: wanted.max_fps,
        };
        match self.recorder.start(region, &options) {
            Ok(()) if wanted.dim_outside => self.recording_dim.expect(region),
            Ok(()) => {}
            Err(error) => self.tools.announce_recording(RecordingUpdate::Failed { error }),
        }
        self.sync_screen_recording();
    }

    pub(in crate::app) fn sync_screen_recording(&mut self) {
        match self.recorder.poll() {
            Some(Ok(saved)) => self.recording_saved(saved),
            Some(Err(error)) => self.tools.announce_recording(RecordingUpdate::Failed { error }),
            None => {}
        }
        let state = self.recording_state();
        self.tools.feed_recording(state);
    }

    pub(in crate::app) fn preview_screen_recording(&mut self) {
        self.previewed_recording = Instant::now().checked_sub(PREVIEWED_FOR);
        self.sync_screen_recording();
    }

    fn recording_state(&self) -> RecordingState {
        if let Some(since) = self.previewed_recording {
            return RecordingState::Recording { since: unix_since(since) };
        }
        match (self.recorder.is_busy(), self.recorder.is_recording(), self.recorder.since()) {
            (false, _, _) if self.choosing_area() => RecordingState::Choosing,
            (false, _, _) => RecordingState::Idle,
            (true, false, _) => RecordingState::Saving,
            (true, true, Some(since)) => RecordingState::Recording { since: unix_since(since) },
            (true, true, None) => RecordingState::Starting,
        }
    }

    fn recording_saved(&mut self, saved: Saved) {
        let path = saved.path.to_string_lossy().into_owned();
        self.tools.announce_recording(RecordingUpdate::Saved { path, seconds: saved.seconds, note: saved.note });
    }
}

fn unix_since(since: Instant) -> u64 {
    static ANCHOR: LazyLock<(Instant, u64)> = LazyLock::new(|| (Instant::now(), now_ms()));
    let (instant, unix) = *ANCHOR;
    let millis = |duration: Duration| duration.as_millis().min(u128::from(u64::MAX)) as u64;
    match since.checked_duration_since(instant) {
        Some(after) => unix.saturating_add(millis(after)),
        None => unix.saturating_sub(millis(instant.duration_since(since))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_maps_to_one_wall_time() {
        let since = Instant::now() - Duration::from_secs(5);
        assert_eq!(unix_since(since), unix_since(since));
        assert!(now_ms().abs_diff(unix_since(since) + 5_000) < 1_000);
    }
}
