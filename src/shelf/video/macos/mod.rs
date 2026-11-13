//! Video on macOS with AVFoundation: an export session for transcoding and an image generator for frames.

mod asset;
mod diagnose;
mod export;
mod extraction;
mod generator;
mod geometry;
mod loading;
mod presets;
mod raster;
mod schedule;
mod session;
mod wait;

use std::path::Path;

use objc2::rc::autoreleasepool;

use super::{Container, Frame, FrameSpec, MediaError, Probe, Quality};
use crate::shelf::Task;

pub(super) fn probe(path: &Path) -> Option<Probe> {
    autoreleasepool(|_| {
        let asset = asset::open(path)?;
        let seconds = asset::duration_seconds(&asset);
        let dims = asset::first_video_dims(&asset);
        (seconds.is_some() || dims.is_some()).then_some(Probe { dims, seconds })
    })
}

pub(super) fn can_write(container: Container) -> bool {
    matches!(container, Container::Mp4 | Container::Mov | Container::M4a)
}

pub(super) fn transcode(
    source: &Path,
    target: &Path,
    container: Container,
    quality: Quality,
    task: &mut Task,
) -> Result<(), MediaError> {
    export::transcode(source, target, container, quality, task)
}

pub(super) fn frames(
    source: &Path,
    spec: FrameSpec,
    task: &mut Task,
    each: &mut dyn FnMut(Frame) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    extraction::extract(source, spec, task, each)
}
