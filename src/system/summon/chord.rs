//! Spots Command-Space in the stream of key events, swallowing the keys that belong to it and passing the rest.

pub const SPACE: i64 = 49;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub code: i64,
    pub down: bool,
    pub repeat: bool,
    pub mods: Mods,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
    Summon,
}

#[derive(Debug, Default)]
pub struct Chord {
    held: bool,
}

impl Chord {
    pub fn see(&mut self, stroke: Stroke) -> Verdict {
        let only_command = stroke.mods == Mods { command: true, ..Mods::default() };
        match (stroke.code == SPACE, stroke.down) {
            (true, true) if only_command => {
                let first = !stroke.repeat && !std::mem::replace(&mut self.held, true);
                if first { Verdict::Summon } else { Verdict::Swallow }
            }
            (true, false) if std::mem::take(&mut self.held) => Verdict::Swallow,
            _ => Verdict::Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: Mods = Mods { command: true, shift: false, option: false, control: false };
    const LETTER_A: i64 = 0;

    fn down(code: i64, mods: Mods) -> Stroke {
        Stroke { code, down: true, repeat: false, mods }
    }

    fn up(code: i64, mods: Mods) -> Stroke {
        Stroke { code, down: false, repeat: false, mods }
    }

    #[test]
    fn command_space_summons_once() {
        let mut chord = Chord::default();
        assert_eq!(chord.see(down(SPACE, COMMAND)), Verdict::Summon);
        assert_eq!(chord.see(Stroke { repeat: true, ..down(SPACE, COMMAND) }), Verdict::Swallow, "held down");
        assert_eq!(chord.see(down(SPACE, COMMAND)), Verdict::Swallow, "a second press without a release");
        assert_eq!(chord.see(up(SPACE, Mods::default())), Verdict::Swallow, "command may be let go first");
        assert_eq!(chord.see(up(SPACE, Mods::default())), Verdict::Pass, "the release was already taken");
        assert_eq!(chord.see(down(SPACE, COMMAND)), Verdict::Summon, "and the next press summons again");
    }

    #[test]
    fn every_other_stroke_passes_untouched() {
        let mut chord = Chord::default();
        assert_eq!(chord.see(down(SPACE, Mods::default())), Verdict::Pass, "a plain space is typed");
        assert_eq!(chord.see(up(SPACE, Mods::default())), Verdict::Pass);
        for mods in [
            Mods { shift: true, ..COMMAND },
            Mods { option: true, ..COMMAND },
            Mods { control: true, ..COMMAND },
            Mods { control: true, ..Mods::default() },
            Mods { option: true, ..Mods::default() },
        ] {
            assert_eq!(chord.see(down(SPACE, mods)), Verdict::Pass, "{mods:?} belongs to someone else");
            assert_eq!(chord.see(up(SPACE, mods)), Verdict::Pass);
        }
        assert_eq!(chord.see(down(LETTER_A, COMMAND)), Verdict::Pass);
        assert_eq!(chord.see(up(LETTER_A, COMMAND)), Verdict::Pass);
        assert_eq!(chord.see(Stroke { repeat: true, ..down(SPACE, COMMAND) }), Verdict::Swallow, "never seen pressed");
    }
}
