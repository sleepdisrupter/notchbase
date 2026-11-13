//! Words that read as more than letters: a kind (`pdf`, `pictures`), an ending, or a date (`today`, `last week`).

use std::sync::Arc;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::find::context::{Context, DAY, Stamp};
use crate::find::kind::Kind;
use crate::find::model::flag;

use super::parse::Term;

const WEEK: Stamp = 7 * DAY;
const RECENT: Stamp = WEEK;
const UNKNOWN: Stamp = 0;
const FIRST_OF_THE_MONTH: u32 = 1;
const BETWEEN_READINGS: &str = " \u{b7} ";
const FOLDERS: &str = "Folders";

#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Kind(Kind),
    Ends(Vec<u8>),
    Within(Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub from: Stamp,
    pub until: Stamp,
    pub label: &'static str,
}

impl Span {
    pub fn holds(&self, modified: Stamp, created: Stamp) -> bool {
        let at = modified.max(created);
        at != UNKNOWN && self.from <= at && at < self.until
    }
}

impl Reading {
    pub fn holds(&self, name: &[u8], flags: u8, modified: Stamp, created: Stamp) -> bool {
        match self {
            Reading::Kind(kind) => Kind::of(name, flags) == *kind,
            Reading::Ends(ending) => flags & flag::DIR == 0 && name.len() > ending.len() && name.ends_with(ending),
            Reading::Within(span) => span.holds(modified, created),
        }
    }

    pub fn launches(&self) -> bool {
        *self == Reading::Kind(Kind::App)
    }

    pub fn may_hold(&self, flags: u8) -> bool {
        match self {
            Reading::Kind(Kind::Folder) => flags & flag::DIR != 0,
            Reading::Ends(_) => flags & flag::DIR == 0,
            Reading::Kind(_) | Reading::Within(_) => true,
        }
    }

    fn label(&self) -> String {
        match self {
            Reading::Kind(kind) => plural(*kind).to_owned(),
            Reading::Ends(ending) => files_ending(ending),
            Reading::Within(span) => span.label.to_owned(),
        }
    }
}

pub fn dated(phrase: &str, context: &Context) -> Option<Span> {
    let today = context.day_start();
    let week = today.saturating_sub(Stamp::from(context.weekday) * DAY);
    let (from, until, label) = match phrase {
        "today" => (today, Stamp::MAX, "today"),
        "yesterday" => (today.saturating_sub(DAY), today, "yesterday"),
        "this week" => (week, Stamp::MAX, "this week"),
        "last week" => (week.saturating_sub(WEEK), week, "last week"),
        "this month" => (month_start(context), Stamp::MAX, "this month"),
        "recent" => (context.now.saturating_sub(RECENT), Stamp::MAX, "recent"),
        _ => return None,
    };
    Some(Span { from, until, label })
}

pub fn files_ending(ending: &[u8]) -> String {
    let bare = ending.strip_prefix(b".").unwrap_or(ending);
    match Kind::ending(bare).filter(|kind| kind.extensions().len() == 1) {
        Some(kind) => plural(kind).to_owned(),
        None => format!(".{} files", String::from_utf8_lossy(bare)),
    }
}

pub fn echo(terms: &[Term]) -> Option<Arc<str>> {
    let mut said: Vec<String> = Vec::new();
    for term in terms {
        let strict = term.strict.then(|| files_ending(&term.needle));
        let folders = (term.folders && strict.is_none()).then(|| FOLDERS.to_owned());
        let read = term.reading.as_ref().map(Reading::label);
        for label in [strict, folders, read].into_iter().flatten() {
            if !said.contains(&label) {
                said.push(label);
            }
        }
    }
    let joined = said.join(BETWEEN_READINGS);
    let mut letters = joined.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect::<String>().into())
}

fn month_start(context: &Context) -> Stamp {
    let local = i64::from(context.now) + i64::from(context.offset_secs);
    let day = Timestamp::from_second(local).map_or(FIRST_OF_THE_MONTH, |at| at.to_zoned(TimeZone::UTC).day() as u32);
    context.day_start().saturating_sub(day.saturating_sub(FIRST_OF_THE_MONTH) * DAY)
}

fn plural(kind: Kind) -> &'static str {
    match kind {
        Kind::Folder => FOLDERS,
        Kind::App => "Apps",
        Kind::Document => "Documents",
        Kind::Pdf => "PDFs",
        Kind::Image => "Images",
        Kind::Video => "Videos",
        Kind::Audio => "Music",
        Kind::Archive => "Archives",
        Kind::Code => "Code",
        Kind::Text => "Text files",
        Kind::Sheet => "Spreadsheets",
        Kind::Slides => "Presentations",
        Kind::Font => "Fonts",
        Kind::Disk => "Disk images",
        Kind::Link => "Links",
        Kind::Other => "Files",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::HOUR;
    use crate::find::query::parse;

    const WEDNESDAY_NOON: Stamp = 1_704_110_400 + 9 * DAY;
    const BERLIN: i32 = 3600;

    fn wednesday() -> Context {
        Context::at(WEDNESDAY_NOON, BERLIN, None)
    }

    fn range(phrase: &str) -> (Stamp, Stamp) {
        let span = dated(phrase, &wednesday()).unwrap_or_else(|| panic!("{phrase} is a date"));
        (span.from, span.until)
    }

    #[test]
    fn dates_become_local_day_ranges() {
        let midnight = WEDNESDAY_NOON - 13 * HOUR;
        assert_eq!(wednesday().weekday, 2);
        assert_eq!(range("today"), (midnight, Stamp::MAX));
        assert_eq!(range("yesterday"), (midnight - DAY, midnight));
        assert_eq!(range("this week"), (midnight - 2 * DAY, Stamp::MAX), "the week began on Monday");
        assert_eq!(range("last week"), (midnight - 9 * DAY, midnight - 2 * DAY));
        assert_eq!(range("this month"), (midnight - 9 * DAY, Stamp::MAX), "it is the tenth of January");
        assert_eq!(range("recent"), (WEDNESDAY_NOON - 7 * DAY, Stamp::MAX));
        assert_eq!(dated("tomorrow", &wednesday()), None);
    }

    #[test]
    fn date_reads_newer_stamp() {
        let today = dated("today", &wednesday()).unwrap();
        assert!(today.holds(WEDNESDAY_NOON, 5) && today.holds(5, WEDNESDAY_NOON));
        assert!(!today.holds(WEDNESDAY_NOON - 2 * DAY, 5));
        let yesterday = dated("yesterday", &wednesday()).unwrap();
        assert!(yesterday.holds(WEDNESDAY_NOON - DAY, 0) && !yesterday.holds(WEDNESDAY_NOON, WEDNESDAY_NOON - DAY));
        let ever = Span { from: 0, until: Stamp::MAX, label: "ever" };
        assert!(!ever.holds(0, 0), "a file with no dates was not changed today");
    }

    #[test]
    fn kind_and_ending_hold_files() {
        let images = Reading::Kind(Kind::Image);
        assert!(images.holds(b"photo.png", 0, 0, 0) && !images.holds(b"photo.png", flag::DIR, 0, 0));
        let rust = Reading::Ends(b".rs".to_vec());
        assert!(Reading::Kind(Kind::App).launches() && !images.launches() && !rust.launches(), "only apps launch");
        assert!(rust.holds(b"main.rs", 0, 0, 0));
        assert!(
            !rust.holds(b"main.rst", 0, 0, 0)
                && !rust.holds(b".rs", 0, 0, 0)
                && !rust.holds(b"old.rs", flag::DIR, 0, 0)
        );
    }

    #[test]
    fn field_echoes_readings() {
        let said = |text: &str| echo(&parse(text, &wednesday()).terms).map(|echo| echo.to_string());
        assert_eq!(said("pdf last week").as_deref(), Some("PDFs \u{b7} last week"));
        assert_eq!(said("today").as_deref(), Some("Today"));
        assert_eq!(said("folders").as_deref(), Some("Folders"));
        assert_eq!(said("src/").as_deref(), Some("Folders"));
        assert_eq!(said("*.rs").as_deref(), Some(".rs files"));
        assert_eq!(said("rs notes").as_deref(), Some(".rs files"));
        assert_eq!(said("image images").as_deref(), Some("Images"), "one reading is said once");
        assert_eq!(said("cargo readme"), None);
    }
}
