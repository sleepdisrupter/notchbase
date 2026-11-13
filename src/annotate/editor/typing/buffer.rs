//! The text of a block and its caret, in letters. Invisible characters are dropped and a block holds 10,000 at most.

use std::ops::RangeInclusive;

const LINE_BREAK: char = '\n';
const LONGEST: usize = 10_000;
// Zero-width and direction marks. A block of nothing but these would hold words with no ink.
const NEVER_DRAWN: [RangeInclusive<char>; 5] = [
    '\u{200B}'..='\u{200F}',
    '\u{202A}'..='\u{202E}',
    '\u{2060}'..='\u{2064}',
    '\u{2066}'..='\u{206F}',
    '\u{FEFF}'..='\u{FEFF}',
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Buffer {
    text: String,
    caret: usize,
}

impl Buffer {
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn caret(&self) -> usize {
        self.caret
    }

    pub(super) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(super) fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub(super) fn insert(&mut self, typed: &str) -> bool {
        let room = LONGEST.saturating_sub(self.text.chars().count());
        let typed = typed.chars().filter(|letter| can_be_written(*letter));
        let typed: String = typed.take(room).collect();
        self.text.insert_str(self.byte_at(self.caret), &typed);
        self.caret += typed.chars().count();
        !typed.is_empty()
    }

    pub(super) fn erase_back(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        self.caret -= 1;
        self.erase_forward()
    }

    pub(super) fn erase_forward(&mut self) -> bool {
        let (from, to) = (self.byte_at(self.caret), self.byte_at(self.caret + 1));
        self.text.replace_range(from..to, "");
        to > from
    }

    pub(super) fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub(super) fn right(&mut self) {
        self.place(self.caret + 1);
    }

    pub(super) fn trim_the_end(&mut self) -> bool {
        let kept = self.text.trim_end().len();
        let trimmed = kept < self.text.len();
        self.text.truncate(kept);
        self.place(self.caret);
        trimmed
    }

    pub(super) fn place(&mut self, caret: usize) {
        self.caret = caret.min(self.text.chars().count());
    }

    fn byte_at(&self, letter: usize) -> usize {
        self.text.char_indices().nth(letter).map_or(self.text.len(), |(byte, _)| byte)
    }
}

fn can_be_written(letter: char) -> bool {
    letter == LINE_BREAK || !(letter.is_control() || NEVER_DRAWN.iter().any(|hidden| hidden.contains(&letter)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(text: &str) -> Buffer {
        let mut buffer = Buffer::default();
        buffer.insert(text);
        buffer
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut buffer = holding("cat");
        assert_eq!((buffer.text(), buffer.caret()), ("cat", 3));
        buffer.left();
        assert!(buffer.insert("r"));
        assert_eq!((buffer.text(), buffer.caret()), ("cart", 3));
        buffer.place(0);
        buffer.insert("s");
        assert_eq!((buffer.text(), buffer.caret()), ("scart", 1));
    }

    #[test]
    fn keeps_breaks_drops_controls() {
        let mut buffer = holding("one\r\ntwo\tthree\u{7}\u{1b}");
        assert_eq!((buffer.text(), buffer.caret()), ("one\ntwothree", 12));
        assert!(!buffer.insert("\r\t\u{0}"), "nothing that can be typed");
        assert_eq!(buffer.text(), "one\ntwothree");
        assert!(buffer.insert("\n"));
        assert_eq!(buffer.text(), "one\ntwothree\n");
    }

    #[test]
    fn invisible_characters_are_dropped() {
        let mut buffer = holding("a\u{200B}b\u{FEFF}\u{202E}c\u{2060}\u{2069}");
        assert_eq!((buffer.text(), buffer.caret()), ("abc", 3));
        assert!(!buffer.insert("\u{200B}\u{200D}\u{2064}\u{206F}"), "nothing that leaves a mark");
        assert!(holding("\u{200F}\u{202A}\u{2066}").is_empty());
        assert_eq!(holding("\u{2010}\u{2065}\u{2070}").text(), "\u{2010}\u{2065}\u{2070}", "their neighbours stay");
    }

    #[test]
    fn long_paste_is_cut_short() {
        let mut buffer = holding("é");
        assert!(buffer.insert(&"я".repeat(LONGEST + 50)));
        assert_eq!((buffer.text().chars().count(), buffer.caret()), (LONGEST, LONGEST));
        assert!(!buffer.insert("x"), "a full block takes nothing more");
        assert!(buffer.erase_back() && buffer.insert("xy"));
        assert!(buffer.text().ends_with("яx"));
    }

    #[test]
    fn caret_counts_letters() {
        let mut buffer = holding("éя😀x");
        assert_eq!((buffer.text().len(), buffer.caret()), (9, 4));
        buffer.left();
        assert!(buffer.erase_back());
        assert_eq!((buffer.text(), buffer.caret()), ("éяx", 2));
        buffer.place(1);
        assert!(buffer.erase_forward());
        assert_eq!((buffer.text(), buffer.caret()), ("éx", 1));
        buffer.insert("ж");
        assert_eq!((buffer.text(), buffer.caret()), ("éжx", 2));
    }

    #[test]
    fn erasing_stops_at_either_end() {
        let mut buffer = holding("ab");
        assert!(!buffer.erase_forward(), "nothing lies after the caret");
        assert!(buffer.erase_back() && buffer.erase_back());
        assert!(!buffer.erase_back() && buffer.is_empty());
        assert_eq!(buffer.caret(), 0);
    }

    #[test]
    fn caret_never_leaves_the_words() {
        let mut buffer = holding("ab\ncd");
        (0..9).for_each(|_| buffer.left());
        assert_eq!(buffer.caret(), 0);
        (0..9).for_each(|_| buffer.right());
        assert_eq!(buffer.caret(), 5);
        buffer.place(99);
        assert_eq!(buffer.caret(), 5);
    }

    #[test]
    fn trailing_space_can_be_trimmed() {
        let mut buffer = holding("  one\n two \n\n ");
        assert!(buffer.trim_the_end());
        assert_eq!((buffer.text(), buffer.caret()), ("  one\n two", 10));
        assert!(!buffer.trim_the_end(), "nothing is left to trim");
        let mut spaces = holding(" \n ");
        assert!(spaces.trim_the_end() && spaces.is_empty());
        assert_eq!(spaces.caret(), 0);
    }

    #[test]
    fn white_space_is_blank_not_empty() {
        assert!(Buffer::default().is_empty() && Buffer::default().is_blank());
        let spaces = holding("  \n ");
        assert!(spaces.is_blank() && !spaces.is_empty());
        assert!(!holding(" a ").is_blank());
    }
}
