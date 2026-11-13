//! The valve: what a rebuild may drop when memory is short, one more tier at each level. User files always stay.

use super::{EntryId, Room, tier};

pub const HARD_STOP: u8 = 5;
const KEEPS_EVERY_TIER: u8 = 4;
const FIRST_TIER_DROPPED: [u8; HARD_STOP as usize + 1] = [KEEPS_EVERY_TIER, 3, 2, 2, 1, 1];
const OTHER_VOLUMES_GO_AT: u8 = 3;

pub(super) struct Valve {
    level: u8,
    boot: Option<EntryId>,
}

impl Valve {
    pub fn of(room: &Room) -> Self {
        Self { level: room.level.min(HARD_STOP), boot: room.boot }
    }

    pub fn keeps(&self, flags: u8) -> bool {
        tier(flags) < FIRST_TIER_DROPPED[usize::from(self.level)]
    }

    pub fn opens(&self, root: EntryId) -> bool {
        self.level < OTHER_VOLUMES_GO_AT || self.boot.is_none_or(|boot| boot == root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::model::{flag, with_tier};

    fn valve(level: u8, boot: Option<EntryId>) -> Valve {
        Valve::of(&Room { cap: usize::MAX, level, boot })
    }

    fn kept_tiers(level: u8) -> Vec<u8> {
        (0..4).filter(|tier| valve(level, Some(0)).keeps(with_tier(flag::DIR | flag::HIDDEN, *tier))).collect()
    }

    #[test]
    fn each_level_drops_a_tier() {
        assert_eq!(kept_tiers(0), [0, 1, 2, 3]);
        assert_eq!(kept_tiers(1), [0, 1, 2], "caches go first");
        assert_eq!(kept_tiers(2), [0, 1], "then dependency and build folders");
        assert_eq!(kept_tiers(3), [0, 1], "other volumes go before the library");
        assert_eq!(kept_tiers(4), [0], "then the library");
        assert_eq!(kept_tiers(HARD_STOP), [0], "the hard stop evicts nothing more");
        assert_eq!(kept_tiers(u8::MAX), [0], "a level past the last is the last");
    }

    #[test]
    fn other_volumes_close_at_three() {
        for level in 0..OTHER_VOLUMES_GO_AT {
            assert!(valve(level, Some(0)).opens(0) && valve(level, Some(0)).opens(1), "level {level}");
        }
        for level in OTHER_VOLUMES_GO_AT..=HARD_STOP {
            assert!(valve(level, Some(0)).opens(0) && !valve(level, Some(0)).opens(1), "level {level}");
            assert!(valve(level, None).opens(1), "with no boot volume named, none counts as another volume");
        }
    }
}
