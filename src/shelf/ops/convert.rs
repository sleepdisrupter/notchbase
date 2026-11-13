//! Convert: an image into another image format, or into a one-page PDF.

use super::tally::FileResult;
use super::{Outputs, Source, file_size};
use crate::shelf::names::{file_name, file_stem};
use crate::shelf::{Format, Kind, images, pdf};

pub(super) fn one(source: &Source, format: Format, outputs: &Outputs) -> Result<FileResult, String> {
    if source.kind != Kind::Image {
        return Err(format!("Can't convert {}", file_name(&source.path)));
    }
    let before = file_size(&source.path)?;
    let bytes = match format {
        Format::Pdf => pdf::combine(std::slice::from_ref(source), &mut |_| {})?.0,
        format => images::convert(&source.path, format)?,
    };
    let path = outputs.write(&source.path, &file_stem(&source.path), format.extension(), &bytes)?;
    Ok(FileResult::Saved { path, before, after: bytes.len() as u64 })
}
