//! Compress: a smaller copy of each file, kept only when it saves at least 3%.

use super::tally::FileResult;
use super::{Outputs, Source, file_size, video};
use crate::shelf::names::{file_name, file_stem};
use crate::shelf::{Kind, Task, images, pdf};

const MIN_SAVING_PERCENT: u128 = 3;

pub(super) fn one(source: &Source, outputs: &Outputs, task: &mut Task) -> Result<FileResult, String> {
    if source.kind == Kind::Video {
        return video::compress(source, outputs, task);
    }
    let before = file_size(&source.path)?;
    let (bytes, ext) = compressed(source)?;
    let after = bytes.len() as u64;
    if !worth_keeping(before, after) {
        return Ok(FileResult::AlreadyCompact);
    }
    let stem = format!("{} compressed", file_stem(&source.path));
    let path = outputs.write(&source.path, &stem, &ext, &bytes)?;
    Ok(FileResult::Saved { path, before, after })
}

fn compressed(source: &Source) -> Result<(Vec<u8>, String), String> {
    match source.kind {
        Kind::Image => images::compress(&source.path),
        Kind::Pdf => Ok((pdf::compress(&source.path)?, "pdf".to_owned())),
        Kind::Video | Kind::Audio | Kind::Folder | Kind::Other => {
            Err(format!("Can't compress {}", file_name(&source.path)))
        }
    }
}

pub(super) fn worth_keeping(before: u64, after: u64) -> bool {
    before > 0 && u128::from(after) * 100 <= u128::from(before) * (100 - MIN_SAVING_PERCENT)
}
