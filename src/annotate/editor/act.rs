//! Carries out a command: picking a colour or size, switching tools, undo and redo, and finishing a line or block.

use super::command::Command;
use super::tool::Tool;
use super::{Editor, Outcome, Turn};
use crate::annotate::{Brush, Fit};
use crate::platform::{self, Feel};

impl Editor {
    pub(super) fn perform(&mut self, command: Command, turn: &mut Turn) {
        match command {
            Command::Pick(index) => self.pick(index, turn),
            Command::Resize(size) => self.resize(size, turn.fit),
            Command::Settle => self.remember(turn),
            Command::Nudge(by) => {
                self.resize(self.brush.size() + by, turn.fit);
                self.remember(turn);
            }
            Command::Use(tool) => self.take_up(tool, turn),
            Command::Undo => self.rewind(Step::Back, turn),
            Command::Redo => self.rewind(Step::Forward, turn),
            Command::Cancel => self.pen = None,
            Command::Finish => self.finish_the_words(turn),
            Command::Close => {
                self.finish_the_line(turn);
                self.finish_the_words(turn);
                turn.outcomes.push(Outcome::Close);
            }
        }
    }

    pub(super) fn finish_the_line(&mut self, turn: &mut Turn) {
        if let Some(stroke) = self.pen.take().and_then(|pen| pen.stroke()) {
            turn.dirty = turn.dirty.union(self.sheet.commit(stroke));
        }
    }

    pub(super) fn finish_the_words(&mut self, turn: &mut Turn) {
        if let (Some(typing), Some(setter)) = (self.typing.take(), self.setter.as_mut()) {
            turn.dirty = turn.dirty.union(typing.place(setter, &mut self.sheet));
        }
    }

    fn take_up(&mut self, tool: Tool, turn: &mut Turn) {
        if tool == self.tool {
            return;
        }
        self.finish_the_line(turn);
        self.finish_the_words(turn);
        self.tool = tool;
        platform::haptic(Feel::Tick);
    }

    fn pick(&mut self, index: usize, turn: &mut Turn) {
        let Some(color) = Brush::PALETTE.get(index) else { return };
        let picked = self.brush.with_color(*color);
        if picked != self.brush {
            self.brush = picked;
            platform::haptic(Feel::Tick);
        }
        if let Some(typing) = self.typing.as_mut() {
            typing.dye(picked.color());
        }
        self.remember(turn);
    }

    fn resize(&mut self, size: f32, fit: Option<Fit>) {
        let sized = self.brush.with_size(size);
        let reached_an_end = sized.size() == Brush::THINNEST || sized.size() == Brush::THICKEST;
        if sized != self.brush && reached_an_end {
            platform::haptic(Feel::Tick);
        }
        self.brush = sized;
        if let (Some(typing), Some(fit)) = (self.typing.as_mut(), fit) {
            typing.size(fit.em(sized.letter_size()));
        }
    }

    fn remember(&mut self, turn: &mut Turn) {
        if self.brush != self.remembered {
            self.remembered = self.brush;
            turn.outcomes.push(Outcome::Remember(self.brush));
        }
    }

    fn rewind(&mut self, step: Step, turn: &mut Turn) {
        self.finish_the_words(turn);
        let redrawn = match step {
            Step::Back => self.sheet.undo(),
            Step::Forward => self.sheet.redo(),
        };
        if let Some(dirty) = redrawn {
            turn.dirty = turn.dirty.union(dirty);
            platform::haptic(Feel::Snap);
        }
    }
}

enum Step {
    Back,
    Forward,
}
