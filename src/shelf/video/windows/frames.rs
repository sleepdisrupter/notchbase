//! Reads frames for a GIF: decoded, turned upright and handed over one at a time on schedule.

use std::path::Path;

use ::windows::Win32::Foundation::E_UNEXPECTED;
use ::windows::Win32::Media::MediaFoundation::{MF_MT_DEFAULT_STRIDE, MFVideoFormat_RGB32};
use ::windows::core::Error;

use super::diagnostics::{Diagnose, complain};
use super::geometry::{Turn, fit_within};
use super::media_type::uncompressed_video;
use super::pixels::{Layout, sample_rgba, turned};
use super::progress::Progress;
use super::reader::{Decoded, Reader, Stream, TICKS_PER_SECOND};
use super::runtime::Runtime;
use super::schedule::Schedule;
use super::tracks::VideoTrack;
use crate::shelf::Task;
use crate::shelf::video::{Frame, FrameSpec, MediaError};

struct Emitter<'a, 'b> {
    source: &'a Path,
    layout: Layout,
    turn: Turn,
    task: &'a mut Task<'b>,
    progress: Progress,
    each: &'a mut dyn FnMut(Frame) -> Result<(), MediaError>,
}

pub(super) fn frames(
    source: &Path,
    spec: FrameSpec,
    task: &mut Task,
    each: &mut dyn FnMut(Frame) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    let _runtime = Runtime::start(source)?;
    let reader = Reader::open(source).unsupported("open", source)?;
    let video = reader
        .native(Stream::Video)
        .and_then(|native| VideoTrack::of(&native))
        .ok_or_else(|| complain(MediaError::Unsupported, "find a video stream", source, "none present"))?;
    reader.keep_only(&[Stream::Video]).failed("select the video stream", source)?;
    let size = fit_within(video.visible, spec.max_side.max(1));
    let layout = decode_as_bgrx(&reader, size).unsupported("decode the video stream", source)?;
    let limit = reader.duration_seconds().map_or(spec.max_seconds, |seconds| seconds.min(spec.max_seconds));
    let mut schedule = Schedule::new(spec.fps, limit);
    let progress = Progress::new(Some(limit));
    let mut emitter = Emitter { source, layout, turn: video.turn, task, progress, each };
    let mut start = None;
    let mut shown: Option<(Decoded, f64)> = None;
    while !schedule.finished() {
        if emitter.task.cancelled() {
            return Err(MediaError::Cancelled);
        }
        let Some(decoded) = reader.next(Stream::Video).failed("decode", source)? else { break };
        let start = *start.get_or_insert(decoded.time);
        let seconds = (decoded.time - start).max(0) as f64 / TICKS_PER_SECOND;
        if let Some((previous, _)) = shown.replace((decoded, seconds)) {
            emitter.emit(&previous, schedule.due_before(seconds))?;
        }
    }
    if let Some((last, seconds)) = shown {
        let shown_for = last.duration_seconds().unwrap_or(1.0 / spec.fps.max(1.0));
        emitter.emit(&last, schedule.due_before(seconds + shown_for))?;
    }
    Ok(())
}

fn decode_as_bgrx(reader: &Reader, size: [u32; 2]) -> Result<Layout, Error> {
    let request = uncompressed_video(&MFVideoFormat_RGB32, size)?;
    unsafe { request.SetUINT32(&MF_MT_DEFAULT_STRIDE, size[0] * 4) }?;
    reader.set_output(Stream::Video, &request)?;
    Layout::of(&reader.output(Stream::Video)?).ok_or_else(|| Error::from(E_UNEXPECTED))
}

impl Emitter<'_, '_> {
    fn emit(&mut self, decoded: &Decoded, times: Vec<f64>) -> Result<(), MediaError> {
        if times.is_empty() {
            return Ok(());
        }
        let image = sample_rgba(&decoded.sample, self.layout)
            .failed("read a frame", self.source)?
            .ok_or_else(|| complain(MediaError::Failed, "read a frame", self.source, "unexpected buffer size"))?;
        let image = turned(image, self.turn);
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        for seconds in times {
            if self.task.cancelled() {
                return Err(MediaError::Cancelled);
            }
            (self.each)(Frame { width, height, rgba: rgba.clone(), seconds })?;
            self.progress.reached(self.task, seconds);
        }
        Ok(())
    }
}
