//! Moves decoded samples from the reader to the writer, always taking the stream that is furthest behind.

use std::path::Path;

use super::diagnostics::Diagnose;
use super::progress::Progress;
use super::reader::{Decoded, Reader, Stream, TICKS_PER_SECOND};
use super::sink::Sink;
use crate::shelf::Task;
use crate::shelf::video::MediaError;

struct Lane {
    stream: Stream,
    index: u32,
    pending: Option<Decoded>,
}

pub(super) fn pump(reader: &Reader, sink: Sink, task: &mut Task, source: &Path) -> Result<(), MediaError> {
    let mut lanes = Vec::with_capacity(sink.lanes().len());
    for &(stream, index) in sink.lanes() {
        let pending = reader.next(stream).failed("decode", source)?;
        lanes.push(Lane { stream, index, pending });
    }
    let start = lanes.iter().filter_map(|lane| lane.pending.as_ref().map(|decoded| decoded.time)).min().unwrap_or(0);
    let mut progress = Progress::new(reader.duration_seconds());
    while let Some(lane) = earliest(&mut lanes) {
        if task.cancelled() {
            return Err(MediaError::Cancelled);
        }
        let Some(decoded) = lane.pending.take() else { break };
        let time = (decoded.time - start).max(0);
        unsafe { decoded.sample.SetSampleTime(time) }.failed("retime a sample", source)?;
        sink.write(lane.index, &decoded.sample).failed("encode", source)?;
        progress.reached(task, time as f64 / TICKS_PER_SECOND);
        lane.pending = reader.next(lane.stream).failed("decode", source)?;
    }
    if task.cancelled() {
        return Err(MediaError::Cancelled);
    }
    sink.finish().failed("finish writing", source)
}

fn earliest(lanes: &mut [Lane]) -> Option<&mut Lane> {
    lanes
        .iter_mut()
        .filter(|lane| lane.pending.is_some())
        .min_by_key(|lane| lane.pending.as_ref().map_or(i64::MAX, |decoded| decoded.time))
}
