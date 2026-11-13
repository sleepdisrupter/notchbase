//! Spots a lone press of the Windows key. Its press is held back, so pressing and letting go alone opens Find, and when
//! another key comes first the held key is replayed so the shortcut still works.

pub const LEFT_WIN: u32 = 0x5B;
pub const RIGHT_WIN: u32 = 0x5C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub code: u32,
    pub down: bool,
    pub ours: bool,
    pub modified: bool,
    pub system_holds: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
    Chord(u32),
    Summon,
}

#[derive(Debug, Default)]
pub struct Lone {
    held: Option<u32>,
    shared: bool,
}

impl Lone {
    pub fn see(&mut self, stroke: Stroke) -> Verdict {
        if stroke.ours {
            return Verdict::Pass;
        }
        let win = matches!(stroke.code, LEFT_WIN | RIGHT_WIN);
        if self.shared && self.held == Some(stroke.code) && stroke.down && !stroke.system_holds {
            self.held = None;
        }
        match (win, stroke.down, self.held) {
            (true, true, None) => {
                (self.held, self.shared) = (Some(stroke.code), stroke.modified);
                if self.shared { Verdict::Pass } else { Verdict::Swallow }
            }
            (true, true, Some(held)) if held == stroke.code => self.as_the_system_knows(),
            (true, false, Some(held)) if held == stroke.code => {
                self.held = None;
                if self.shared { Verdict::Pass } else { Verdict::Summon }
            }
            (_, true, Some(held)) if !self.shared => {
                self.shared = true;
                Verdict::Chord(held)
            }
            _ => Verdict::Pass,
        }
    }

    fn as_the_system_knows(&self) -> Verdict {
        if self.shared { Verdict::Pass } else { Verdict::Swallow }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER_E: u32 = 0x45;
    const LETTER_S: u32 = 0x53;
    const SHIFT: u32 = 0x10;

    fn down(code: u32) -> Stroke {
        Stroke { code, down: true, ours: false, modified: false, system_holds: false }
    }

    fn up(code: u32) -> Stroke {
        Stroke { down: false, ..down(code) }
    }

    fn seen(lone: &mut Lone, strokes: &[Stroke]) -> Vec<Verdict> {
        strokes.iter().map(|stroke| lone.see(*stroke)).collect()
    }

    #[test]
    fn lone_press_summons() {
        let mut lone = Lone::default();
        assert_eq!(seen(&mut lone, &[down(LEFT_WIN), up(LEFT_WIN)]), [Verdict::Swallow, Verdict::Summon]);
        assert_eq!(seen(&mut lone, &[down(RIGHT_WIN), up(RIGHT_WIN)]), [Verdict::Swallow, Verdict::Summon]);
        let held = [down(LEFT_WIN), down(LEFT_WIN), down(LEFT_WIN), up(LEFT_WIN)];
        let all = [Verdict::Swallow, Verdict::Swallow, Verdict::Swallow, Verdict::Summon];
        assert_eq!(seen(&mut lone, &held), all, "held a while: its repeats are kept back too");
    }

    #[test]
    fn chord_replays_the_windows_key() {
        let mut lone = Lone::default();
        let explorer = [down(LEFT_WIN), down(LETTER_E), down(LETTER_E), up(LETTER_E), up(LEFT_WIN)];
        let chord = [Verdict::Swallow, Verdict::Chord(LEFT_WIN), Verdict::Pass, Verdict::Pass, Verdict::Pass];
        assert_eq!(seen(&mut lone, &explorer), chord, "the system sees Windows, then E, then both let go");
        let snip = [down(RIGHT_WIN), down(SHIFT), down(LETTER_S), up(LETTER_S), up(SHIFT), up(RIGHT_WIN)];
        let seen_snip = seen(&mut lone, &snip);
        assert_eq!(seen_snip[..3], [Verdict::Swallow, Verdict::Chord(RIGHT_WIN), Verdict::Pass]);
        assert!(seen_snip[3..].iter().all(|verdict| *verdict == Verdict::Pass));
        let both = [down(LEFT_WIN), down(RIGHT_WIN), up(RIGHT_WIN), up(LEFT_WIN)];
        assert_eq!(seen(&mut lone, &both)[1], Verdict::Chord(LEFT_WIN), "the other Windows key is a key beside it");
        assert_eq!(seen(&mut lone, &[down(LEFT_WIN), up(LEFT_WIN)]), [Verdict::Swallow, Verdict::Summon]);
    }

    #[test]
    fn modified_press_is_the_systems() {
        let mut lone = Lone::default();
        let with_control = Stroke { modified: true, ..down(LEFT_WIN) };
        let strokes = [with_control, Stroke { system_holds: true, ..with_control }, up(LEFT_WIN)];
        assert_eq!(seen(&mut lone, &strokes), [Verdict::Pass; 3], "a shortcut already under way is left alone");
        assert_eq!(seen(&mut lone, &[down(LEFT_WIN), up(LEFT_WIN)]), [Verdict::Swallow, Verdict::Summon]);
    }

    #[test]
    fn missed_release_is_forgotten() {
        let mut lone = Lone::default();
        let locked = [down(LEFT_WIN), down(0x4C)];
        assert_eq!(seen(&mut lone, &locked), [Verdict::Swallow, Verdict::Chord(LEFT_WIN)], "Windows+L locks");
        let after = [down(LEFT_WIN), up(LEFT_WIN)];
        assert_eq!(seen(&mut lone, &after), [Verdict::Swallow, Verdict::Summon], "the system no longer holds the key");
        let still_down = Stroke { system_holds: true, ..down(LEFT_WIN) };
        lone.see(down(LEFT_WIN));
        lone.see(down(LETTER_E));
        assert_eq!(lone.see(still_down), Verdict::Pass, "a repeat of a key the system does hold is only a repeat");
    }

    #[test]
    fn own_and_other_strokes_pass() {
        let mut lone = Lone::default();
        let replayed = Stroke { ours: true, ..down(LEFT_WIN) };
        assert_eq!(seen(&mut lone, &[replayed, Stroke { ours: true, ..down(LETTER_E) }]), [Verdict::Pass; 2]);
        assert_eq!(seen(&mut lone, &[down(LETTER_E), up(LETTER_E), up(LEFT_WIN)]), [Verdict::Pass; 3]);
        let between = [down(LEFT_WIN), up(LETTER_E), up(LEFT_WIN)];
        assert_eq!(seen(&mut lone, &between)[1..], [Verdict::Pass, Verdict::Summon], "a key let go is not a press");
    }
}
