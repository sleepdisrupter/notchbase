//! Between Whisper and the cursor: dropping what Whisper makes up in silence, checking the
//! tidy-up model kept to its job, and fitting the words into the text around the cursor.

use crate::groq::Segment;
use crate::words::{likeness, split, squash};

/// Lines Whisper is known to invent when it hears little or nothing.
const INVENTED: [&str; 13] = [
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "thanks for listening",
    "please subscribe",
    "subscribe",
    "you",
    "bye",
    "bye bye",
    "hmm",
    "subtitles by the amara org community",
    "transcribed by",
    "the end",
];
/// Under this much speech, a line from `INVENTED` was probably invented.
const INVENTED_UNDER: f32 = 1.2;
/// A segment Whisper itself doubts this much is dropped.
const NO_SPEECH: f32 = 0.6;
const DOUBTFUL: f32 = -0.9;
/// Whisper stuck in a loop repeats itself, which compresses very well.
const LOOPING: f32 = 2.6;

/// Whisper's text, without segments it heard nothing in, or loops.
pub fn heard(text: &str, segments: &[Segment], voiced: f32) -> String {
    let kept = if segments.is_empty() {
        text.trim().to_owned()
    } else {
        let kept: Vec<&str> = segments
            .iter()
            .filter(|segment| !(segment.no_speech_prob > NO_SPEECH && segment.avg_logprob < DOUBTFUL))
            .filter(|segment| segment.compression_ratio < LOOPING || segment.text.len() < 40)
            .map(|segment| segment.text.trim())
            .collect();
        kept.join(" ")
    };
    let plain: String = kept.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let invented = INVENTED.iter().any(|line| plain == *line);
    if invented && voiced < INVENTED_UNDER {
        return String::new();
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The tidy-up model's answer, if it kept to its job: the same words, give or take fixes.
/// It's thrown away if it answered the text instead of cleaning it, or rewrote it.
pub fn polished(original: &str, answer: Option<String>) -> String {
    let Some(answer) = answer.map(|answer| answer.trim().trim_matches('"').trim().to_owned()) else {
        return original.to_owned();
    };
    if answer.is_empty() {
        return original.to_owned();
    }
    let before: Vec<String> = split(original).into_iter().map(|w| squash(&w.text)).collect();
    let after: Vec<String> = split(&answer).into_iter().map(|w| squash(&w.text)).collect();
    if before.is_empty() || after.is_empty() {
        return original.to_owned();
    }
    let ratio = after.len() as f32 / before.len() as f32;
    let kept = before.iter().filter(|word| after.contains(word)).count() as f32 / before.len() as f32;
    let close = likeness(&before.join(" "), &after.join(" "));
    let short = before.len() <= 4;
    let sane = ratio > 0.5 && ratio < 1.5 && (kept >= 0.5 || close >= 0.6 || short && close >= 0.4);
    let answer: String = answer.chars().map(|c| if c.is_whitespace() && c != '\n' { ' ' } else { c }).collect();
    if sane && crate::diff::swaps(original, &answer).iter().all(|(from, to)| sounds_alike(from, to)) {
        answer
    } else {
        original.to_owned()
    }
}

/// Whether the tidy-up model swapped a word for one that could have been misheard as it, like
/// "Brock" for "Groq". A swap to a word that sounds nothing like it ("Brock" for "Dana") is the
/// model guessing, and its whole answer is dropped.
fn sounds_alike(from: &str, to: &str) -> bool {
    // "three, no wait, four" written "4": the speaker corrected themselves, and the end is what counts.
    let said = split(from);
    let meant_words = split(to).len();
    if said.len() > meant_words {
        let end = &from[said[said.len() - meant_words].start..];
        if heard_as(end, to) {
            return true;
        }
    }
    heard_as(from, to)
}

fn heard_as(from: &str, to: &str) -> bool {
    let (heard, meant) = (squash(from), squash(to));
    let number = |word: &str| NUMBERS.contains(&word) || word.chars().all(|c| c.is_numeric() || c == ' ');
    heard == meant
        || likeness(&heard, &meant) >= SOUNDS_ALIKE
        || split(from).iter().all(|word| number(&word.text.to_lowercase())) && meant.chars().all(char::is_numeric)
}

/// How alike two spellings must be for one to have been heard as the other.
const SOUNDS_ALIKE: f32 = 0.34;
const NUMBERS: [&str; 21] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "twenty", "thirty", "forty", "fifty", "hundred", "thousand", "million", "and",
];

/// Fits the words to where they land: a space after a word, no capital in mid-sentence, and
/// no full stop when the sentence carries on after the cursor.
pub fn fitted(text: &str, before: Option<&str>, after: Option<&str>) -> String {
    let mut out = text.trim().to_owned();
    if out.is_empty() {
        return out;
    }
    if let Some(before) = before {
        let last = before.chars().last();
        let mid_sentence = before
            .trim_end()
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, ',' | ';' | '(' | '-' | '–'));
        if mid_sentence && starts_with_plain_capital(&out) {
            out = lowercase_first(&out);
        }
        if last.is_some_and(|c| !c.is_whitespace() && !matches!(c, '(' | '[' | '{' | '"' | '“' | '/' | '@' | '#')) {
            out.insert(0, ' ');
        }
    }
    if let Some(after) = after {
        let next = after.chars().next();
        if next.is_some_and(|c| c.is_alphanumeric()) {
            if after.chars().next().is_some_and(char::is_lowercase) && out.ends_with('.') && !out.ends_with("..") {
                out.pop();
            }
            out.push(' ');
        }
    }
    out
}

/// "Hello" but not "I", "GitHub", "NASA" or "Priya" (a name).
fn starts_with_plain_capital(text: &str) -> bool {
    let Some(first) = split(text).into_iter().next() else { return false };
    let mut chars = first.text.chars();
    let Some(initial) = chars.next() else { return false };
    let rest: String = chars.collect();
    initial.is_uppercase()
        && !rest.is_empty()
        && rest.chars().all(|c| !c.is_uppercase())
        && first.start == text.find(initial).unwrap_or(0)
        && crate::words::common(&first.text)
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_lowercase().chain(chars).collect())
}

/// Parts of one long recording, written down separately, as one text.
pub fn joined(parts: &[String]) -> String {
    parts.iter().map(|part| part.trim()).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, no_speech_prob: f32, avg_logprob: f32) -> Segment {
        Segment { text: text.into(), no_speech_prob, avg_logprob, compression_ratio: 1.2 }
    }

    #[test]
    fn invented_lines_in_silence_are_dropped() {
        assert_eq!(heard(" Thank you.", &[], 0.4), "");
        assert_eq!(heard(" Thank you.", &[], 3.0), "Thank you.", "said for real");
        assert_eq!(heard("Bye-bye!", &[], 0.3), "");
        assert_eq!(heard("Ship it.", &[], 0.3), "Ship it.");
    }

    #[test]
    fn doubtful_segments_are_dropped() {
        let segments = [segment(" Ship it today.", 0.01, -0.2), segment(" Thanks for watching!", 0.9, -1.4)];
        assert_eq!(heard("Ship it today. Thanks for watching!", &segments, 2.0), "Ship it today.");
    }

    #[test]
    fn polish_is_kept_when_it_cleans() {
        let original = "um so I pushed the notch base release to github";
        let answer = "So I pushed the notchbase release to GitHub.";
        assert_eq!(polished(original, Some(answer.into())), answer);
        assert_eq!(polished("hello", Some("Hello.".into())), "Hello.");
        assert_eq!(polished("hello", None), "hello");
    }

    #[test]
    fn polish_that_answers_is_thrown_away() {
        let original = "what's the weather like in Paris tomorrow";
        let answer =
            "I don't have live weather data, but Paris in late November is usually cool and rainy, around 8°C.";
        assert_eq!(polished(original, Some(answer.into())), original);
        assert_eq!(
            polished("write me a poem about cats", Some("Whiskers soft in morning light".into())),
            "write me a poem about cats"
        );
    }

    #[test]
    fn polish_that_swaps_in_unlike_words_is_thrown_away() {
        let original = "Can you ask Brock whether it works? Also ping Dana.";
        let guessed = "Can you ask Dana whether it works? Also ping Dana.";
        assert_eq!(polished(original, Some(guessed.into())), original);
        let heard_right = "Can you ask Groq whether it works? Also ping Dana.";
        assert_eq!(polished(original, Some(heard_right.into())), heard_right);
        let corrected = "so the meeting is at three, no wait, four pm with uh Priya";
        assert_eq!(
            polished(corrected, Some("The meeting is at 4\u{202f}pm with Priya.".into())),
            "The meeting is at 4 pm with Priya."
        );
    }

    #[test]
    fn words_fit_the_cursor() {
        assert_eq!(fitted("Hello there.", Some("I said"), None), " hello there.");
        assert_eq!(fitted("Hello there.", Some("Done. "), None), "Hello there.");
        assert_eq!(fitted("GitHub is down.", Some("and"), None), " GitHub is down.");
        assert_eq!(fitted("Ship it.", Some(""), Some("and then")), "Ship it ");
        assert_eq!(fitted("Ship it.", None, None), "Ship it.");
        assert_eq!(fitted("Priya agreed.", Some("so"), None), " Priya agreed.");
        assert_eq!(fitted("next", Some("("), None), "next");
    }

    #[test]
    fn parts_join() {
        assert_eq!(joined(&["First part.".into(), " ".into(), "Second part.".into()]), "First part. Second part.");
    }
}
