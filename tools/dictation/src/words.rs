//! Words and how they compare: splitting text into words, telling a term (a name, a product, some
//! code) from an everyday word, and how close two spellings are.

/// Everyday English words. A word on this list is never "corrected" into a glossary term, and is
/// never learned as one.
const COMMON: &str = "a about above after again against all almost also always am an and another any are around as ask at \
away back bad be because been before being below best better between big both bring but buy by call came can car \
change check close come could day did different do does doing done down during each early end even every eye face \
fact far feel few find fine first for found free from full get give go going gone good got great had hand has have \
having he head hear help her here him his home house how i idea if important in into is it its just keep kind know \
large last late later leave left let life light like line little live long look lot love made make man many may me \
mean might mind more most move much must my name need never new next nice night no not note nothing now of off oh \
ok okay old on once one only open or order other our out over own part people place play point put quite rather \
read real really right room run said same saw say see seem send set she should show side since small so some \
something soon start still stop such sure take talk tell than thank thanks that the their them then there these \
they thing things think this those though thought three through time to today together told too took try turn two \
under up us use used very want was way we week well went were what when where which while who why will with work \
would write year yes yet you your just ready code file files fix test run build push pull release bridge half lag \
review ship update key config move look call team user users data page window windows screen mail email meeting \
plan today tomorrow yesterday morning evening monday tuesday wednesday thursday friday saturday sunday hi hello \
hey bye please sorry yeah nope sound word words text type typed mark main base notch app apps road \
i'm i'll i've i'd you're you'll you've you'd he's she's it's we're we'll we've they're they'll they've that's \
there's what's let's don't doesn't didn't can't couldn't won't wouldn't isn't aren't wasn't weren't shouldn't \
haven't hasn't gonna wanna gotta lots maybe anyway actually basically literally probably definitely ";

pub fn common(word: &str) -> bool {
    let lower = word.to_lowercase();
    COMMON.split_whitespace().any(|known| known == lower)
}

/// A word in a text, and where it sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub start: usize,
    pub end: usize,
    /// Whether it starts a sentence, so a capital letter means nothing.
    pub opens: bool,
}

fn inner(c: char) -> bool {
    matches!(c, '.' | '-' | '_' | '\'' | '’' | '/' | '+' | '#')
}

/// The words of `text`: letters and digits, with dots, dashes, apostrophes and the like kept
/// when they sit between them ("node.js", "x86_64", "don't").
pub fn split(text: &str) -> Vec<Word> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut words = Vec::new();
    let mut opens = true;
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if !c.is_alphanumeric() {
            if matches!(c, '.' | '!' | '?' | '\n' | ':') {
                opens = true;
            }
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() {
            let c = chars[j].1;
            let joins = inner(c) && chars.get(j + 1).is_some_and(|(_, next)| next.is_alphanumeric());
            if c.is_alphanumeric() || joins {
                j += 1;
            } else {
                break;
            }
        }
        let end = chars.get(j).map_or(text.len(), |(at, _)| *at);
        words.push(Word { text: text[start..end].to_owned(), start, end, opens });
        opens = false;
        i = j;
    }
    words
}

/// Lowercase letters and digits only: how two spellings are compared.
pub fn squash(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Whether a word looks like a name, a product or code rather than an everyday word.
pub fn term_like(word: &Word) -> bool {
    let text = possessive(&word.text).0;
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 || chars.len() > 40 || chars.iter().all(|c| c.is_numeric()) {
        return false;
    }
    let letters = chars.iter().any(|c| c.is_alphabetic());
    let digits = chars.iter().any(|c| c.is_numeric());
    let inner_capital = chars.iter().skip(1).any(|c| c.is_uppercase());
    let joined = chars.iter().any(|c| matches!(c, '.' | '_' | '/' | '+' | '#'));
    let capitalized = chars[0].is_uppercase() && !word.opens && !common(text) && text != "I";
    letters && (inner_capital || digits || joined || capitalized)
}

/// "notchbase's" is "notchbase" and "'s".
pub fn possessive(text: &str) -> (&str, &str) {
    for ending in ["'s", "’s"] {
        if let Some(stem) = text.strip_suffix(ending).filter(|stem| !stem.is_empty()) {
            return (stem, &text[stem.len()..]);
        }
    }
    (text, "")
}

/// Edit distance between two strings, by characters.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb)).min(above + 1).min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// 1 for the same, 0 for nothing alike.
pub fn likeness(a: &str, b: &str) -> f32 {
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return 1.0;
    }
    1.0 - distance(a, b) as f32 / longest as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str) -> Word {
        Word { text: text.into(), start: 0, end: text.len(), opens: false }
    }

    #[test]
    fn words_keep_inner_marks() {
        let words: Vec<String> = split("Ship node.js, x86_64 and don't. OK?").into_iter().map(|w| w.text).collect();
        assert_eq!(words, ["Ship", "node.js", "x86_64", "and", "don't", "OK"]);
        let opens: Vec<bool> = split("One two. Three").into_iter().map(|w| w.opens).collect();
        assert_eq!(opens, [true, false, true]);
    }

    #[test]
    fn words_know_where_they_are() {
        let text = "café, Groq!";
        let words = split(text);
        assert_eq!(&text[words[0].start..words[0].end], "café");
        assert_eq!(&text[words[1].start..words[1].end], "Groq");
    }

    #[test]
    fn terms_look_like_terms() {
        for term in ["GitHub", "iOS", "x86", "node.js", "snake_case", "Priya", "Groq's"] {
            assert!(term_like(&word(term)), "{term}");
        }
        for plain in ["hello", "The", "I", "2024", "a", "Monday"] {
            assert!(!term_like(&word(plain)), "{plain}");
        }
        let first = Word { opens: true, ..word("Priya") };
        assert!(!term_like(&first), "a capital that starts a sentence tells nothing");
    }

    #[test]
    fn spellings_compare() {
        assert_eq!(distance("teraform", "terraform"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert!(likeness("pria", "priya") >= 0.8);
        assert!(likeness("room", "roam") < 0.8);
        assert_eq!(squash("Notch-Base's"), "notchbases");
        assert_eq!(possessive("Groq's"), ("Groq", "'s"));
    }
}
