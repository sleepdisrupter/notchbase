//! Other systems have no video support.

use std::path::Path;

use super::{Container, Frame, FrameSpec, MediaError, Probe, Quality};
use crate::shelf::Task;

pub(super) fn probe(_path: &Path) -> Option<Probe> {
    None
}

pub(super) fn can_write(_container: Container) -> bool {
    false
}

pub(super) fn transcode(
    _source: &Path,
    _target: &Path,
    _container: Container,
    _quality: Quality,
    _task: &mut Task,
) -> Result<(), MediaError> {
    Err(MediaError::Unsupported)
}

pub(super) fn frames(
    _source: &Path,
    _spec: FrameSpec,
    _task: &mut Task,
    _each: &mut dyn FnMut(Frame) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    Err(MediaError::Unsupported)
}
