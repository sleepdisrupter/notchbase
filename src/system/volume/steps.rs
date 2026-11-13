//! What a key press does: a step up or down that also unmutes, smaller with Option and Shift held, or a mute toggle.

use super::{BrightnessPress, KeyPress, Level, Press};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps {
    pub coarse: f32,
    pub fine: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Change {
    pub volume: Option<f32>,
    pub muted: Option<bool>,
}

pub fn apply(key: KeyPress, level: Level, steps: Steps) -> Change {
    let step = |direction: f32| nudge(level.volume, direction, key.fine, steps);
    let unmute = level.muted.then_some(false);
    match key.press {
        Press::Up => Change { volume: Some(step(1.0)), muted: unmute },
        Press::Down => Change { volume: Some(step(-1.0)), muted: unmute },
        Press::Mute => Change { volume: None, muted: Some(!level.muted) },
    }
}

pub fn after(level: Level, change: Change) -> Level {
    Level { volume: change.volume.unwrap_or(level.volume), muted: change.muted.unwrap_or(level.muted) }
}

pub fn brighten(key: BrightnessPress, level: f32, steps: Steps) -> f32 {
    nudge(level, if key.up { 1.0 } else { -1.0 }, key.fine, steps)
}

fn nudge(level: f32, direction: f32, fine: bool, steps: Steps) -> f32 {
    let count = if fine { steps.fine } else { steps.coarse };
    (((level * count).round() + direction) / count).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Steps = Steps { coarse: 50.0, fine: 50.0 };
    const MAC: Steps = Steps { coarse: 16.0, fine: 64.0 };

    fn press(press: Press) -> KeyPress {
        KeyPress { press, fine: false }
    }

    fn level(volume: f32, muted: bool) -> Level {
        Level { volume, muted }
    }

    #[test]
    fn windows_steps_by_two_percent() {
        let up = apply(press(Press::Up), level(0.5, false), WINDOWS);
        assert_eq!(up, Change { volume: Some(0.52), muted: None });
        let odd = apply(press(Press::Down), level(0.513, false), WINDOWS);
        assert_eq!(odd.volume, Some(0.5));
        assert_eq!(apply(press(Press::Up), level(1.0, false), WINDOWS).volume, Some(1.0));
        assert_eq!(apply(press(Press::Down), level(0.0, false), WINDOWS).volume, Some(0.0));
    }

    #[test]
    fn mac_steps_by_sixteenths() {
        assert_eq!(apply(press(Press::Up), level(0.5, false), MAC).volume, Some(9.0 / 16.0));
        let fine = KeyPress { press: Press::Up, fine: true };
        assert_eq!(apply(fine, level(0.5, false), MAC).volume, Some(33.0 / 64.0));
    }

    #[test]
    fn stepping_unmutes_mute_toggles() {
        let up = apply(press(Press::Up), level(0.3, true), WINDOWS);
        assert_eq!(up.muted, Some(false));
        assert_eq!(apply(press(Press::Mute), level(0.3, false), WINDOWS), Change { volume: None, muted: Some(true) });
        assert_eq!(after(level(0.3, true), up), level(0.32, false));
    }

    #[test]
    fn brightness_stops_at_the_ends() {
        let (up, down) = (BrightnessPress { up: true, fine: false }, BrightnessPress { up: false, fine: false });
        assert_eq!(brighten(up, 0.5, MAC), 9.0 / 16.0);
        assert_eq!(brighten(down, 0.51, MAC), 7.0 / 16.0);
        assert_eq!(brighten(BrightnessPress { up: false, fine: true }, 0.5, MAC), 31.0 / 64.0);
        assert_eq!((brighten(up, 1.0, MAC), brighten(down, 0.0, MAC)), (1.0, 0.0));
    }
}
