//! Turns and sizes: a track's rotation, its displayed sides, and fitting within a longest side at even sizes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Turn {
    Upright,
    Clockwise,
    Half,
    Counterclockwise,
}

impl Turn {
    pub(super) fn from_rotation_degrees(degrees: u32) -> Self {
        match degrees % 360 {
            90 => Self::Clockwise,
            180 => Self::Half,
            270 => Self::Counterclockwise,
            _ => Self::Upright,
        }
    }

    pub(super) fn rotation_degrees(self) -> u32 {
        match self {
            Self::Upright => 0,
            Self::Clockwise => 90,
            Self::Half => 180,
            Self::Counterclockwise => 270,
        }
    }

    pub(super) fn displayed(self, [width, height]: [u32; 2]) -> [u32; 2] {
        match self {
            Self::Clockwise | Self::Counterclockwise => [height, width],
            Self::Upright | Self::Half => [width, height],
        }
    }
}

pub(super) fn fit_within([width, height]: [u32; 2], max_side: u32) -> [u32; 2] {
    let longest = width.max(height);
    if longest <= max_side || longest == 0 {
        return [width, height];
    }
    let scale = f64::from(max_side) / f64::from(longest);
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).clamp(1, max_side);
    [scaled(width), scaled(height)]
}

pub(super) fn even([width, height]: [u32; 2]) -> [u32; 2] {
    [(width & !1).max(2), (height & !1).max(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sideways_turns_swap_sides() {
        assert_eq!(Turn::from_rotation_degrees(90).displayed([1920, 1080]), [1080, 1920]);
        assert_eq!(Turn::from_rotation_degrees(270).displayed([1920, 1080]), [1080, 1920]);
        assert_eq!(Turn::from_rotation_degrees(180).displayed([1920, 1080]), [1920, 1080]);
        assert_eq!(Turn::from_rotation_degrees(45), Turn::Upright);
        assert_eq!(Turn::Counterclockwise.rotation_degrees(), 270);
    }

    #[test]
    fn fitting_never_enlarges() {
        assert_eq!(fit_within([3840, 2160], 1920), [1920, 1080]);
        assert_eq!(fit_within([1080, 1920], 480), [270, 480]);
        assert_eq!(fit_within([640, 360], 1920), [640, 360]);
        assert_eq!(fit_within([4000, 1], 400), [400, 1]);
    }

    #[test]
    fn sizes_round_down_to_even() {
        assert_eq!(even([1919, 1081]), [1918, 1080]);
        assert_eq!(even([1, 1]), [2, 2]);
    }
}
