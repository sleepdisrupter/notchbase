//! The formats the shelf writes (images for Convert, clips for video and audio) and the operations themselves.

use super::video::{self, Container};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Bmp,
    Gif,
    Ico,
    Pdf,
}

impl Format {
    pub const ALL: [Format; 8] =
        [Format::Png, Format::Jpeg, Format::Webp, Format::Tiff, Format::Bmp, Format::Gif, Format::Ico, Format::Pdf];

    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Tiff => "TIFF",
            Format::Bmp => "BMP",
            Format::Gif => "GIF",
            Format::Ico => "ICO",
            Format::Pdf => "PDF",
        }
    }

    pub(super) fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Tiff => "tiff",
            Format::Bmp => "bmp",
            Format::Gif => "gif",
            Format::Ico => "ico",
            Format::Pdf => "pdf",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clip {
    Mp4,
    Mov,
    Gif,
    M4a,
    Mp3,
    Wav,
    Flac,
}

impl Clip {
    pub fn for_video() -> Vec<Clip> {
        Self::writable([Clip::Mp4, Clip::Mov, Clip::Gif, Clip::M4a])
    }

    pub fn for_audio() -> Vec<Clip> {
        Self::writable([Clip::M4a, Clip::Mp3, Clip::Wav, Clip::Flac])
    }

    pub fn label(self) -> &'static str {
        match self {
            Clip::Mp4 => "MP4",
            Clip::Mov => "MOV",
            Clip::Gif => "GIF",
            Clip::M4a => "M4A",
            Clip::Mp3 => "MP3",
            Clip::Wav => "WAV",
            Clip::Flac => "FLAC",
        }
    }

    pub(super) fn extension(self) -> &'static str {
        match self {
            Clip::Mp4 => "mp4",
            Clip::Mov => "mov",
            Clip::Gif => "gif",
            Clip::M4a => "m4a",
            Clip::Mp3 => "mp3",
            Clip::Wav => "wav",
            Clip::Flac => "flac",
        }
    }

    pub(super) fn container(self) -> Option<Container> {
        match self {
            Clip::Mp4 => Some(Container::Mp4),
            Clip::Mov => Some(Container::Mov),
            Clip::M4a => Some(Container::M4a),
            Clip::Mp3 => Some(Container::Mp3),
            Clip::Gif | Clip::Wav | Clip::Flac => None,
        }
    }

    fn writable<const N: usize>(clips: [Clip; N]) -> Vec<Clip> {
        clips.into_iter().filter(|clip| clip.container().is_none_or(video::can_write)).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Convert(Format),
    Encode(Clip),
    Compress,
    Combine,
    Split,
}

impl Op {
    pub(super) fn verb(self) -> &'static str {
        match self {
            Op::Convert(_) | Op::Encode(_) => "convert",
            Op::Compress => "compress",
            Op::Combine => "combine",
            Op::Split => "split",
        }
    }
}
