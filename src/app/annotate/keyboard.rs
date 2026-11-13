//! Who holds the keyboard while the annotator is open, and when the notch borrows it.

use winit::window::WindowId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Hold {
    NoAnnotator,
    Elsewhere,
    Annotator,
    Lent,
}

impl Hold {
    pub(in crate::app) fn is_with_notchbase(self) -> bool {
        matches!(self, Self::Annotator | Self::Lent)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Farewell {
    WhereItCameFrom,
    TheAppInFront,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Holder {
    #[default]
    Elsewhere,
    Annotator,
    Leaving,
    Lent(WindowId),
}

#[derive(Debug, Default)]
pub(super) struct Keyboard {
    holder: Holder,
    strayed: bool,
}

impl Keyboard {
    pub(super) fn annotator(&mut self, focused: bool) {
        self.holder = match (self.holder, focused) {
            (_, true) => Holder::Annotator,
            (Holder::Annotator, false) => Holder::Leaving,
            (holder, false) => holder,
        };
    }

    pub(super) fn another_window(&mut self, id: WindowId, focused: bool) {
        self.holder = match (self.holder, focused) {
            (Holder::Elsewhere, _) => Holder::Elsewhere,
            (_, true) => Holder::Lent(id),
            (Holder::Lent(borrower), false) if borrower == id => Holder::Leaving,
            (holder, false) => holder,
        };
    }

    pub(super) fn settle(&mut self, still_open: impl Fn(WindowId) -> bool) {
        let left_notchbase = match self.holder {
            Holder::Leaving => true,
            Holder::Lent(borrower) => !still_open(borrower),
            Holder::Elsewhere | Holder::Annotator => false,
        };
        if left_notchbase {
            self.holder = Holder::Elsewhere;
            self.strayed = true;
        }
    }

    pub(super) fn hold(&self) -> Hold {
        match self.holder {
            Holder::Elsewhere => Hold::Elsewhere,
            Holder::Annotator | Holder::Leaving => Hold::Annotator,
            Holder::Lent(_) => Hold::Lent,
        }
    }

    pub(super) fn farewell(&self) -> Option<Farewell> {
        let back = if self.strayed { Farewell::TheAppInFront } else { Farewell::WhereItCameFrom };
        self.hold().is_with_notchbase().then_some(back)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(raw: u64) -> WindowId {
        WindowId::from(raw)
    }

    fn notch() -> WindowId {
        window(1)
    }

    fn overlay() -> WindowId {
        window(2)
    }

    fn focused() -> Keyboard {
        let mut keyboard = Keyboard::default();
        keyboard.annotator(false);
        keyboard.annotator(true);
        keyboard
    }

    fn settled(mut keyboard: Keyboard) -> Keyboard {
        keyboard.settle(|_| true);
        keyboard
    }

    #[test]
    fn holds_nothing_until_focused() {
        let mut keyboard = Keyboard::default();
        keyboard.annotator(false);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Elsewhere, None));
        keyboard.annotator(true);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Annotator, Some(Farewell::WhereItCameFrom)));
    }

    #[test]
    fn notch_click_borrows_keyboard() {
        let mut keyboard = focused();
        keyboard.annotator(false);
        assert!(keyboard.hold().is_with_notchbase(), "who takes it is not known until the turn ends");
        keyboard.another_window(notch(), true);
        let keyboard = settled(keyboard);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Lent, Some(Farewell::WhereItCameFrom)));
    }

    #[test]
    fn loan_seen_from_either_window() {
        let mut keyboard = focused();
        keyboard.another_window(notch(), true);
        keyboard.annotator(false);
        assert_eq!(settled(keyboard).hold(), Hold::Lent);
    }

    #[test]
    fn loan_passes_through_capture() {
        let mut keyboard = focused();
        keyboard.annotator(false);
        keyboard.another_window(notch(), true);
        keyboard.settle(|_| true);
        keyboard.another_window(overlay(), false);
        assert_eq!(keyboard.hold(), Hold::Lent, "a window that was never the borrower ends nothing");
        keyboard.another_window(notch(), false);
        keyboard.another_window(overlay(), true);
        keyboard.settle(|_| true);
        assert_eq!(keyboard.hold(), Hold::Lent);
        keyboard.another_window(overlay(), false);
        keyboard.annotator(true);
        let keyboard = settled(keyboard);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Annotator, Some(Farewell::WhereItCameFrom)));
    }

    #[test]
    fn leaving_for_app_is_remembered() {
        let mut keyboard = focused();
        keyboard.annotator(false);
        keyboard.settle(|_| true);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Elsewhere, None));
        keyboard.annotator(true);
        assert_eq!((keyboard.hold(), keyboard.farewell()), (Hold::Annotator, Some(Farewell::TheAppInFront)));
    }

    #[test]
    fn losing_focus_ends_loan() {
        let mut keyboard = focused();
        keyboard.annotator(false);
        keyboard.another_window(notch(), true);
        keyboard.settle(|_| true);
        keyboard.another_window(notch(), false);
        keyboard.settle(|_| true);
        assert_eq!(keyboard.hold(), Hold::Elsewhere);
        keyboard.annotator(true);
        assert_eq!(keyboard.farewell(), Some(Farewell::TheAppInFront));
    }

    #[test]
    fn silent_close_ends_loan() {
        let mut keyboard = focused();
        keyboard.annotator(false);
        keyboard.another_window(overlay(), true);
        keyboard.settle(|id| id != overlay());
        assert_eq!(keyboard.hold(), Hold::Elsewhere);
    }

    #[test]
    fn taking_from_app_borrows_nothing() {
        let mut keyboard = Keyboard::default();
        keyboard.another_window(notch(), true);
        assert_eq!(settled(keyboard).hold(), Hold::Elsewhere);
        let mut keyboard = focused();
        keyboard.annotator(false);
        keyboard.settle(|_| true);
        keyboard.another_window(notch(), true);
        assert_eq!(settled(keyboard).hold(), Hold::Elsewhere);
    }
}
