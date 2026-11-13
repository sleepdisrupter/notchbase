//! What kind of file an item is, from its extension: an image, video, audio, a PDF, a folder or something else.

use std::path::Path;

use super::names::lowercase_extension;
use super::{audio, images, video};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Pdf,
    Folder,
    Other,
}

impl Kind {
    pub(super) fn of_path(path: &Path, is_dir: bool) -> Kind {
        if is_dir {
            return Kind::Folder;
        }
        let ext = lowercase_extension(path);
        if ext == "pdf" {
            Kind::Pdf
        } else if images::is_image_ext(&ext) {
            Kind::Image
        } else if video::is_video_ext(&ext) {
            Kind::Video
        } else if audio::is_audio_ext(&ext) {
            Kind::Audio
        } else {
            Kind::Other
        }
    }
}
