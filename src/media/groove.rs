//! A made-up 118 BPM groove of kicks, hi-hats and slow sways, so the notch still dances when the output can't be heard.

use std::f32::consts::TAU;

use notchbase_spectrum::Bands;

const BEAT_SECONDS: f32 = 60.0 / 118.0;
const KICK_DECAY: f32 = 7.0;
const HAT_DECAY: f32 = 14.0;

pub fn groove(seconds: f32) -> Bands {
    let beat = (seconds / BEAT_SECONDS).fract();
    let offbeat = (beat + 0.5).fract();
    let kick = (-beat * KICK_DECAY).exp();
    let hat = (-offbeat * HAT_DECAY).exp();
    let sway = |rate: f32, phase: f32| 0.5 + 0.5 * (seconds * TAU * rate + phase).sin();
    let bands: Bands = [
        0.25 + 0.7 * kick,
        0.2 + 0.45 * kick * sway(0.23, 0.0),
        0.3 + 0.35 * sway(0.31, 1.3),
        0.25 + 0.4 * sway(0.17, 2.1) * (0.6 + 0.4 * hat),
        0.15 + 0.6 * hat,
        0.1 + 0.5 * hat * sway(0.41, 0.7),
    ];
    bands.map(|band| band.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use notchbase_spectrum::BANDS;

    use super::*;

    #[test]
    fn groove_pulses_and_stays_in_range() {
        let on = groove(0.0);
        let between = groove(BEAT_SECONDS * 0.45);
        assert!(on[0] > between[0] + 0.4, "{on:?} {between:?}");
        for i in 0..200 {
            let bands = groove(i as f32 * 0.037);
            assert!(bands.iter().all(|b| (0.0..=1.0).contains(b)));
            assert_eq!(bands.len(), BANDS);
        }
    }
}
