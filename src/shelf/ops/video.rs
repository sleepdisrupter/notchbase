//! Converting and compressing video. Most formats go through the system's encoder, and a GIF is built here from
//! frames, 12 a second, no bigger than 480 pixels and no longer than 30 seconds.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage};

use super::compress::worth_keeping;
use super::outputs::Staged;
use super::tally::FileResult;
use super::{Outputs, Source, file_size};
use crate::shelf::names::{file_name, file_stem};
use crate::shelf::video::{self, Container, FrameSpec, MediaError, Quality};
use crate::shelf::{Clip, Kind, Task};

const GIF: FrameSpec = FrameSpec { max_side: 480, fps: 12.0, max_seconds: 30.0 };
const GIF_SPEED: i32 = 10;
const MIN_FRAME_MS: f64 = 20.0;
const MAX_FRAME_MS: f64 = 10_000.0;

pub(super) fn encode(source: &Source, clip: Clip, outputs: &Outputs, task: &mut Task) -> Result<FileResult, String> {
    if source.kind != Kind::Video {
        return Err(format!("Can't convert {}", file_name(&source.path)));
    }
    let before = file_size(&source.path)?;
    let staged = outputs.stage(&source.path, &file_stem(&source.path), clip.extension())?;
    let written = match clip.container() {
        Some(container) => video::transcode(&source.path, staged.path(), container, Quality::Original, task),
        None => write_gif(&source.path, staged.path(), task),
    };
    written.map_err(|err| failure(err, source, "convert"))?;
    saved(staged, before)
}

pub(super) fn compress(source: &Source, outputs: &Outputs, task: &mut Task) -> Result<FileResult, String> {
    let before = file_size(&source.path)?;
    let stem = format!("{} compressed", file_stem(&source.path));
    let staged = outputs.stage(&source.path, &stem, Clip::Mp4.extension())?;
    video::transcode(&source.path, staged.path(), Container::Mp4, Quality::Compact, task)
        .map_err(|err| failure(err, source, "compress"))?;
    if !worth_keeping(before, file_size(staged.path())?) {
        return Ok(FileResult::AlreadyCompact);
    }
    saved(staged, before)
}

pub(super) fn saved(staged: Staged, before: u64) -> Result<FileResult, String> {
    let after = file_size(staged.path())?;
    let path = staged.commit()?;
    Ok(FileResult::Saved { path, before, after })
}

fn write_gif(source: &Path, target: &Path, task: &mut Task) -> Result<(), MediaError> {
    let file = File::create(target).map_err(|_| MediaError::Failed)?;
    let mut encoder = GifEncoder::new_with_speed(BufWriter::new(file), GIF_SPEED);
    encoder.set_repeat(Repeat::Infinite).map_err(|_| MediaError::Failed)?;
    let mut pending: Option<(RgbaImage, f64)> = None;
    video::frames(source, GIF, task, &mut |frame| {
        let image = RgbaImage::from_raw(frame.width, frame.height, frame.rgba).ok_or(MediaError::Failed)?;
        match pending.replace((image, frame.seconds)) {
            Some((previous, at)) => encoder.encode_frame(gif_frame(previous, frame.seconds - at)),
            None => Ok(()),
        }
        .map_err(|_| MediaError::Failed)
    })?;
    let (last, _) = pending.ok_or(MediaError::Failed)?;
    encoder.encode_frame(gif_frame(last, 1.0 / GIF.fps)).map_err(|_| MediaError::Failed)
}

fn gif_frame(image: RgbaImage, seconds: f64) -> Frame {
    let millis = (seconds * 1000.0).round().clamp(MIN_FRAME_MS, MAX_FRAME_MS) as u32;
    Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(millis, 1))
}

pub(super) fn failure(err: MediaError, source: &Source, verb: &str) -> String {
    let name = file_name(&source.path);
    match err {
        MediaError::Cancelled => "Cancelled".to_owned(),
        MediaError::NoAudio => format!("{name} has no audio"),
        MediaError::Unsupported | MediaError::Failed => format!("Couldn't {verb} {name}"),
    }
}
