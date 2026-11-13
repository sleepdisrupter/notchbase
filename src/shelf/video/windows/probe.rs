//! Probes a video's displayed size and length.

use std::path::Path;

use super::reader::{Reader, Stream};
use super::runtime::Runtime;
use super::tracks::VideoTrack;
use crate::shelf::video::Probe;

pub(super) fn probe(path: &Path) -> Option<Probe> {
    let _runtime = Runtime::start(path).ok()?;
    let reader = Reader::open(path).ok()?;
    let dims = reader.native(Stream::Video).and_then(|native| VideoTrack::of(&native)).map(|video| video.displayed());
    Some(Probe { dims, seconds: reader.duration_seconds() })
}
