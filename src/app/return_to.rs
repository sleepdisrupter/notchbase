//! Who gets the keyboard back when the notch, a capture or a text field lets go of it: the app that was in
//! front, or the annotator.

use crate::platform::FrontApp;

use super::annotate::Hold;

#[derive(Debug, PartialEq)]
pub(super) enum ReturnTo<App = FrontApp> {
    Annotator,
    App(App),
}

impl<App> ReturnTo<App> {
    pub(super) fn remembered(keyboard: Hold, in_front: impl FnOnce() -> Option<App>) -> Option<Self> {
        if keyboard.is_with_notchbase() { Some(Self::Annotator) } else { in_front().map(Self::App) }
    }

    pub(super) fn resolved(self, keyboard: Hold, in_front: impl FnOnce() -> Option<App>) -> Option<Self> {
        match (self, keyboard) {
            (Self::App(app), _) => Some(Self::App(app)),
            (Self::Annotator, Hold::Lent) => Some(Self::Annotator),
            (Self::Annotator, Hold::NoAnnotator | Hold::Elsewhere) => in_front().map(Self::App),
            (Self::Annotator, Hold::Annotator) => None,
        }
    }

    pub(super) fn app(self) -> Option<App> {
        match self {
            Self::App(app) => Some(app),
            Self::Annotator => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFARI: &str = "Safari";
    const XCODE: &str = "Xcode";
    const EVERY_HOLD: [Hold; 4] = [Hold::NoAnnotator, Hold::Elsewhere, Hold::Annotator, Hold::Lent];

    fn safari() -> Option<&'static str> {
        Some(SAFARI)
    }

    fn xcode() -> Option<&'static str> {
        Some(XCODE)
    }

    fn nothing_in_front() -> Option<&'static str> {
        None
    }

    #[test]
    fn front_app_handed_back() {
        let remembered = ReturnTo::remembered(Hold::NoAnnotator, safari);
        assert_eq!(remembered, Some(ReturnTo::App(SAFARI)));
        let handed_back = remembered.and_then(|target| target.resolved(Hold::NoAnnotator, xcode));
        assert_eq!(handed_back, Some(ReturnTo::App(SAFARI)), "the app from before, not the one in front now");
        assert_eq!(ReturnTo::remembered(Hold::NoAnnotator, nothing_in_front), None);
    }

    #[test]
    fn idle_annotator_changes_nothing() {
        assert_eq!(ReturnTo::remembered(Hold::Elsewhere, safari), Some(ReturnTo::App(SAFARI)));
    }

    #[test]
    fn remembered_app_wins() {
        for keyboard in EVERY_HOLD {
            assert_eq!(ReturnTo::App(SAFARI).resolved(keyboard, xcode), Some(ReturnTo::App(SAFARI)), "{keyboard:?}");
        }
    }

    #[test]
    fn annotator_is_the_way_back() {
        for keyboard in [Hold::Annotator, Hold::Lent] {
            assert_eq!(ReturnTo::remembered(keyboard, safari), Some(ReturnTo::Annotator), "{keyboard:?}");
            assert_eq!(ReturnTo::remembered(keyboard, nothing_in_front), Some(ReturnTo::Annotator), "{keyboard:?}");
        }
    }

    #[test]
    fn borrowed_keyboard_goes_back() {
        assert_eq!(ReturnTo::Annotator.resolved(Hold::Lent, safari), Some(ReturnTo::Annotator));
    }

    #[test]
    fn focused_annotator_left_alone() {
        assert_eq!(ReturnTo::Annotator.resolved(Hold::Annotator, safari), None);
    }

    #[test]
    fn left_annotator_yields_to_app() {
        assert_eq!(ReturnTo::Annotator.resolved(Hold::Elsewhere, xcode), Some(ReturnTo::App(XCODE)));
        assert_eq!(ReturnTo::<&str>::Annotator.resolved(Hold::Elsewhere, nothing_in_front), None);
        let (left, closed) = (Hold::Elsewhere, Hold::NoAnnotator);
        assert_eq!(ReturnTo::Annotator.resolved(left, xcode), ReturnTo::Annotator.resolved(closed, xcode));
    }

    #[test]
    fn closed_annotator_yields_to_app() {
        assert_eq!(ReturnTo::Annotator.resolved(Hold::NoAnnotator, safari), Some(ReturnTo::App(SAFARI)));
        assert_eq!(ReturnTo::<&str>::Annotator.resolved(Hold::NoAnnotator, nothing_in_front), None);
    }

    #[test]
    fn only_apps_pass_to_new_annotator() {
        assert_eq!(ReturnTo::App(SAFARI).app(), Some(SAFARI));
        assert_eq!(ReturnTo::<&str>::Annotator.app(), None);
    }
}
