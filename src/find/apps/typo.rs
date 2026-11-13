//! An app's name typed with one slip of the finger still finds the app.

use crate::find::rank::boundary;
use crate::find::weights::TYPO_FROM;

pub fn mistyped(needle: &[u8], name: &[u8]) -> Option<usize> {
    let first = *needle.first().filter(|_| needle.len() >= TYPO_FROM)?;
    let starts = memchr::memchr_iter(first, name).filter(|at| boundary(name, *at));
    starts.into_iter().find(|at| meant(needle, &name[*at..]))
}

fn meant(typed: &[u8], word: &[u8]) -> bool {
    let lengths = [typed.len(), typed.len() + 1, typed.len() - 1];
    lengths.into_iter().filter_map(|length| word.get(..length)).any(|start| one_slip(typed, start))
}

fn one_slip(typed: &[u8], meant: &[u8]) -> bool {
    let Some(at) = typed.iter().zip(meant).position(|(typed, meant)| typed != meant) else {
        return typed.len().abs_diff(meant.len()) <= 1;
    };
    let (typed, meant) = (&typed[at..], &meant[at..]);
    match typed.len().cmp(&meant.len()) {
        std::cmp::Ordering::Greater => typed[1..] == *meant,
        std::cmp::Ordering::Less => *typed == meant[1..],
        std::cmp::Ordering::Equal => typed[1..] == meant[1..] || swapped(typed, meant),
    }
}

fn swapped(typed: &[u8], meant: &[u8]) -> bool {
    typed.len() >= 2 && typed[0] == meant[1] && typed[1] == meant[0] && typed[2..] == meant[2..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_slip_still_matches() {
        for typed in ["sfaari", "safair", "safarj", "saafari", "sfari", "safar", "safari"] {
            assert_eq!(mistyped(typed.as_bytes(), b"safari"), Some(0), "{typed}");
        }
        for typed in ["chrmoe", "chrone", "chhrome", "chrom"] {
            assert_eq!(mistyped(typed.as_bytes(), b"google chrome"), Some(7), "{typed}");
        }
        assert_eq!(mistyped(b"stuido", b"visual studio code"), Some(7));
        assert_eq!(mistyped(b"spto", b"spotify"), Some(0), "a slip in what was typed so far");
    }

    #[test]
    fn worse_slips_match_nothing() {
        for typed in ["sfaair", "safxxi", "xafari", "afari"] {
            assert_eq!(mistyped(typed.as_bytes(), b"safari"), None, "{typed}");
        }
        assert_eq!(mistyped(b"saf", b"safari"), None, "three letters are too few");
        assert_eq!(mistyped(b"sfa", b"safari"), None);
        assert_eq!(mistyped(b"hrome", b"google chrome"), None, "it has to start a word");
        assert_eq!(mistyped(b"", b"safari"), None);
        assert_eq!(mistyped(b"safari", b""), None);
    }

    #[test]
    fn slip_checked_at_every_word() {
        assert_eq!(mistyped(b"moniter", b"activity monitor"), Some(9));
        assert_eq!(mistyped(b"actvity", b"activity monitor"), Some(0));
        assert_eq!(mistyped(b"monitro", b"monitor-2"), Some(0));
        assert_eq!(mistyped(b"notez", b"notion"), None);
        assert_eq!(mistyped(b"node", b"notes"), Some(0), "one letter off is one letter off");
    }
}
