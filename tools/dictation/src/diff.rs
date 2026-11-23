//! What changed between the text that was typed and the text the user left there: the words
//! they swapped, which are the corrections dictation learns from.

use crate::words::{split, squash};

/// The longest swap that still reads as fixing a word rather than rewriting the sentence.
const LONGEST: usize = 3;

/// Each run of words replaced by another, as (typed, edited). Pure insertions and deletions,
/// and changes to punctuation alone, are left out.
pub fn corrections(typed: &str, edited: &str) -> Vec<(String, String)> {
    let a: Vec<String> = split(typed).into_iter().map(|word| word.text).collect();
    let b: Vec<String> = split(edited).into_iter().map(|word| word.text).collect();
    // A rewrite, not a correction.
    if a.is_empty() || b.is_empty() || a.len().abs_diff(b.len()) > a.len().max(4) / 2 {
        return Vec::new();
    }
    aligned(&a, &b, LONGEST)
}

/// Every run of words replaced by another, however long.
pub fn swaps(before: &str, after: &str) -> Vec<(String, String)> {
    let a: Vec<String> = split(before).into_iter().map(|word| word.text).collect();
    let b: Vec<String> = split(after).into_iter().map(|word| word.text).collect();
    aligned(&a, &b, usize::MAX)
}

fn aligned(a: &[String], b: &[String], longest: usize) -> Vec<(String, String)> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let same = |x: &String, y: &String| x.to_lowercase() == y.to_lowercase() || x == y;
    let mut table = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] =
                if same(&a[i], &b[j]) { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let kept = usize::from(table[0][0]);
    if longest != usize::MAX && kept * 2 < a.len().min(b.len()) {
        return Vec::new();
    }
    let mut swaps = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut from, mut to): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut flush = |from: &mut Vec<&str>, to: &mut Vec<&str>| {
        let usable = !from.is_empty() && !to.is_empty() && from.len() <= longest && to.len() <= longest;
        let (heard, meant) = (from.join(" "), to.join(" "));
        if usable && heard != meant && !squash(&meant).is_empty() {
            swaps.push((heard, meant));
        }
        from.clear();
        to.clear();
    };
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && same(&a[i], &b[j]) {
            if a[i] != b[j] {
                from.push(&a[i]);
                to.push(&b[j]);
            }
            flush(&mut from, &mut to);
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || table[i][j + 1] >= table[i + 1][j]) {
            to.push(&b[j]);
            j += 1;
        } else {
            from.push(&a[i]);
            i += 1;
        }
    }
    flush(&mut from, &mut to);
    swaps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
    }

    #[test]
    fn swapped_words_are_found() {
        let found = corrections("ask Brock about the notch base release", "ask Groq about the notchbase release");
        assert_eq!(found, pairs(&[("Brock", "Groq"), ("notch base", "notchbase")]));
    }

    #[test]
    fn capitals_count_as_a_correction() {
        assert_eq!(corrections("push to Github now", "push to GitHub now"), pairs(&[("Github", "GitHub")]));
    }

    #[test]
    fn punctuation_and_additions_teach_nothing() {
        assert!(corrections("hello world", "hello, world!").is_empty());
        assert!(corrections("send it today", "send it today please").is_empty());
        assert!(corrections("send it today", "send today").is_empty());
    }

    #[test]
    fn every_swap_is_listed() {
        let found = swaps("so ask Brock now please", "So ask Dana now.");
        assert_eq!(found, pairs(&[("so", "So"), ("Brock", "Dana")]));
    }

    #[test]
    fn rewrites_teach_nothing() {
        assert!(corrections("the quick brown fox jumps", "a completely different sentence about cats").is_empty());
        assert!(corrections("", "anything").is_empty());
    }
}
