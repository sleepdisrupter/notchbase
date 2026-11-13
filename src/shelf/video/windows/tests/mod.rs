mod cancel;
mod errors;
mod fixtures;
mod frames;
mod orientation;
mod probe;
mod shelf;
mod transcode;

use std::path::Path;

use super::reader::{Reader, Stream};
use super::runtime::Runtime;

pub(super) struct Streams {
    video: bool,
    audio: bool,
}

pub(super) fn streams(path: &Path) -> Streams {
    let _runtime = Runtime::start(path).unwrap();
    let reader = Reader::open(path).unwrap();
    Streams { video: reader.native(Stream::Video).is_some(), audio: reader.native(Stream::Audio).is_some() }
}

pub(super) fn about(actual: Option<f64>, expected: f64) -> bool {
    actual.is_some_and(|actual| (actual - expected).abs() < 0.2)
}
