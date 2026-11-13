//! Now playing on Windows through the System Media Transport Controls, which apps on the media overlay report to.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::core::{Interface, Result};
use windows_future::IAsyncOperation;

use crate::media::track::{Control, Timeline, Track};
use crate::platform::Apartment;

const LARGEST_ARTWORK: u64 = 8 << 20;
const TICKS_PER_SECOND: f64 = 10_000_000.0;
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

pub struct Source {
    manager: Option<Manager>,
    session: Option<Session>,
    properties: Option<Properties>,
    start_ticks: i64,
    _apartment: Apartment,
}

impl Source {
    pub fn new() -> Self {
        let apartment = Apartment::join();
        Self { manager: None, session: None, properties: None, start_ticks: 0, _apartment: apartment }
    }

    pub fn poll(&mut self) -> Option<Track> {
        self.properties = None;
        self.session = None;
        if self.manager.is_none() {
            self.manager = Manager::RequestAsync().and_then(|op| op.join()).ok();
        }
        let manager = self.manager.as_ref()?;
        let (session, playing) = pick_session(manager)?;
        let properties = session.TryGetMediaPropertiesAsync().and_then(|op| op.join()).ok()?;
        let title = properties.Title().ok()?.to_string_lossy();
        if title.trim().is_empty() {
            return None;
        }
        let artist = properties.Artist().map(|artist| artist.to_string_lossy()).unwrap_or_default();
        let app = session.SourceAppUserModelId().map(|id| id.to_string_lossy()).unwrap_or_default();
        let timeline = self.timeline_of(&session, playing);
        self.properties = Some(properties);
        self.session = Some(session);
        Some(Track { artwork_key: format!("{app}\u{1f}{title}\u{1f}{artist}"), title, artist, playing, timeline })
    }

    pub fn artwork(&mut self) -> Option<Vec<u8>> {
        read_thumbnail(self.properties.as_ref()?).ok()
    }

    pub fn control(&mut self, control: Control) {
        let Some(session) = &self.session else { return };
        let request = match control {
            Control::PlayPause => session.TryTogglePlayPauseAsync(),
            Control::Next => session.TrySkipNextAsync(),
            Control::Previous => session.TrySkipPreviousAsync(),
            Control::Seek(seconds) => {
                // Positions count from the timeline's start, so a seek adds it back.
                let ticks = self.start_ticks + (seconds.max(0.0) * TICKS_PER_SECOND) as i64;
                session.TryChangePlaybackPositionAsync(ticks)
            }
        };
        let _ = request.and_then(|op| op.join());
    }

    fn timeline_of(&mut self, session: &Session, playing: bool) -> Option<Timeline> {
        let timeline = session.GetTimelineProperties().ok()?;
        let start = timeline.StartTime().ok()?.Duration;
        let end = timeline.EndTime().ok()?.Duration;
        let mut position = timeline.Position().ok()?.Duration - start;
        let updated = timeline.LastUpdatedTime().map(|time| time.UniversalTime).unwrap_or(0);
        // The player only updates its position now and then, so add the time since it last did.
        if playing && updated > 0 {
            position += (now_ticks() - updated).max(0);
        }
        self.start_ticks = start;
        Timeline::new(position as f64 / TICKS_PER_SECOND, (end - start) as f64 / TICKS_PER_SECOND, Instant::now())
    }
}

/// Now in Windows time: 100 ns ticks since 1601.
fn now_ticks() -> i64 {
    let since_unix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    UNIX_EPOCH_TICKS + (since_unix.as_nanos() / 100) as i64
}

/// A playing session wins, otherwise the one Windows calls current.
fn pick_session(manager: &Manager) -> Option<(Session, bool)> {
    let is_playing = |session: &Session| {
        session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).is_ok_and(|status| status == Status::Playing)
    };
    if let Ok(sessions) = manager.GetSessions() {
        let count = sessions.Size().unwrap_or(0);
        if let Some(playing) = (0..count).filter_map(|i| sessions.GetAt(i).ok()).find(|s| is_playing(s)) {
            return Some((playing, true));
        }
    }
    let current = manager.GetCurrentSession().ok()?;
    let playing = is_playing(&current);
    Some((current, playing))
}

fn read_thumbnail(properties: &Properties) -> Result<Vec<u8>> {
    let stream = properties.Thumbnail()?.OpenReadAsync()?.join()?;
    let size = stream.Size()?.min(LARGEST_ARTWORK) as u32;
    let reader = DataReader::CreateDataReader(&stream)?;
    let loading: IAsyncOperation<u32> = reader.LoadAsync(size)?.cast()?;
    let loaded = loading.join()?;
    let mut bytes = vec![0; loaded as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_counted_from_1601() {
        let ticks = now_ticks();
        let year_2020 = UNIX_EPOCH_TICKS + 1_577_836_800 * 10_000_000;
        assert!(ticks > year_2020);
    }
}
