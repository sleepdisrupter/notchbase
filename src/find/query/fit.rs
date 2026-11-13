//! How a row fits a question: which terms its name holds, which came from a folder above, whether the match was
//! loose, and how tucked away the row is.

use crate::find::model::EntryId;

use super::parse::MOST_TERMS;

pub const NOBODY: EntryId = EntryId::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Away {
    #[default]
    InSight,
    Hidden,
    Foreign,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub own: u8,
    pub read: u8,
    pub over: [EntryId; MOST_TERMS],
    pub leads: bool,
    pub fuzzy: bool,
    pub squeezed: bool,
    pub app: bool,
    pub alias: bool,
    pub ahead: bool,
    pub away: Away,
}

impl Default for Fit {
    fn default() -> Self {
        Self {
            own: 0,
            read: 0,
            over: [NOBODY; MOST_TERMS],
            leads: true,
            fuzzy: false,
            squeezed: false,
            app: false,
            alias: false,
            ahead: false,
            away: Away::InSight,
        }
    }
}

impl Fit {
    pub fn fuzzy() -> Self {
        Self { fuzzy: true, ..Self::default() }
    }

    pub fn squeezed() -> Self {
        Self { squeezed: true, ..Self::default() }
    }

    pub fn owns(&self, term: usize) -> bool {
        self.own & (1 << term) != 0
    }

    pub fn reads(&self, term: usize) -> bool {
        self.read & (1 << term) != 0
    }

    pub fn above(&self, term: usize) -> Option<EntryId> {
        self.over.get(term).copied().filter(|above| *above != NOBODY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_says_where_words_came_from() {
        let fit = Fit { own: 0b001, read: 0b010, over: [NOBODY, NOBODY, 7, NOBODY, NOBODY, NOBODY], ..Fit::default() };
        assert!(fit.owns(0) && !fit.owns(1) && fit.reads(1) && !fit.reads(0));
        assert_eq!((fit.above(0), fit.above(2), fit.above(MOST_TERMS)), (None, Some(7), None));
    }

    #[test]
    fn loose_fit_owns_no_word() {
        let (fuzzy, squeezed) = (Fit::fuzzy(), Fit::squeezed());
        assert_eq!((fuzzy.own, fuzzy.fuzzy, fuzzy.squeezed, fuzzy.app), (0, true, false, false));
        assert_eq!((squeezed.own, squeezed.fuzzy, squeezed.squeezed, squeezed.app), (0, false, true, false));
        assert!(!Fit::default().fuzzy && !Fit::default().squeezed && Fit::default().leads);
        assert!(!Fit::default().alias && !Fit::default().ahead, "only an app named by the question goes ahead");
        assert_eq!(Fit::default().away, Away::InSight, "nothing lies out of sight until it was weighed");
        assert!(Away::InSight < Away::Hidden && Away::Hidden < Away::Foreign, "each lies further away");
    }
}
