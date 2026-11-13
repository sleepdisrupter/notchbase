//! How the window is brought forward: asking for it again unminimises it, handing focus back never does.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Asked,
    HandedBack,
}

impl Focus {
    pub(super) fn restores(self) -> bool {
        self == Self::Asked
    }

    pub(super) fn reaches(self, minimised: bool) -> bool {
        self.restores() || !minimised
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMISED: bool = true;
    const ON_SCREEN: bool = false;

    #[test]
    fn handed_back_leaves_minimised() {
        assert!(!Focus::HandedBack.restores());
        assert!(!Focus::HandedBack.reaches(MINIMISED), "nor is it handed a keyboard it cannot use");
    }

    #[test]
    fn handed_back_reaches_on_screen() {
        assert!(Focus::HandedBack.reaches(ON_SCREEN));
    }

    #[test]
    fn asking_restores_and_focuses() {
        assert!(Focus::Asked.restores());
        assert!(Focus::Asked.reaches(MINIMISED) && Focus::Asked.reaches(ON_SCREEN));
    }
}
