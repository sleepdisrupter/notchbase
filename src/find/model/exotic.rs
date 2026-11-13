//! The original spelling of names the fold changed beyond letter case.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::find) struct Exotics {
    rows: Vec<u32>,
    ends: Vec<u32>,
    text: String,
}

impl Exotics {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn push(&mut self, row: u32, name: &str) {
        self.text.push_str(name);
        self.rows.push(row);
        self.ends.push(self.text.len() as u32);
    }

    pub fn name(&self, row: u32) -> Option<&str> {
        self.rows.binary_search(&row).ok().map(|nth| self.nth(nth))
    }

    pub fn each(&self) -> impl Iterator<Item = (u32, &str)> {
        self.rows.iter().enumerate().map(|(nth, row)| (*row, self.nth(nth)))
    }

    pub fn shrink(&mut self) {
        self.rows.shrink_to_fit();
        self.ends.shrink_to_fit();
        self.text.shrink_to_fit();
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.text.capacity() + (self.rows.capacity() + self.ends.capacity()) * size_of::<u32>()
    }

    fn nth(&self, nth: usize) -> &str {
        let from = nth.checked_sub(1).map_or(0, |before| self.ends[before] as usize);
        self.text.get(from..self.ends[nth] as usize).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_found_by_its_row() {
        let mut exotics = Exotics::default();
        assert_eq!((exotics.len(), exotics.name(0)), (0, None));
        exotics.push(3, "R\u{e9}sum\u{e9}.pdf");
        exotics.push(4, "");
        exotics.push(90, "\u{41e}\u{442}\u{447}\u{451}\u{442}");
        assert_eq!(exotics.len(), 3);
        assert_eq!(exotics.name(3), Some("R\u{e9}sum\u{e9}.pdf"));
        assert_eq!(exotics.name(4), Some(""));
        assert_eq!(exotics.name(90), Some("\u{41e}\u{442}\u{447}\u{451}\u{442}"));
        assert_eq!((exotics.name(0), exotics.name(5), exotics.name(91)), (None, None, None));
    }

    #[test]
    fn names_keep_their_order() {
        let mut exotics = Exotics::default();
        [(1, "\u{c9}t\u{e9}"), (7, "Stra\u{df}e"), (8, "\u{c5}")]
            .iter()
            .for_each(|(row, name)| exotics.push(*row, name));
        let listed: Vec<(u32, &str)> = exotics.each().collect();
        assert_eq!(listed, [(1, "\u{c9}t\u{e9}"), (7, "Stra\u{df}e"), (8, "\u{c5}")]);
        let mut same = exotics.clone();
        same.shrink();
        assert!(same == exotics && same.bytes() <= exotics.bytes());
    }
}
