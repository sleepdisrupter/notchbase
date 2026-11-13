//! The block of words being typed: its text, place, size and colour. It can be dragged, and always keeps half an em
//! of itself on the picture so it can't be lost off the edge.

mod buffer;
mod placed;
mod scribe;
mod setter;
mod show;

use std::mem;

use egui::{Color32, Pos2, TextureHandle, Vec2, pos2, vec2};

use self::buffer::Buffer;
pub(super) use self::scribe::{Page, Scribe};
pub(super) use self::setter::Setter;
use crate::annotate::{Brush, Fit, Proof, Words};

const DRAG_STARTS_AFTER: f32 = 3.0;
const KEPT_ON_THE_PICTURE: f32 = 0.5;
const LINE_BREAK: &str = "\n";

pub(super) struct Typing {
    buffer: Buffer,
    origin: Pos2,
    em: f32,
    color: Color32,
    proof: Option<Proof>,
    stale: bool,
    texture: Option<TextureHandle>,
    unshown: bool,
    grab: Option<Grab>,
    replaces: Option<usize>,
}

struct Grab {
    from: Pos2,
    origin: Pos2,
    moved: bool,
}

impl Typing {
    fn open(at: Pos2, brush: Brush, fit: Fit, setter: &mut Setter) -> Self {
        let mut typing = Self {
            buffer: Buffer::default(),
            origin: at.round(),
            em: fit.em(brush.letter_size()),
            color: brush.color(),
            proof: None,
            stale: true,
            texture: None,
            unshown: false,
            grab: None,
            replaces: None,
        };
        let line = typing.proof(setter).map_or(0.0, |proof| proof.caret(0).height());
        typing.move_to(at - vec2(0.0, line / 2.0), setter.image());
        typing
    }

    pub(super) fn holds_words(&self) -> bool {
        !self.buffer.is_blank()
    }

    #[cfg(test)]
    pub(super) fn caret(&self) -> usize {
        self.buffer.caret()
    }

    pub(super) fn dye(&mut self, color: Color32) {
        self.stale |= color != self.color;
        self.color = color;
    }

    pub(super) fn size(&mut self, em: f32) {
        self.stale |= em != self.em;
        self.em = em;
    }

    fn proof(&mut self, setter: &mut Setter) -> Option<&Proof> {
        if mem::take(&mut self.stale) {
            self.set(setter);
            let kept = self.kept_on_the_picture(self.origin, setter.image());
            if kept != self.origin {
                self.origin = kept;
                self.set(setter);
            }
        }
        self.proof.as_ref()
    }

    fn set(&mut self, setter: &mut Setter) {
        let words = Words { text: self.buffer.text().to_owned(), origin: self.origin, em: self.em };
        if let Some(proof) = setter.set(&words, self.color) {
            self.proof = Some(proof);
            self.unshown = true;
        }
    }

    fn insert(&mut self, typed: &str) {
        self.stale |= self.buffer.insert(typed);
    }

    fn break_the_line(&mut self) {
        self.insert(LINE_BREAK);
    }

    fn erase_back(&mut self) {
        self.stale |= self.buffer.erase_back();
    }

    fn erase_forward(&mut self) {
        self.stale |= self.buffer.erase_forward();
    }

    fn jump(&mut self, setter: &mut Setter, to: fn(&Proof, usize) -> usize) {
        let caret = self.buffer.caret();
        let landed = self.proof(setter).map(|proof| to(proof, caret));
        self.buffer.place(landed.unwrap_or(caret));
    }

    fn is_under(&mut self, point: Pos2, fit: Fit, setter: &mut Setter) -> bool {
        self.proof(setter);
        self.outline_as_drawn(fit).is_some_and(|outline| outline.contains(point))
    }

    fn grab(&mut self, at: Pos2) {
        self.grab = Some(Grab { from: at, origin: self.origin, moved: false });
    }

    fn pull(&mut self, to: Pos2, fit: Fit, image: [usize; 2]) {
        let Some(grab) = self.grab.as_mut() else { return };
        grab.moved |= grab.from.distance(to) > DRAG_STARTS_AFTER;
        if grab.moved {
            let pulled = grab.origin + (to - grab.from) / fit.scale();
            self.move_to(pulled, image);
        }
    }

    fn let_go(&mut self, at: Pos2, fit: Fit, setter: &mut Setter) {
        let Some(grab) = self.grab.take() else { return };
        if !grab.moved {
            let pointed = self.proof(setter).map(|proof| proof.index_at(fit.to_image(at)));
            self.buffer.place(pointed.unwrap_or(self.buffer.caret()));
        }
    }

    fn move_to(&mut self, origin: Pos2, image: [usize; 2]) {
        let origin = self.kept_on_the_picture(origin.round(), image);
        self.stale |= origin != self.origin;
        self.origin = origin;
    }

    fn kept_on_the_picture(&self, origin: Pos2, [width, height]: [usize; 2]) -> Pos2 {
        let size = self.proof.as_ref().map_or(Vec2::ZERO, |proof| proof.lettering().frame().size());
        let image = vec2(width as f32, height as f32);
        let foothold = Vec2::splat(self.em * KEPT_ON_THE_PICTURE).min(image);
        let (least, most) = (foothold.min(size) - size, image - foothold);
        pos2(origin.x.max(least.x).min(most.x), origin.y.max(least.y).min(most.y)).round()
    }
}
