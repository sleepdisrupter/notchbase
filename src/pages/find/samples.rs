//! Sample answers for the Find previews.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::bridge::{Access, FindRow, FindState, FindView, Kind, Why};

const MINUTE: u32 = 60;
const HOUR: u32 = 60 * MINUTE;
const DAY: u32 = 24 * HOUR;
const ENTRIES: u32 = 1_452_310;
const SCANNED: u32 = 12_480;
const QUERY: &str = "notch";
const NOTHING: &str = "invoice 1987";
const HOME: &str = "/Users/john";

type Sample = (u64, &'static str, Kind, u32);

pub(crate) fn suggestions() -> FindView {
    let rows = [
        ((11, "Documents/GitHub/notchbase/src/main.rs", Kind::Code, 9 * MINUTE), Why::After("Cargo.toml".into())),
        ((12, "Documents/Invoices/March 2026.pdf", Kind::Pdf, 2 * HOUR), Why::Opened(0)),
        ((13, "Documents/GitHub/notchbase", Kind::Folder, 25 * MINUTE), Why::Busy(14)),
        ((14, "Desktop/Trip to Lisbon.jpg", Kind::Image, DAY), Why::Edited(0)),
    ];
    let mut view = ready("", rows.into_iter().map(|(sample, why)| row(sample, Some(why), None)).collect());
    if let Some(first) = Arc::get_mut(&mut view.rows).and_then(|rows| rows.first_mut()) {
        first.predicted = true;
    }
    view
}

pub(crate) fn results() -> FindView {
    let rows: [(Sample, u16); 7] = [
        ((24, "Applications/NotchNook.app", Kind::App, 40 * DAY), 0),
        ((21, "Documents/GitHub/notchbase", Kind::Folder, 4 * MINUTE), 0),
        ((22, "Documents/GitHub/notchbase/wiki/Notch.md", Kind::Text, 3 * HOUR), 0),
        ((23, "Documents/Design/notch-island-mockups.png", Kind::Image, DAY), 0),
        ((25, "Downloads/Dynamic Island and the notch, explained.mp4", Kind::Video, 2 * DAY), 23),
        ((26, "Documents/GitHub/notchbase/src/notch", Kind::Folder, 5 * HOUR), 0),
        ((27, "Music/Notches - Live at the Roundhouse.m4a", Kind::Audio, 400 * DAY), 0),
    ];
    let marked = |at: u16| at..at + QUERY.len() as u16;
    ready(QUERY, rows.into_iter().map(|(sample, at)| row(sample, None, Some(marked(at)))).collect())
}

pub(crate) fn none() -> FindView {
    FindView { access: Access::Limited, ..ready(NOTHING, Vec::new()) }
}

pub(crate) fn indexing() -> FindView {
    FindView { state: FindState::Starting, entries: SCANNED, indexing: Some(SCANNED), ..FindView::default() }
}

pub(crate) fn access() -> FindView {
    FindView { access: Access::Limited, nudge: true, ..suggestions() }
}

pub(crate) fn off() -> FindView {
    FindView::default()
}

fn ready(query: &str, rows: Vec<FindRow>) -> FindView {
    FindView {
        state: FindState::Ready,
        answered: 1,
        query: query.into(),
        rows: rows.into(),
        entries: ENTRIES,
        ..FindView::default()
    }
}

fn row((key, path, kind, age): Sample, why: Option<Why>, mark: Option<Range<u16>>) -> FindRow {
    let now = u32::try_from(jiff::Timestamp::now().as_second()).unwrap_or(u32::MAX);
    let full = Path::new(HOME).join(path);
    let app = kind == Kind::App;
    let shown = if app { full.file_stem() } else { full.file_name() };
    let name = shown.map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let parent = Path::new(path).parent().map(|parent| parent.to_string_lossy().into_owned());
    let place =
        parent.filter(|parent| !parent.is_empty()).map_or_else(|| "~".to_owned(), |parent| format!("~/{parent}"));
    let when = now.saturating_sub(age);
    let why = why.map(|why| match why {
        Why::Opened(_) => Why::Opened(when),
        Why::Edited(_) => Why::Edited(when),
        Why::Added(_) => Why::Added(when),
        other => other,
    });
    FindRow {
        key,
        path: Arc::from(full.as_path()),
        name: name.into(),
        place: place.into(),
        marks: mark.into_iter().collect(),
        kind,
        app,
        folder: app || kind == Kind::Folder,
        cloud: false,
        modified: when,
        why,
        predicted: false,
        icon: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_real_states() {
        assert_eq!(suggestions().rows.len(), crate::bridge::ONE_SCREEN);
        assert!(suggestions().rows[0].predicted && suggestions().query.is_empty());
        let results = results();
        assert!(results.rows.iter().all(|row| !row.marks.is_empty()));
        assert!(
            results
                .rows
                .iter()
                .all(|row| row.marks.iter().all(|mark| usize::from(mark.end) <= row.name.chars().count()))
        );
        let app = &results.rows[0];
        assert_eq!((&*app.name, app.app, app.folder, app.kind), ("NotchNook", true, true, Kind::App));
        assert_eq!((&*app.place, &*app.path), ("~/Applications", Path::new("/Users/john/Applications/NotchNook.app")));
        assert!(results.rows[1..].iter().all(|row| !row.app), "one app, as the service sends it, then files");
        assert_eq!((&*results.rows[1].name, &*results.rows[1].place), ("notchbase", "~/Documents/GitHub"));
        assert!(none().rows.is_empty() && !none().query.is_empty());
        assert_eq!(indexing().state, FindState::Starting);
        assert!(access().nudge);
        assert_eq!(off().state, FindState::Off);
    }
}
