//! Converting audio. M4A and MP3 go through the system's encoder and WAV and FLAC are written here. A file the
//! system can't open is decoded to WAV first.

use std::path::Path;

use super::tally::FileResult;
use super::video::{failure, saved};
use super::{Outputs, Source, file_size};
use crate::shelf::names::file_stem;
use crate::shelf::video::{self, Container, MediaError, Quality};
use crate::shelf::{Clip, Task, audio};

pub(super) fn encode(source: &Source, clip: Clip, outputs: &Outputs, task: &mut Task) -> Result<FileResult, String> {
    let before = file_size(&source.path)?;
    let staged = outputs.stage(&source.path, &file_stem(&source.path), clip.extension())?;
    let written = match (clip, clip.container()) {
        (_, Some(container)) => through_system(source, staged.path(), container, outputs, task),
        (Clip::Wav, None) => audio::write_wav(&source.path, staged.path(), task),
        (Clip::Flac, None) => audio::write_flac(&source.path, staged.path(), task),
        _ => Err(MediaError::Unsupported),
    };
    written.map_err(|err| failure(err, source, "convert"))?;
    saved(staged, before)
}

fn through_system(
    source: &Source,
    target: &Path,
    container: Container,
    outputs: &Outputs,
    task: &mut Task,
) -> Result<(), MediaError> {
    match video::transcode(&source.path, target, container, Quality::Original, task) {
        Err(MediaError::Cancelled) => Err(MediaError::Cancelled),
        Err(_) => {
            let _ = std::fs::remove_file(target);
            let pcm = outputs
                .stage(&source.path, &file_stem(&source.path), Clip::Wav.extension())
                .map_err(|_| MediaError::Failed)?;
            audio::write_wav(&source.path, pcm.path(), &mut task.part(0, 2))?;
            video::transcode(pcm.path(), target, container, Quality::Original, &mut task.part(1, 2))
        }
        done => done,
    }
}
