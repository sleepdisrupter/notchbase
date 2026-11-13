//! Commit, replace, undo and redo. A step remembers the mark it replaced, and each one moves the revision on.

use std::mem;

use super::Sheet;
use crate::annotate::mark::Mark;
use crate::annotate::rect::PixelRect;

pub(super) struct Step {
    at: usize,
    mark: Option<Mark>,
}

impl Sheet {
    pub fn commit(&mut self, mark: impl Into<Mark>) -> PixelRect {
        let (back, inked) = self.turn(Step { at: self.marks.len(), mark: Some(mark.into()) });
        self.remember(back);
        inked
    }

    pub fn replace(&mut self, at: usize, mark: impl Into<Mark>) -> Option<PixelRect> {
        if at >= self.marks.len() {
            return None;
        }
        let (back, redrawn) = self.turn(Step { at, mark: Some(mark.into()) });
        self.remember(back);
        Some(redrawn)
    }

    pub fn undo(&mut self) -> Option<PixelRect> {
        let step = self.done.pop()?;
        let (forward, redrawn) = self.turn(step);
        self.undone.push(forward);
        self.revision += 1;
        Some(redrawn)
    }

    pub fn redo(&mut self) -> Option<PixelRect> {
        let step = self.undone.pop()?;
        let (back, redrawn) = self.turn(step);
        self.done.push(back);
        self.revision += 1;
        Some(redrawn)
    }

    fn remember(&mut self, back: Step) {
        self.undone.clear();
        self.done.push(back);
        self.revision += 1;
    }

    fn turn(&mut self, Step { at, mark }: Step) -> (Step, PixelRect) {
        let settled = self.settle_all_but(at);
        let was = match mark {
            Some(mark) if at < self.marks.len() => Some(mem::replace(&mut self.marks[at], mark)),
            Some(mark) => {
                self.marks.push(mark);
                None
            }
            None => self.marks.pop(),
        };
        let redrawn = match &was {
            Some(was) => self.repaint(was.bounds().union(self.bounds_at(at))),
            None => self.ink_latest(),
        };
        (Step { at, mark: was }, settled.union(redrawn))
    }

    fn settle_all_but(&mut self, at: usize) -> PixelRect {
        if self.lifted == Some(at) {
            self.lifted = None;
            return PixelRect::EMPTY;
        }
        self.settle()
    }
}
