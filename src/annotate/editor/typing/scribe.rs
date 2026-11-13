//! Applies input to the words: a press opens, grabs or finishes a block, and keys type, erase and move the caret.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};

use crate::annotate::{Brush, Fit, PixelRect, Proof, Sheet};

use super::super::{keys, pen};
use super::{Setter, Typing};

#[derive(Clone, Copy)]
pub(in crate::annotate::editor) struct Page {
    pub(in crate::annotate::editor) fit: Fit,
    pub(in crate::annotate::editor) ready: bool,
    pub(in crate::annotate::editor) toolbar: Rect,
}

pub(in crate::annotate::editor) struct Scribe<'a> {
    pub(in crate::annotate::editor) typing: &'a mut Option<Typing>,
    pub(in crate::annotate::editor) setter: &'a mut Setter,
    pub(in crate::annotate::editor) sheet: &'a mut Sheet,
    pub(in crate::annotate::editor) brush: Brush,
    pub(in crate::annotate::editor) page: Page,
}

impl Scribe<'_> {
    pub(in crate::annotate::editor) fn write(mut self, events: &[Event]) -> PixelRect {
        events.iter().fold(PixelRect::EMPTY, |redrawn, event| redrawn.union(self.read(event)))
    }

    fn read(&mut self, event: &Event) -> PixelRect {
        match event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } => self.press(*pos),
            Event::WindowFocused(false) => self.end(),
            Event::Key { key: Key::Backspace | Key::Delete, pressed: true, repeat: false, .. }
                if self.has_nothing_to_erase() =>
            {
                self.end()
            }
            _ => {
                self.edit(event);
                PixelRect::EMPTY
            }
        }
    }

    fn press(&mut self, at: Pos2) -> PixelRect {
        let Page { fit, ready, toolbar } = self.page;
        if !ready || toolbar.contains(at) {
            return PixelRect::EMPTY;
        }
        if let Some(typing) = self.typing.as_mut()
            && typing.is_under(at, fit, self.setter)
        {
            typing.grab(at);
            return PixelRect::EMPTY;
        }
        let ended = self.end();
        if pen::starts_at(fit, at) { ended.union(self.open(at)) } else { ended }
    }

    fn open(&mut self, at: Pos2) -> PixelRect {
        let (fit, pressed) = (self.page.fit, self.page.fit.to_image(at));
        let placed = self.sheet.words_at(pressed).and_then(|index| Some((index, self.sheet.words(index)?.clone())));
        let Some((index, lettering)) = placed else {
            *self.typing = Some(Typing::open(pressed, self.brush, fit, self.setter));
            return PixelRect::EMPTY;
        };
        let mut typing = Typing::reopen(index, &lettering, self.setter);
        typing.grab(at);
        *self.typing = Some(typing);
        self.sheet.lift(index).unwrap_or_default()
    }

    fn end(&mut self) -> PixelRect {
        self.typing.take().map_or(PixelRect::EMPTY, |typing| typing.place(self.setter, self.sheet))
    }

    fn has_nothing_to_erase(&self) -> bool {
        self.typing.as_ref().is_some_and(|typing| typing.buffer.is_empty())
    }

    fn edit(&mut self, event: &Event) {
        let (Some(typing), Page { fit, .. }) = (self.typing.as_mut(), self.page) else { return };
        match event {
            Event::PointerMoved(pos) => typing.pull(*pos, fit, self.setter.image()),
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } => {
                typing.let_go(*pos, fit, self.setter);
            }
            Event::Text(typed) | Event::Paste(typed) => typing.insert(typed),
            Event::Key { repeat: false, .. } if keys::breaks_the_line(event) => typing.break_the_line(),
            Event::Key { key, pressed: true, modifiers, .. } => steer(typing, *key, modifiers, self.setter),
            _ => {}
        }
    }
}

fn steer(typing: &mut Typing, key: Key, modifiers: &Modifiers, setter: &mut Setter) {
    match key {
        Key::Backspace => typing.erase_back(),
        Key::Delete => typing.erase_forward(),
        Key::ArrowLeft if modifiers.mac_cmd => typing.jump(setter, Proof::line_start),
        Key::ArrowRight if modifiers.mac_cmd => typing.jump(setter, Proof::line_end),
        Key::ArrowLeft => typing.buffer.left(),
        Key::ArrowRight => typing.buffer.right(),
        Key::ArrowUp => typing.jump(setter, Proof::index_above),
        Key::ArrowDown => typing.jump(setter, Proof::index_below),
        Key::Home => typing.jump(setter, Proof::line_start),
        Key::End => typing.jump(setter, Proof::line_end),
        _ => {}
    }
}
