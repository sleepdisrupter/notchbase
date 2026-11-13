//! A video track's visible size, turn and frame rate, read from the display aperture with anamorphic pixels squared.

use ::windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_VIDEO_ROTATION,
};

use super::geometry::Turn;
use super::media_type::{AudioShape, number, pair};

const APERTURE_BYTES: usize = 16;
const MIN_PIXEL_ASPECT: f64 = 0.25;
const MAX_PIXEL_ASPECT: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VideoTrack {
    pub(super) visible: [u32; 2],
    pub(super) turn: Turn,
    pub(super) frame_rate: Option<[u32; 2]>,
}

impl VideoTrack {
    pub(super) fn of(media_type: &IMFMediaType) -> Option<Self> {
        let coded = pair(media_type, &MF_MT_FRAME_SIZE)?;
        let stored =
            aperture(media_type).filter(|[width, height]| *width <= coded[0] && *height <= coded[1]).unwrap_or(coded);
        let visible = square_pixels(stored, pair(media_type, &MF_MT_PIXEL_ASPECT_RATIO));
        let turn = Turn::from_rotation_degrees(number(media_type, &MF_MT_VIDEO_ROTATION).unwrap_or(0));
        Some(Self { visible, turn, frame_rate: pair(media_type, &MF_MT_FRAME_RATE) })
    }

    pub(super) fn displayed(&self) -> [u32; 2] {
        self.turn.displayed(self.visible)
    }
}

pub(super) fn audio_shape(media_type: &IMFMediaType) -> Option<AudioShape> {
    let rate = number(media_type, &MF_MT_AUDIO_SAMPLES_PER_SECOND).filter(|rate| *rate > 0)?;
    let channels = number(media_type, &MF_MT_AUDIO_NUM_CHANNELS).filter(|channels| *channels > 0)?;
    Some(AudioShape { rate, channels })
}

fn square_pixels([width, height]: [u32; 2], pixel_aspect: Option<[u32; 2]>) -> [u32; 2] {
    let Some([across, down]) = pixel_aspect else { return [width, height] };
    let ratio = f64::from(across) / f64::from(down);
    if !(MIN_PIXEL_ASPECT..=MAX_PIXEL_ASPECT).contains(&ratio) {
        return [width, height];
    }
    [((f64::from(width) * ratio).round() as u32).max(1), height]
}

fn aperture(media_type: &IMFMediaType) -> Option<[u32; 2]> {
    let mut area = [0u8; APERTURE_BYTES];
    unsafe { media_type.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut area, None) }.ok()?;
    area_size(&area)
}

fn area_size(area: &[u8; APERTURE_BYTES]) -> Option<[u32; 2]> {
    let side = |at: usize| i32::from_le_bytes([area[at], area[at + 1], area[at + 2], area[at + 3]]);
    let [width, height] = [side(8), side(12)];
    (width > 0 && height > 0).then_some([width as u32, height as u32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aperture_gives_visible_size() {
        let mut area = [0u8; APERTURE_BYTES];
        area[8..12].copy_from_slice(&1920i32.to_le_bytes());
        area[12..16].copy_from_slice(&1080i32.to_le_bytes());
        assert_eq!(area_size(&area), Some([1920, 1080]));
        assert_eq!(area_size(&[0; APERTURE_BYTES]), None);
    }

    #[test]
    fn anamorphic_pixels_are_squared() {
        assert_eq!(square_pixels([1440, 1080], Some([4, 3])), [1920, 1080]);
        assert_eq!(square_pixels([1920, 1080], Some([1, 1])), [1920, 1080]);
        assert_eq!(square_pixels([1920, 1080], Some([100, 1])), [1920, 1080]);
        assert_eq!(square_pixels([720, 480], None), [720, 480]);
    }
}
