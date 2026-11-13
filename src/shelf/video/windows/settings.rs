//! Encoder settings: bitrates by quality and frame size, compact frames at most 1920 on the longest side, and audio
//! nudged to rates and channels the encoders accept.

use std::ops::RangeInclusive;

use super::geometry::{even, fit_within};
use super::media_type::AudioShape;
use crate::shelf::video::Quality;

const COMPACT_MAX_SIDE: u32 = 1920;
const FALLBACK_FRAME_RATE: [u32; 2] = [30, 1];
const BITRATE_FRAME_RATE_CAP: f64 = 60.0;
const AAC_RATES: [u32; 2] = [44_100, 48_000];
const AAC_CHANNELS: [u32; 3] = [1, 2, 6];
const MP3_RATES: [u32; 3] = [32_000, 44_100, 48_000];
pub(super) const MP3_BYTES_PER_SECOND: u32 = 32_000;

struct Budget {
    bits_per_pixel_per_frame: f64,
    bitrate: RangeInclusive<f64>,
    audio_bytes_per_second: u32,
}

fn budget(quality: Quality) -> Budget {
    match quality {
        Quality::Original => {
            Budget { bits_per_pixel_per_frame: 0.2, bitrate: 1.0e6..=80.0e6, audio_bytes_per_second: 24_000 }
        }
        Quality::Compact => {
            Budget { bits_per_pixel_per_frame: 0.07, bitrate: 0.25e6..=12.0e6, audio_bytes_per_second: 16_000 }
        }
    }
}

pub(super) fn frame_size(quality: Quality, visible: [u32; 2]) -> [u32; 2] {
    match quality {
        Quality::Original => even(visible),
        Quality::Compact => even(fit_within(visible, COMPACT_MAX_SIDE)),
    }
}

pub(super) fn frame_rate(reported: Option<[u32; 2]>) -> [u32; 2] {
    reported.filter(|[numerator, denominator]| *numerator > 0 && *denominator > 0).unwrap_or(FALLBACK_FRAME_RATE)
}

pub(super) fn video_bitrate(quality: Quality, [width, height]: [u32; 2], [numerator, denominator]: [u32; 2]) -> u32 {
    let budget = budget(quality);
    let fps = (f64::from(numerator) / f64::from(denominator.max(1))).clamp(1.0, BITRATE_FRAME_RATE_CAP);
    let bits = budget.bits_per_pixel_per_frame * f64::from(width) * f64::from(height) * fps;
    bits.clamp(*budget.bitrate.start(), *budget.bitrate.end()) as u32
}

pub(super) fn audio_bytes_per_second(quality: Quality) -> u32 {
    budget(quality).audio_bytes_per_second
}

pub(super) fn encodable_audio(source: AudioShape) -> AudioShape {
    let rate = if AAC_RATES.contains(&source.rate) { source.rate } else { AAC_RATES[1] };
    let channels = if AAC_CHANNELS.contains(&source.channels) { source.channels } else { 2 };
    AudioShape { rate, channels }
}

pub(super) fn mp3_audio(source: AudioShape) -> AudioShape {
    let rate = if MP3_RATES.contains(&source.rate) { source.rate } else { MP3_RATES[1] };
    AudioShape { rate, channels: source.channels.clamp(1, 2) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_1080p30_near_4_5_mbps() {
        let bitrate = video_bitrate(Quality::Compact, [1920, 1080], [30, 1]);
        assert!((4_300_000..=4_400_000).contains(&bitrate), "{bitrate}");
    }

    #[test]
    fn bitrates_stay_inside_sane_bounds() {
        assert_eq!(video_bitrate(Quality::Original, [7680, 4320], [240, 1]), 80_000_000);
        assert_eq!(video_bitrate(Quality::Compact, [160, 120], [15, 1]), 250_000);
        assert!(video_bitrate(Quality::Original, [1920, 1080], [30, 1]) > 12_000_000);
    }

    #[test]
    fn compact_frames_fit_1920() {
        assert_eq!(frame_size(Quality::Compact, [3840, 2160]), [1920, 1080]);
        assert_eq!(frame_size(Quality::Compact, [2160, 3840]), [1080, 1920]);
        assert_eq!(frame_size(Quality::Compact, [641, 361]), [640, 360]);
        assert_eq!(frame_size(Quality::Original, [3840, 2160]), [3840, 2160]);
    }

    #[test]
    fn audio_nudged_for_aac() {
        let odd = encodable_audio(AudioShape { rate: 22_050, channels: 3 });
        assert_eq!(odd, AudioShape { rate: 48_000, channels: 2 });
        let fine = AudioShape { rate: 44_100, channels: 6 };
        assert_eq!(encodable_audio(fine), fine);
        assert_eq!(frame_rate(Some([0, 1])), [30, 1]);
    }

    #[test]
    fn audio_nudged_for_mp3() {
        assert_eq!(mp3_audio(AudioShape { rate: 96_000, channels: 6 }), AudioShape { rate: 44_100, channels: 2 });
        let fine = AudioShape { rate: 48_000, channels: 1 };
        assert_eq!(mp3_audio(fine), fine);
    }
}
