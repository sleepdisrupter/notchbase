//! Shortens long names from the middle, keeping the extension and the first match in sight.

use std::ops::Range;

const ELLIPSIS: char = '\u{2026}';
const TAIL_PAST_EXTENSION: usize = 3;
const BEFORE_MATCH: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cut {
    head: usize,
    tail: usize,
}

impl Cut {
    pub(super) fn whole(name: &str) -> Self {
        let count = name.chars().count();
        Self { head: count, tail: count }
    }

    pub(super) fn text(&self, name: &str) -> String {
        let whole = name.chars().count();
        if self.head >= whole {
            return name.to_owned();
        }
        let head = name.chars().take(self.head);
        head.chain(std::iter::once(ELLIPSIS)).chain(name.chars().skip(self.tail)).collect()
    }

    pub(super) fn place(&self, index: usize) -> Option<usize> {
        match index {
            index if index < self.head => Some(index),
            index if index >= self.tail && self.tail > self.head => Some(self.head + 1 + index - self.tail),
            _ => None,
        }
    }
}

pub(super) fn middle(name: &str, fits: impl Fn(&str) -> bool) -> Cut {
    let whole = Cut::whole(name);
    if fits(name) {
        return whole;
    }
    let count = whole.head;
    let extension = name.rfind('.').filter(|&dot| dot > 0).map_or(0, |dot| name[dot..].chars().count());
    let cut = |kept: usize| {
        let tail = (extension + TAIL_PAST_EXTENSION).min(kept / 2).max(extension.min(kept));
        Cut { head: kept - tail, tail: count - tail }
    };
    let (mut lo, mut hi) = (0, count.saturating_sub(1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&cut(mid).text(name)) { lo = mid } else { hi = mid - 1 }
    }
    cut(lo)
}

pub(super) fn middle_showing(name: &str, mark: Option<&Range<u16>>, fits: impl Fn(&str) -> bool) -> Cut {
    let cut = middle(name, &fits);
    let Some(mark) = mark.filter(|mark| (mark.start..mark.end).any(|index| cut.place(usize::from(index)).is_none()))
    else {
        return cut;
    };
    let start = usize::from(mark.start);
    let mut tails = (start.saturating_sub(BEFORE_MATCH)..=start).filter(|&tail| tail > 0);
    let Some(tail) = tails.find(|&tail| fits(&Cut { head: 0, tail }.text(name))) else {
        return cut;
    };
    let (mut lo, mut hi) = (0, tail - 1);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&Cut { head: mid, tail }.text(name)) { lo = mid } else { hi = mid - 1 }
    }
    Cut { head: lo, tail }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_name_keeps_extension() {
        let name = "quarterly-report-final-version.pdf";
        let cut = middle(name, |text| text.chars().count() <= 16);
        let text = cut.text(name);
        assert_eq!(text.chars().count(), 16);
        assert!(text.ends_with("ion.pdf") && text.starts_with("quarter"), "{text}");
        assert_eq!(cut.place(0), Some(0));
        assert_eq!(cut.place(12), None, "a letter in the middle is gone");
        assert_eq!(cut.place(name.chars().count() - 1), Some(15));
        assert_eq!(middle("short.txt", |_| true), Cut::whole("short.txt"));
        assert_eq!(Cut::whole("a.txt").place(4), Some(4));
    }

    #[test]
    fn cut_keeps_first_match() {
        let name = "Dynamic Island and the notch, explained.mp4";
        let fits = |text: &str| text.chars().count() <= 30;
        let mark = 23..28;
        let text = middle_showing(name, Some(&mark), fits).text(name);
        assert!(text.contains("notch") && text.ends_with(".mp4") && text.starts_with("Dyn"), "{text}");
        assert_eq!(text, "Dynam\u{2026}the notch, explained.mp4");
        let early = 0..7;
        assert_eq!(middle_showing(name, Some(&early), fits), middle(name, fits), "a match in sight changes nothing");
    }
}
