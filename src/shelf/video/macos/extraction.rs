//! Reads frames for a GIF with an image generator, 24 at a time, and turns each one into opaque RGBA.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use objc2::rc::{Retained, autoreleasepool};
use objc2_av_foundation::{AVAsset, AVAssetImageGenerator};
use objc2_core_graphics::CGImage;
use objc2_foundation::NSError;

use super::super::{Frame, FrameSpec, MediaError};
use super::asset::{self, Media};
use super::diagnose;
use super::generator::{self, Generated};
use super::geometry::fit_within;
use super::raster::opaque_rgba;
use super::schedule::frame_times;
use super::wait::{self, Wait};
use crate::shelf::Task;

const FRAMES_PER_REQUEST: usize = 24;
const FRAME_PATIENCE: Duration = Duration::from_secs(60);
const ACTION: &str = "read frames from";

type Each<'a> = &'a mut dyn FnMut(Frame) -> Result<(), MediaError>;

pub(super) fn extract(source: &Path, spec: FrameSpec, task: &mut Task, each: Each) -> Result<(), MediaError> {
    autoreleasepool(|_| {
        let asset = asset::open(source).ok_or(MediaError::Unsupported)?;
        let times = planned_times(&asset, source, spec)?;
        let mut extraction = Extraction {
            generator: generator::upright_scaled(&asset, spec),
            source,
            max_side: spec.max_side,
            size: None,
            emitted: 0,
            first_error: None,
        };
        extraction.run(&times, task, each)
    })
}

fn planned_times(asset: &AVAsset, source: &Path, spec: FrameSpec) -> Result<Vec<f64>, MediaError> {
    let video =
        asset::tracks(asset, Media::Video).map_err(|error| diagnose::failure(ACTION, source, error.as_deref()))?;
    if video.count() == 0 {
        return Err(diagnose::refusal(ACTION, source, "it has no video track", MediaError::Unsupported));
    }
    let times = asset::duration_seconds(asset).map(|duration| frame_times(duration, spec)).unwrap_or_default();
    if times.is_empty() {
        return Err(diagnose::refusal(ACTION, source, "its duration is unknown", MediaError::Unsupported));
    }
    Ok(times)
}

struct Extraction<'a> {
    generator: Retained<AVAssetImageGenerator>,
    source: &'a Path,
    max_side: u32,
    size: Option<(usize, usize)>,
    emitted: usize,
    first_error: Option<Retained<NSError>>,
}

impl Extraction<'_> {
    fn run(&mut self, times: &[f64], task: &mut Task, each: Each) -> Result<(), MediaError> {
        let mut done = 0usize;
        for chunk in times.chunks(FRAMES_PER_REQUEST) {
            for generated in self.generate(chunk, task)? {
                if task.cancelled() {
                    return Err(MediaError::Cancelled);
                }
                autoreleasepool(|_| self.emit(generated, &mut *each))?;
                done += 1;
                task.report(done as f32 / times.len() as f32);
            }
        }
        if self.emitted > 0 { Ok(()) } else { Err(diagnose::failure(ACTION, self.source, self.first_error.as_deref())) }
    }

    fn generate(&self, chunk: &[f64], task: &mut Task) -> Result<Vec<Generated>, MediaError> {
        let (sender, receiver) = mpsc::channel();
        generator::request(&self.generator, chunk, sender);
        let mut batch = Vec::with_capacity(chunk.len());
        while batch.len() < chunk.len() {
            match wait::until(&receiver, task, Some(FRAME_PATIENCE), |_| {}) {
                Wait::Ready(generated) => batch.push(generated),
                Wait::Abandoned => break,
                Wait::Cancelled => return Err(self.stop(MediaError::Cancelled)),
                Wait::Stalled => {
                    let stalled =
                        diagnose::refusal(ACTION, self.source, "the decoder stopped responding", MediaError::Failed);
                    return Err(self.stop(stalled));
                }
            }
        }
        batch.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        Ok(batch)
    }

    fn stop(&self, error: MediaError) -> MediaError {
        generator::cancel(&self.generator);
        error
    }

    fn emit(&mut self, generated: Generated, each: Each) -> Result<(), MediaError> {
        let Some(image) = generated.image else {
            if self.first_error.is_none() {
                self.first_error = generated.error;
            }
            return Ok(());
        };
        let (width, height) = *self.size.get_or_insert_with(|| natural_fit(&image, self.max_side));
        let rgba = opaque_rgba(&image, width, height).ok_or(MediaError::Failed)?;
        self.emitted += 1;
        each(Frame { width: width as u32, height: height as u32, rgba, seconds: generated.seconds })
    }
}

fn natural_fit(image: &CGImage, max_side: u32) -> (usize, usize) {
    fit_within(CGImage::width(Some(image)), CGImage::height(Some(image)), max_side)
}
