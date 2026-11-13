//! Folds names for matching: lower case, accents dropped, and combining marks read like the composed letter.

const END_OF_NAME: char = '\0';
const STANDS_FOR_END_OF_NAME: u8 = b' ';
const FIRST_MARK: char = '\u{300}';
const LAST_MARK: char = '\u{36f}';
const FIRST_LATIN: char = '\u{c0}';
const LAST_LATIN: char = '\u{17f}';
const LETTERS_EACH: usize = 2;
const UNUSED: u8 = b' ';
const LONGEST_LETTER: usize = 4;
const LATIN: &[u8; (LAST_LATIN as usize - FIRST_LATIN as usize + 1) * LETTERS_EACH] =
    b"a a a a a a aec e e e e i i i i d n o o o o o   o u u u u y thss\
a a a a a a aec e e e e i i i i d n o o o o o   o u u u u y thy \
a a a a a a c c c c c c c c d d d d e e e e e e e e e e g g g g \
g g g g h h h h i i i i i i i i i i ijijj j k k k l l l l l l l \
l l l n n n n n n n n n o o o o o o oeoer r r r r r s s s s s s \
s s t t t t t t u u u u u u u u u u u u w w y y y z z z z z z s ";
const CYRILLIC: [(char, char); 4] =
    [('\u{419}', '\u{438}'), ('\u{439}', '\u{438}'), ('\u{401}', '\u{435}'), ('\u{451}', '\u{435}')];

pub fn fold_into(text: &str, out: &mut Vec<u8>) -> bool {
    if text.is_ascii() && !text.contains(END_OF_NAME) {
        out.extend(text.bytes().map(|byte| byte.to_ascii_lowercase()));
        return false;
    }
    text.chars().fold(false, |lost, letter| fold_letter(letter, out) | lost)
}

pub fn fold_query(text: &str) -> Vec<u8> {
    let mut folded = Vec::with_capacity(text.len());
    fold_into(text, &mut folded);
    folded
}

fn fold_letter(letter: char, out: &mut Vec<u8>) -> bool {
    if letter == END_OF_NAME {
        out.push(STANDS_FOR_END_OF_NAME);
        return true;
    }
    if letter.is_ascii() {
        out.push(letter.to_ascii_lowercase() as u8);
        return false;
    }
    if (FIRST_MARK..=LAST_MARK).contains(&letter) {
        return true;
    }
    if let Some(plain) = latin(letter) {
        out.extend(plain.iter().filter(|byte| **byte != UNUSED));
        return true;
    }
    if let Some((_, plain)) = CYRILLIC.iter().find(|(accented, _)| *accented == letter) {
        keep(*plain, out);
        return true;
    }
    if letter.to_lowercase().eq([letter]) {
        keep(letter, out);
        return false;
    }
    for lower in letter.to_lowercase() {
        if lower == letter {
            keep(lower, out);
        } else {
            fold_letter(lower, out);
        }
    }
    true
}

fn latin(letter: char) -> Option<&'static [u8]> {
    let nth = (letter as usize).checked_sub(FIRST_LATIN as usize).filter(|_| letter <= LAST_LATIN)?;
    let plain = &LATIN[nth * LETTERS_EACH..(nth + 1) * LETTERS_EACH];
    plain.iter().any(|byte| *byte != UNUSED).then_some(plain)
}

fn keep(letter: char, out: &mut Vec<u8>) {
    out.extend(letter.encode_utf8(&mut [0; LONGEST_LETTER]).bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folded(text: &str) -> (String, bool) {
        let mut out = Vec::new();
        let lost = fold_into(text, &mut out);
        (String::from_utf8(out).expect("a fold is text"), lost)
    }

    #[test]
    fn folding_lowers_and_appends() {
        let mut out = b"kept/".to_vec();
        assert!(!fold_into("ReadMe.MD", &mut out));
        assert_eq!(out, b"kept/readme.md");
        assert_eq!(fold_query(""), b"");
        assert_eq!(folded("notes 2 (final)_v3.TXT"), ("notes 2 (final)_v3.txt".to_owned(), false));
    }

    #[test]
    fn accents_fold_away_and_say_so() {
        assert_eq!(folded("Cr\u{e8}me Br\u{fb}l\u{e9}e.PDF"), ("creme brulee.pdf".to_owned(), true));
        assert_eq!(folded("Stra\u{df}e"), ("strasse".to_owned(), true));
        assert_eq!(folded("\u{c6}sop \u{152}uvre \u{de}or"), ("aesop oeuvre thor".to_owned(), true));
        assert_eq!(folded("\u{141}\u{f3}d\u{17a}"), ("lodz".to_owned(), true));
        assert_eq!(fold_query("Caf\u{e9} 2"), b"cafe 2");
        assert_eq!(folded("M\u{dc}NCHEN"), folded("m\u{fc}nchen"));
    }

    #[test]
    fn combining_marks_fold_alike() {
        let composed = "r\u{e9}sum\u{e9}.pdf";
        let decomposed = "re\u{301}sume\u{301}.pdf";
        assert_ne!(composed, decomposed);
        assert_eq!(folded(composed), ("resume.pdf".to_owned(), true));
        assert_eq!(folded(decomposed), ("resume.pdf".to_owned(), true));
        assert_eq!(folded("\u{439}\u{451}"), folded("\u{438}\u{306}\u{435}\u{308}"), "also outside Latin");
    }

    #[test]
    fn latin_letters_fold_plain() {
        for letter in FIRST_LATIN..=LAST_LATIN {
            let (plain, lost) = folded(&letter.to_string());
            if !lost {
                assert!(['\u{d7}', '\u{f7}'].contains(&letter), "{letter:?} is a sign, not a letter");
                assert_eq!(plain, letter.to_string());
                continue;
            }
            assert!((1..=LETTERS_EACH).contains(&plain.len()), "{letter:?} became {plain:?}");
            assert!(plain.bytes().all(|byte| byte.is_ascii_lowercase()), "{letter:?} became {plain:?}");
            let lower: String = letter.to_lowercase().collect();
            assert_eq!(folded(&lower).0, plain, "{letter:?} and {lower:?} are one letter to a search");
        }
    }

    #[test]
    fn other_alphabets_only_lower() {
        let (lower, upper) = ("\u{43e}\u{442}\u{447}\u{435}\u{442}", "\u{41e}\u{422}\u{427}\u{415}\u{422}");
        assert_eq!(folded(lower), (lower.to_owned(), false));
        assert_eq!(folded(upper), (lower.to_owned(), true));
        assert_eq!(folded("\u{6587}\u{4ef6}.txt"), ("\u{6587}\u{4ef6}.txt".to_owned(), false));
        assert_eq!(folded("\u{1e9e}"), ("ss".to_owned(), true), "a capital sharp s is still a sharp s");
        assert_eq!(folded("\u{1f600} Fun"), ("\u{1f600} fun".to_owned(), false));
    }

    #[test]
    fn fold_never_holds_end_byte() {
        assert_eq!(folded("a\0B"), ("a b".to_owned(), true));
        assert_eq!(folded("\0"), (" ".to_owned(), true));
        assert_eq!(folded("\u{301}"), (String::new(), true));
    }

    #[test]
    fn folding_twice_changes_nothing() {
        let names =
            ["Cr\u{e8}me Br\u{fb}l\u{e9}e.PDF", "\u{41e}\u{422}\u{427}\u{401}\u{422}", "Stra\u{df}e", "\u{130}zmir"];
        for name in names {
            let once = folded(name).0;
            assert_eq!(folded(&once), (once.clone(), false), "{name}");
        }
    }
}
