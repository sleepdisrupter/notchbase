//! Audio the shelf handles itself: reading lengths, and writing WAV and FLAC from anything Symphonia can decode.
//! Compressed formats go through the system's encoder instead.

mod decode;
mod flac;
mod wav;

use std::path::Path;

use super::Task;
use super::video::MediaError;
use decode::Decoder;

const EXTENSIONS: [&str; 12] = ["mp3", "m4a", "m4b", "aac", "wav", "flac", "aif", "aiff", "aifc", "ogg", "oga", "caf"];

pub(super) fn is_audio_ext(ext: &str) -> bool {
    EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

pub(super) fn seconds(path: &Path) -> Option<f64> {
    decode::seconds(path)
}

pub(super) fn write_wav(source: &Path, target: &Path, task: &mut Task) -> Result<(), MediaError> {
    let mut decoder = Decoder::open(source)?;
    let mut writer = wav::Writer::create(target, decoder.shape)?;
    pump(&mut decoder, task, |samples| writer.write(samples))?;
    writer.finish()
}

pub(super) fn write_flac(source: &Path, target: &Path, task: &mut Task) -> Result<(), MediaError> {
    let mut decoder = Decoder::open(source)?;
    let mut writer = flac::Writer::create(target, decoder.shape)?;
    pump(&mut decoder, task, |samples| writer.write(samples))?;
    writer.finish()
}

fn pump(
    decoder: &mut Decoder,
    task: &mut Task,
    mut each: impl FnMut(&[i32]) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    while let Some(samples) = decoder.next()? {
        if task.cancelled() {
            return Err(MediaError::Cancelled);
        }
        each(samples)?;
        if let Some(done) = decoder.progress() {
            task.report(done);
        }
    }
    task.report(1.0);
    Ok(())
}
