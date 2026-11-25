//! Ending a notice early, from its tool or from a click on it.

use egui::Pos2;

use notchbase_sdk::protocol::NoticeNode;

use crate::bridge::ToolSlot;

use super::super::Notch;
use super::super::activity::ActivityKind;

impl Notch {
    pub fn dismiss_notice(&mut self, slot: ToolSlot) {
        let owned = matches!(&self.activity.kind, Some(ActivityKind::Tool(notice)) if notice.slot == slot);
        if owned && self.activity.is_live() {
            self.end_notice(self.pointer_is_on_controls());
        }
    }

    /// Puts `node` in place of `slot`'s notice while it shows, without a new pop: it is measured again on the next
    /// frame and the notch springs to the new shape. Its time starts over.
    pub(in crate::notch) fn reshape_notice(&mut self, slot: ToolSlot, node: NoticeNode) -> bool {
        if !self.activity.is_live() {
            return false;
        }
        let Some(ActivityKind::Tool(notice)) = &mut self.activity.kind else { return false };
        if notice.slot != slot {
            return false;
        }
        *notice = super::ToolNotice { reshaped: true, ..super::ToolNotice::new(slot, node) };
        let duration = self.activity.kind.as_ref().map(ActivityKind::duration);
        self.activity.until = duration.map(|duration| std::time::Instant::now() + duration);
        true
    }

    pub(in crate::notch) fn dismiss_clicked_notice(&mut self, at: Option<Pos2>) {
        if self.rings() {
            self.activity.hushed = self.activity.kind.as_ref().and_then(|kind| match kind {
                ActivityKind::Tool(notice) => Some(notice.slot),
                _ => None,
            });
        }
        self.end_notice(self.hovers_controls(at));
    }

    fn end_notice(&mut self, pointer_is_on_it: bool) {
        self.end_activity();
        if pointer_is_on_it {
            self.wait_for_the_pointer_to_come_back();
        }
    }
}
