//! Every number that tunes the ranking, kept in one place. The tests pin the orderings that matter.

use super::context::{DAY, HOUR, MINUTE, Stamp};

pub const SHORTLIST: usize = 256;
pub const USAGE_SHORTLIST: usize = 256;

pub const TIER_EXACT: f32 = 1000.0;
pub const TIER_STEM: f32 = 940.0;
pub const TIER_PREFIX: f32 = 800.0;
pub const TIER_KIND: f32 = 700.0;
pub const TIER_WORD: f32 = 650.0;
pub const TIER_INSIDE: f32 = 480.0;
pub const TIER_FUZZY: f32 = 300.0;
pub const BOUNDARY: f32 = 16.0;
pub const RUN: f32 = 8.0;
pub const GAP: f32 = -3.0;
pub const GAP_FLOOR: f32 = -30.0;
pub const COVER: f32 = 60.0;
pub const SMART_CASE: f32 = 20.0;
pub const ANCESTOR: f32 = 0.6;

pub const APP: f32 = 60.0;
pub const APP_WORD: f32 = TIER_PREFIX - TIER_WORD;
pub const APP_ALIAS: f32 = -8.0;
pub const APP_BACKSTAGE: f32 = -120.0;
pub const APP_RUNNING: f32 = 25.0;
pub const APP_USED: f32 = 100.0;
pub const APP_USED_FILLS_AT: f32 = 30.0;
pub const APP_USED_LATELY: f32 = 90.0;
pub const APP_USED_LATELY_HALF_LIFE: Stamp = 5 * DAY;
pub const TYPO_FROM: usize = 4;
pub const FOLDER: f32 = 20.0;
pub const HOME_FOLDER: f32 = 30.0;
pub const DEPTH: f32 = -6.0;
pub const DEPTH_FREE: usize = 3;
pub const DEPTH_FLOOR: f32 = -60.0;
pub const HIDDEN: f32 = -150.0;
pub const TIER_PENALTY: [f32; 4] = [0.0, -200.0, -400.0, -400.0];
pub const FRESH_EDIT: f32 = 90.0;
pub const FRESH_EDIT_HALF_LIFE: Stamp = 7 * DAY;
pub const FRESH_NEW: f32 = 60.0;
pub const FRESH_NEW_HALF_LIFE: Stamp = 2 * DAY;

pub const HABIT_CAP: f32 = 450.0;
pub const OPENED: f32 = 220.0;
pub const OPENED_FILLS_AT: f32 = 3.0;
pub const OPEN_RECENT: f32 = 160.0;
pub const OPEN_RECENT_HALF_LIFE: Stamp = 3 * DAY;
pub const EDITED: f32 = 120.0;
pub const EDITED_FILLS_AT: f32 = 2.0;
pub const HEAT: f32 = 80.0;
pub const HEAT_GRANDPARENT: f32 = 40.0;
pub const SEQUENCE: f32 = 140.0;
pub const APP_HABIT: f32 = 90.0;
pub const DAYPART: f32 = 50.0;
pub const DAYPART_MIN_OPENS: f32 = 4.0;

pub const PREFIX: f32 = 600.0;
pub const TAUGHT: f32 = 60.0;
pub const PREFIX_FULL_WEIGHT: f32 = 2.0;
pub const PREFIX_EXTENDED: f32 = 0.7;
pub const PREFIX_BYTES: usize = 12;

pub const PICK_OPEN: f32 = 1.0;
pub const PICK_DRAG: f32 = 1.0;
pub const PICK_SHELVE: f32 = 0.8;
pub const PICK_REVEAL: f32 = 0.7;
pub const SEED_OPEN: f32 = 1.0;
pub const SEED_WITHIN: Stamp = 30 * DAY;
pub const SEED_MOST: usize = 1_500;
pub const SEED_AGAIN_AFTER: Stamp = 5 * MINUTE;
pub const EDIT_SESSION: Stamp = 5 * MINUTE;
pub const HEAT_PARENT: f32 = 1.0;
pub const HEAT_GRANDPARENT_SHARE: f32 = 0.5;
pub const SEQUENCE_WINDOW: Stamp = 20 * MINUTE;

pub const OPEN_HALF_LIFE: Stamp = 14 * DAY;
pub const EDIT_HALF_LIFE: Stamp = 2 * DAY;
pub const HEAT_HALF_LIFE: Stamp = 8 * HOUR;
pub const PAIR_HALF_LIFE: Stamp = 14 * DAY;
pub const PREFIX_HALF_LIFE: Stamp = 60 * DAY;

pub const FILES: usize = 4_096;
pub const FOLDERS: usize = 512;
pub const PREFIXES: usize = 2_048;
pub const PAIRS: usize = 1_024;
pub const APPS: usize = 64;
pub const PROBATION: usize = 1_024;
pub const EVICTION_OPEN_WORTH: f32 = 4.0;
pub const NEVER_EVICTED_WITHIN: Stamp = 7 * DAY;
pub const MOVE_WINDOW: Stamp = 2;
pub const GONE_KEPT: Stamp = 30 * DAY;
pub const PASSED_OVER: f32 = 0.5;
pub const SKIPS_OUT: u8 = 4;
pub const SKIP_FORGIVEN_EVERY: Stamp = 7 * DAY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn better_match_outranks_worse() {
        let tiers = [TIER_EXACT, TIER_STEM, TIER_PREFIX, TIER_KIND, TIER_WORD, TIER_INSIDE, TIER_FUZZY];
        assert!(tiers.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn app_word_counts_as_start() {
        let lifted = [TIER_WORD, TIER_INSIDE].map(|tier| tier + APP_WORD);
        assert_eq!(lifted[0], TIER_PREFIX);
        assert!(lifted[1] < TIER_WORD, "the inside of a word is never lifted, and would not reach a word if it were");
    }

    #[test]
    fn whole_name_beats_used_prefix() {
        let every_use: f32 = [APP_USED, APP_USED_LATELY].iter().sum();
        assert!(every_use < [TIER_EXACT, -TIER_PREFIX].iter().sum());
        let alias = [APP_ALIAS].map(f32::abs);
        assert!(alias[0] > 0.0 && alias[0] < BOUNDARY, "the shown name wins a tie, no more");
        let helper: f32 = [TIER_PREFIX, APP_BACKSTAGE].iter().sum();
        assert!(helper > TIER_WORD, "a helper sinks below its neighbours, not below files");
        assert!([TYPO_FROM].iter().all(|from| *from > 3), "three letters cannot tell a slip from another word");
    }

    #[test]
    fn junk_sinks_by_tier() {
        assert_eq!(TIER_PENALTY.first().copied(), Some(0.0));
        assert!(TIER_PENALTY.windows(2).all(|pair| pair[1] <= pair[0]));
    }

    #[test]
    fn habit_never_beats_exact() {
        let habits = [OPENED, OPEN_RECENT, EDITED, HEAT, HEAT_GRANDPARENT, SEQUENCE, APP_HABIT, DAYPART];
        let every_habit: f32 = habits.iter().sum();
        assert!(every_habit > HABIT_CAP, "the cap is what holds habit back");
        assert!(TIER_INSIDE + every_habit.min(HABIT_CAP) < TIER_EXACT);
        let remembered: f32 = [TIER_INSIDE, PREFIX].iter().sum();
        assert!(remembered > TIER_EXACT);
    }
}
