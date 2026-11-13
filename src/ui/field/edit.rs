//! The text being edited and its caret: typing, erasing by letter or word, moving, and select all.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) text: String,
    pub(super) caret: usize,
    pub(super) all: bool,
}

impl Line {
    pub(super) fn new(text: &str, all: bool) -> Self {
        Self { text: text.to_owned(), caret: text.len(), all: all && !text.is_empty() }
    }

    pub(super) fn set(&mut self, text: &str) {
        *self = Self::new(text, false);
    }

    pub(super) fn insert(&mut self, typed: &str) -> bool {
        let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
        let replaced = self.take_all();
        if typed.is_empty() {
            return replaced;
        }
        self.text.insert_str(self.caret, &typed);
        self.caret += typed.len();
        true
    }

    pub(super) fn erase_back(&mut self, word: bool) -> bool {
        if self.take_all() {
            return true;
        }
        let from = if word { word_start(&self.text, self.caret) } else { before(&self.text, self.caret) };
        self.text.replace_range(from..self.caret, "");
        let changed = from < self.caret;
        self.caret = from;
        changed
    }

    pub(super) fn erase_forward(&mut self, word: bool) -> bool {
        if self.take_all() {
            return true;
        }
        let to = if word { word_end(&self.text, self.caret) } else { after(&self.text, self.caret) };
        self.text.replace_range(self.caret..to, "");
        to > self.caret
    }

    pub(super) fn left(&mut self, word: bool) {
        self.caret = match std::mem::take(&mut self.all) {
            true => 0,
            false if word => word_start(&self.text, self.caret),
            false => before(&self.text, self.caret),
        };
    }

    pub(super) fn right(&mut self, word: bool) {
        self.caret = match std::mem::take(&mut self.all) {
            true => self.text.len(),
            false if word => word_end(&self.text, self.caret),
            false => after(&self.text, self.caret),
        };
    }

    pub(super) fn home(&mut self) {
        self.all = false;
        self.caret = 0;
    }

    pub(super) fn end(&mut self) {
        self.all = false;
        self.caret = self.text.len();
    }

    pub(super) fn select_all(&mut self) {
        self.all = !self.text.is_empty();
        self.caret = self.text.len();
    }

    fn take_all(&mut self) -> bool {
        if !std::mem::take(&mut self.all) {
            return false;
        }
        self.text.clear();
        self.caret = 0;
        true
    }
}

fn before(text: &str, at: usize) -> usize {
    text[..at].char_indices().next_back().map_or(0, |(index, _)| index)
}

fn after(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
}

fn word_start(text: &str, at: usize) -> usize {
    let head = text[..at].trim_end();
    head.rfind(char::is_whitespace).map_or(0, |space| after(head, space))
}

fn word_end(text: &str, at: usize) -> usize {
    let tail = &text[at..];
    let skipped = tail.len() - tail.trim_start().len();
    let word = tail[skipped..].find(char::is_whitespace).unwrap_or(tail.len() - skipped);
    at + skipped + word
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, caret: usize) -> Line {
        Line { text: text.to_owned(), caret, all: false }
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut line = at("crgo", 1);
        assert!(line.insert("a"));
        assert_eq!((line.text.as_str(), line.caret), ("cargo", 2));
        assert!(!line.insert("\t"), "a control character changes nothing");
        assert_eq!(line.text, "cargo");
    }

    #[test]
    fn caret_moves_by_letter_word_line() {
        let mut line = Line::new("one two", false);
        line.left(false);
        assert_eq!(line.caret, 6);
        line.left(true);
        assert_eq!(line.caret, 4);
        line.left(true);
        assert_eq!(line.caret, 0);
        line.right(true);
        assert_eq!(line.caret, 3);
        line.right(true);
        assert_eq!(line.caret, 7);
        line.home();
        assert_eq!(line.caret, 0);
        line.left(false);
        assert_eq!(line.caret, 0, "it stops at the start");
        line.end();
        line.right(false);
        assert_eq!(line.caret, 7, "and at the end");
    }

    #[test]
    fn caret_steps_over_whole_chars() {
        let mut line = Line::new("résumé", false);
        line.left(false);
        assert_eq!(&line.text[line.caret..], "é");
        assert!(line.erase_back(false));
        assert_eq!(line.text, "résué");
    }

    #[test]
    fn erasing_works_both_ways_and_by_word() {
        let mut line = at("one two three", 7);
        assert!(line.erase_back(false));
        assert_eq!((line.text.as_str(), line.caret), ("one tw three", 6));
        assert!(line.erase_forward(false));
        assert_eq!(line.text, "one twthree");
        let mut words = at("one two three", 7);
        assert!(words.erase_back(true));
        assert_eq!((words.text.as_str(), words.caret), ("one  three", 4));
        assert!(words.erase_forward(true));
        assert_eq!(words.text, "one ");
        assert!(!at("", 0).erase_back(false), "nothing to erase is no edit");
        assert!(!at("abc", 3).erase_forward(true));
    }

    #[test]
    fn select_all_then_type_replaces() {
        let mut line = Line::new("cargo", false);
        line.select_all();
        assert!(line.insert("x"));
        assert_eq!((line.text.as_str(), line.caret, line.all), ("x", 1, false));
        let mut erased = Line::new("cargo", true);
        assert!(erased.erase_forward(false));
        assert_eq!(erased.text, "");
        let mut moved = Line::new("cargo", true);
        moved.left(false);
        assert_eq!((moved.caret, moved.all), (0, false), "an arrow drops the selection at its edge");
        assert!(!Line::new("", true).all, "an empty line has nothing to select");
    }
}
