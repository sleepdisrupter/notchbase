//! Transcoding: open the reader, plan the tracks, open the sink and pump the samples across.

use std::path::Path;

use super::diagnostics::Diagnose;
use super::plan::tracks;
use super::pump::pump;
use super::reader::Reader;
use super::runtime::Runtime;
use super::sink::Sink;
use crate::shelf::Task;
use crate::shelf::video::{Container, MediaError, Quality};

pub(super) fn transcode(
    source: &Path,
    target: &Path,
    container: Container,
    quality: Quality,
    task: &mut Task,
) -> Result<(), MediaError> {
    let _runtime = Runtime::start(source)?;
    let reader = Reader::open(source).unsupported("open", source)?;
    let tracks = tracks(&reader, container, quality, source)?;
    if task.cancelled() {
        return Err(MediaError::Cancelled);
    }
    let sink = Sink::open(target, container, &tracks, source)?;
    pump(&reader, sink, task, source)
}
