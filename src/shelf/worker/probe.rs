//! Reads what the shelf shows about a file: its size, dimensions, pages or length, and a thumbnail.

use std::path::Path;

use egui::ColorImage;

use super::Info;
use crate::shelf::{ItemId, Kind, THUMB_PX, audio, images, pdf, video};

pub(super) fn info(id: ItemId, path: &Path, kind: Kind) -> Info {
    let bytes = std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len());
    let mut info = Info { id, bytes, dims: None, pages: None, seconds: None };
    match kind {
        Kind::Image => info.dims = images::dimensions(path),
        Kind::Pdf => info.pages = pdf::page_count(path),
        Kind::Video => {
            let probe = video::probe(path).unwrap_or_default();
            (info.dims, info.seconds) = (probe.dims, probe.seconds);
        }
        Kind::Audio => info.seconds = audio::seconds(path).or_else(|| video::probe(path)?.seconds),
        Kind::Folder | Kind::Other => {}
    }
    info
}

pub(super) fn thumbnail(path: &Path, kind: Kind) -> Option<ColorImage> {
    crate::filedrag::thumbnail(path, THUMB_PX).or_else(|| match kind {
        Kind::Image => images::thumbnail(path, THUMB_PX),
        Kind::Video | Kind::Audio | Kind::Pdf | Kind::Folder | Kind::Other => None,
    })
}
