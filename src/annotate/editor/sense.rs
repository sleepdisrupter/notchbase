//! Reads a frame's input. Shortcuts and toolbar clicks become commands, and the rest goes to the pen or the open words.

use egui::{CursorIcon, Event, Id, Pos2, Sense};

use super::keys::{self, Busy};
use super::layout::Layout;
use super::pen::{self, Hand};
use super::tool::Tool;
use super::toolbar::Available;
use super::typing::{Page, Scribe, Setter};
use super::{Editor, Turn};
use crate::annotate::Fit;

const ID: &str = "notchbase.annotate";

impl Editor {
    pub(super) fn sense(&mut self, ui: &egui::Ui, layout: &Layout, fit: Fit, turn: &mut Turn) -> Option<Pos2> {
        let id = Id::new(ID);
        let (busy, ready) = (self.busy(), self.picture.is_drawable());
        let page = Page { fit, ready, toolbar: layout.toolbar };
        self.page = Some(page);
        let available = Available { undo: self.sheet.can_undo() || self.is_writing(), redo: self.sheet.can_redo() };
        if busy == Busy::Typing {
            self.write(&keys::typed_before_any_command(ui), ui.ctx(), page, turn);
        }
        let mut commands = keys::commands(ui, busy);
        commands.extend(self.toolbar.sense(ui, layout, id, &available));
        commands.into_iter().for_each(|command| self.perform(command, turn));
        let pointer = ui.input(|input| input.pointer.latest_pos());
        let hovered = ui.interact(fit.rect(), id.with("picture"), Sense::DRAG).hovered();
        let over_the_picture = ready && hovered && pointer.is_some_and(|at| pen::starts_at(fit, at));
        match self.tool {
            Tool::Brush => {
                self.draw(ui, fit, ready, turn);
                let pointer = pointer.filter(|_| over_the_picture || self.pen.is_some())?;
                ui.ctx().set_cursor_icon(CursorIcon::None);
                Some(pointer)
            }
            Tool::Text => {
                self.write(&ui.input(|input| input.events.clone()), ui.ctx(), page, turn);
                self.show_the_words(ui.ctx());
                if over_the_picture {
                    ui.ctx().set_cursor_icon(CursorIcon::Text);
                }
                None
            }
        }
    }

    fn draw(&mut self, ui: &egui::Ui, fit: Fit, ready: bool, turn: &mut Turn) {
        let hand = Hand { pen: &mut self.pen, brush: self.brush, fit, ready };
        for stroke in hand.trace(ui) {
            turn.dirty = turn.dirty.union(self.sheet.commit(stroke));
        }
    }

    pub(super) fn read_what_no_frame_has(&mut self, unread: &mut Vec<Event>, turn: &mut Turn) {
        let (Some(page), true) = (self.page, self.typing.is_some()) else { return };
        self.read(&keys::typed_and_not_yet_read(unread), page, turn);
    }

    fn write(&mut self, events: &[Event], ctx: &egui::Context, page: Page, turn: &mut Turn) {
        let image = self.sheet.size();
        self.setter.get_or_insert_with(|| Setter::new(ctx, image));
        self.read(events, page, turn);
    }

    fn read(&mut self, events: &[Event], page: Page, turn: &mut Turn) {
        let Some(setter) = self.setter.as_mut() else { return };
        let scribe = Scribe { typing: &mut self.typing, setter, sheet: &mut self.sheet, brush: self.brush, page };
        turn.dirty = turn.dirty.union(scribe.write(events));
    }

    fn show_the_words(&mut self, ctx: &egui::Context) {
        if let (Some(typing), Some(setter)) = (self.typing.as_mut(), self.setter.as_mut()) {
            typing.show(ctx, setter);
        }
    }
}
