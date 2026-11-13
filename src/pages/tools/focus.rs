//! Which tool's card shows. Lingering on an icon chooses it, passing over it doesn't.

use crate::bridge::ToolSlot;

const DWELL: f32 = 0.14;
const LINGER: f32 = 0.45;

#[derive(Default)]
pub(super) struct Focus {
    chosen: Option<ToolSlot>,
    hovering: Option<ToolSlot>,
    dwell: f32,
}

impl Focus {
    pub(super) fn follow(&mut self, hovered: Option<ToolSlot>, dt: f32, locked: bool) {
        let candidate = hovered.filter(|slot| !locked && Some(*slot) != self.chosen);
        if candidate != self.hovering {
            self.hovering = candidate;
            self.dwell = 0.0;
            return;
        }
        let Some(slot) = candidate else { return };
        self.dwell += dt;
        if self.dwell >= if self.chosen.is_some() { DWELL } else { LINGER } {
            self.choose(slot);
        }
    }

    pub(super) fn choose(&mut self, slot: ToolSlot) {
        self.chosen = Some(slot);
        self.hovering = None;
        self.dwell = 0.0;
    }

    pub(super) fn switch(&mut self, slot: ToolSlot) {
        if self.chosen.is_some() {
            self.choose(slot);
        }
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn waiting(&self) -> bool {
        self.hovering.is_some()
    }

    pub(super) fn resolve(&self, shown: &[ToolSlot], rebinding: Option<ToolSlot>) -> Option<ToolSlot> {
        rebinding.or(self.chosen).filter(|slot| shown.contains(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    const SHOWN: [ToolSlot; 3] = [ToolSlot(1), ToolSlot(2), ToolSlot(3)];

    fn hover(focus: &mut Focus, slot: ToolSlot, seconds: f32, locked: bool) {
        for _ in 0..(seconds / FRAME).ceil() as usize {
            focus.follow(Some(slot), FRAME, locked);
        }
    }

    #[test]
    fn nothing_until_chosen() {
        let mut focus = Focus::default();
        assert_eq!(focus.resolve(&SHOWN, None), None);
        assert_eq!(focus.resolve(&SHOWN, Some(ToolSlot(3))), Some(ToolSlot(3)), "a tool being rebound shows");
        focus.choose(ToolSlot(2));
        assert_eq!(focus.resolve(&SHOWN, None), Some(ToolSlot(2)));
        focus.clear();
        assert_eq!(focus.resolve(&SHOWN, None), None);
    }

    #[test]
    fn lingering_chooses_passing_does_not() {
        let mut focus = Focus::default();
        hover(&mut focus, ToolSlot(2), LINGER * 0.5, false);
        assert!(focus.waiting());
        focus.follow(Some(ToolSlot(3)), FRAME, false);
        focus.follow(None, FRAME, false);
        assert_eq!(focus.resolve(&SHOWN, None), None, "sweeping across icons changes nothing");
        hover(&mut focus, ToolSlot(2), LINGER + 0.1, false);
        assert_eq!(focus.resolve(&SHOWN, None), Some(ToolSlot(2)));
        assert!(!focus.waiting());
    }

    #[test]
    fn next_tool_needs_shorter_rest() {
        let mut focus = Focus::default();
        hover(&mut focus, ToolSlot(1), DWELL * 1.5, false);
        assert_eq!(focus.resolve(&SHOWN, None), None, "the first one has to be lingered on");
        focus.choose(ToolSlot(1));
        hover(&mut focus, ToolSlot(2), DWELL * 1.5, false);
        assert_eq!(focus.resolve(&SHOWN, None), Some(ToolSlot(2)));
    }

    #[test]
    fn click_switches_shown_card() {
        let mut focus = Focus::default();
        focus.switch(ToolSlot(2));
        assert_eq!(focus.resolve(&SHOWN, None), None);
        focus.choose(ToolSlot(1));
        focus.switch(ToolSlot(2));
        assert_eq!(focus.resolve(&SHOWN, None), Some(ToolSlot(2)));
    }

    #[test]
    fn locked_card_keeps_tool() {
        let mut focus = Focus::default();
        focus.choose(ToolSlot(3));
        hover(&mut focus, ToolSlot(1), LINGER * 3.0, true);
        assert_eq!(focus.resolve(&SHOWN, None), Some(ToolSlot(3)));
        assert!(!focus.waiting());
    }

    #[test]
    fn gone_tool_shows_nothing() {
        let mut focus = Focus::default();
        focus.choose(ToolSlot(9));
        assert_eq!(focus.resolve(&SHOWN, None), None);
    }
}
