//! Video on Windows with Media Foundation: a source reader to decode and a sink writer to encode.

mod diagnostics;
mod frames;
mod geometry;
mod media_type;
mod pixels;
mod plan;
mod probe;
mod progress;
mod pump;
mod reader;
mod runtime;
mod schedule;
mod settings;
mod sink;
#[cfg(test)]
mod tests;
mod tracks;
mod transcode;

use std::path::Path;

use super::{Container, Frame, FrameSpec, MediaError, Probe, Quality};
use crate::shelf::Task;

pub(super) fn probe(path: &Path) -> Option<Probe> {
    probe::probe(path)
}

pub(super) fn can_write(container: Container) -> bool {
    match container {
        Container::Mp4 | Container::M4a | Container::Mp3 => true,
        Container::Mov => false,
    }
}

pub(super) fn transcode(
    source: &Path,
    target: &Path,
    container: Container,
    quality: Quality,
    task: &mut Task,
) -> Result<(), MediaError> {
    transcode::transcode(source, target, container, quality, task)
}

pub(super) fn frames(
    source: &Path,
    spec: FrameSpec,
    task: &mut Task,
    each: &mut dyn FnMut(Frame) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    frames::frames(source, spec, task, each)
}
