//! Which export presets to try: the highest quality for originals, HEVC then H.264 at 1080p then medium for compact
//! copies, and Apple M4A for audio.

use objc2_av_foundation::{
    AVAssetExportPreset1920x1080, AVAssetExportPresetAppleM4A, AVAssetExportPresetHEVC1920x1080,
    AVAssetExportPresetHighestQuality, AVAssetExportPresetMediumQuality, AVFileType, AVFileTypeAppleM4A,
    AVFileTypeMPEG4, AVFileTypeQuickTimeMovie,
};
use objc2_foundation::NSString;

use super::super::{Container, Quality};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preset {
    Highest,
    Hevc1080,
    Hd1080,
    Medium,
    AppleM4a,
}

const ORIGINAL: [Preset; 1] = [Preset::Highest];
const COMPACT: [Preset; 3] = [Preset::Hevc1080, Preset::Hd1080, Preset::Medium];
const AUDIO_ONLY: [Preset; 1] = [Preset::AppleM4a];

pub(super) fn candidates(container: Container, quality: Quality) -> &'static [Preset] {
    match (container, quality) {
        (Container::M4a, _) => &AUDIO_ONLY,
        (Container::Mp3, _) => &[],
        (_, Quality::Compact) => &COMPACT,
        (_, Quality::Original) => &ORIGINAL,
    }
}

pub(super) fn file_type(container: Container) -> Option<&'static AVFileType> {
    match container {
        Container::Mp4 => unsafe { AVFileTypeMPEG4 },
        Container::Mov => unsafe { AVFileTypeQuickTimeMovie },
        Container::M4a => unsafe { AVFileTypeAppleM4A },
        Container::Mp3 => None,
    }
}

impl Preset {
    pub(super) fn name(self) -> &'static NSString {
        match self {
            Preset::Highest => unsafe { AVAssetExportPresetHighestQuality },
            Preset::Hevc1080 => unsafe { AVAssetExportPresetHEVC1920x1080 },
            Preset::Hd1080 => unsafe { AVAssetExportPreset1920x1080 },
            Preset::Medium => unsafe { AVAssetExportPresetMediumQuality },
            Preset::AppleM4a => unsafe { AVAssetExportPresetAppleM4A },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_keeps_highest_quality() {
        assert_eq!(candidates(Container::Mp4, Quality::Original), &[Preset::Highest]);
        assert_eq!(candidates(Container::Mov, Quality::Original), &[Preset::Highest]);
    }

    #[test]
    fn compact_prefers_hevc() {
        assert_eq!(candidates(Container::Mp4, Quality::Compact), &[Preset::Hevc1080, Preset::Hd1080, Preset::Medium]);
    }

    #[test]
    fn audio_uses_apple_m4a() {
        assert_eq!(candidates(Container::M4a, Quality::Original), &[Preset::AppleM4a]);
        assert_eq!(candidates(Container::M4a, Quality::Compact), &[Preset::AppleM4a]);
    }
}
