//! Placing a finished block on the sheet and reopening a placed one. An edit counts as a change only when the words,
//! place, size or colour differ from what was placed.

use super::buffer::Buffer;
use super::{Setter, Typing};
use crate::annotate::{Lettering, PixelRect, Sheet};

impl Typing {
    pub(super) fn reopen(at: usize, placed: &Lettering, setter: &mut Setter) -> Self {
        let words = placed.words();
        let mut buffer = Buffer::default();
        buffer.insert(&words.text);
        let mut typing = Self {
            buffer,
            origin: words.origin,
            em: words.em,
            color: placed.color(),
            proof: None,
            stale: true,
            texture: None,
            unshown: false,
            grab: None,
            replaces: Some(at),
        };
        typing.proof(setter);
        typing
    }

    pub(in crate::annotate::editor) fn place(self, setter: &mut Setter, sheet: &mut Sheet) -> PixelRect {
        let (replaces, emptied) = (self.replaces, !self.holds_words());
        let lettering = self.ended(setter);
        setter.rest();
        match (replaces, lettering) {
            (None, Some(lettering)) => sheet.commit(lettering),
            (None, None) => PixelRect::EMPTY,
            (Some(at), None) if emptied => exchange(sheet, at, Lettering::empty()),
            (Some(at), Some(lettering)) if sheet.words(at) != Some(&lettering) => exchange(sheet, at, lettering),
            (Some(_), _) => sheet.settle(),
        }
    }

    pub(in crate::annotate::editor) fn would_change(&self, sheet: &Sheet) -> bool {
        match self.replaces.and_then(|at| sheet.words(at)) {
            Some(placed) => !self.reads_as(placed),
            None => self.holds_words(),
        }
    }

    #[cfg(test)]
    pub(in crate::annotate::editor) fn replaces(&self) -> Option<usize> {
        self.replaces
    }

    #[cfg(test)]
    pub(in crate::annotate::editor) fn typeset(&self) -> Option<&Lettering> {
        self.proof.as_ref().map(|proof| proof.lettering())
    }

    fn ended(mut self, setter: &mut Setter) -> Option<Lettering> {
        self.stale |= self.buffer.trim_the_end();
        self.proof(setter);
        let lettering = self.proof.take()?.into_lettering();
        (!lettering.words().is_blank()).then_some(lettering)
    }

    fn reads_as(&self, placed: &Lettering) -> bool {
        let words = placed.words();
        let written = (self.buffer.text().trim_end(), self.origin, self.em, self.color);
        written == (words.text.as_str(), words.origin, words.em, placed.color())
    }
}

fn exchange(sheet: &mut Sheet, at: usize, lettering: Lettering) -> PixelRect {
    sheet.replace(at, lettering).unwrap_or_else(|| sheet.settle())
}
