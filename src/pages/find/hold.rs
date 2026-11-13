//! The rows on screen, and how a new answer merges into them without moving the selected row. Also an Enter
//! pressed before the answer arrives, held until it does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::bridge::{FindRow, FindView, How, SHOWN};

const HOLD_FOR: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Taken {
    Nothing,
    Refresh,
    Fresh,
}

#[derive(Default)]
pub(super) struct Shown {
    pub(super) rows: Vec<FindRow>,
    pub(super) query: Arc<str>,
    since: u64,
    answered: u64,
}

impl Shown {
    pub(super) fn reset(&mut self, since: u64) {
        *self = Self { since, answered: since, ..Self::default() };
    }

    pub(super) fn answers(&self, draft: &str) -> bool {
        self.answered > self.since && *self.query == *draft
    }

    pub(super) fn take(&mut self, view: &FindView, selected: usize) -> Taken {
        self.refresh_icons(view);
        if view.answered <= self.answered {
            return Taken::Nothing;
        }
        self.answered = view.answered;
        if self.rows.is_empty() || view.query != self.query {
            self.rows = view.rows.to_vec();
            self.query = view.query.clone();
            return Taken::Fresh;
        }
        self.rows.truncate(selected + 1);
        self.rows.retain(|row| !view.gone.contains(&row.key));
        let fresh: Vec<FindRow> =
            view.rows.iter().filter(|row| !self.rows.iter().any(|kept| kept.key == row.key)).cloned().collect();
        self.rows.extend(fresh);
        self.rows.truncate(SHOWN);
        Taken::Refresh
    }

    fn refresh_icons(&mut self, view: &FindView) {
        for row in self.rows.iter_mut().filter(|row| row.icon.is_none()) {
            if let Some(icon) = view.rows.iter().find(|new| new.key == row.key).and_then(|new| new.icon.clone()) {
                row.icon = Some(icon);
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Hold(Option<(How, Instant)>);

impl Hold {
    pub(super) fn hold(&mut self, how: How, now: Instant) {
        self.0 = Some((how, now));
    }

    pub(super) fn drop_it(&mut self) {
        self.0 = None;
    }

    pub(super) fn holding(&self) -> bool {
        self.0.is_some()
    }

    pub(super) fn expire(&mut self, now: Instant) {
        if self.0.is_some_and(|(_, at)| now.duration_since(at) > HOLD_FOR) {
            self.0 = None;
        }
    }

    pub(super) fn due(&mut self, shown: &Shown, draft: &str) -> Option<How> {
        let (how, _) = self.0.filter(|_| shown.answers(draft))?;
        self.0 = None;
        (!shown.rows.is_empty()).then_some(how)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::bridge::{FindState, Kind};

    fn row(key: u64) -> FindRow {
        FindRow {
            key,
            path: Arc::from(Path::new(&format!("/f/{key}"))),
            name: format!("{key}").into(),
            place: "~".into(),
            marks: Vec::new(),
            kind: Kind::Other,
            app: false,
            folder: false,
            cloud: false,
            modified: 0,
            why: None,
            predicted: false,
            icon: None,
        }
    }

    fn view(answered: u64, query: &str, keys: &[u64]) -> FindView {
        let rows: Vec<FindRow> = keys.iter().map(|&key| row(key)).collect();
        FindView { state: FindState::Ready, answered, query: query.into(), rows: rows.into(), ..FindView::default() }
    }

    fn keys(shown: &Shown) -> Vec<u64> {
        shown.rows.iter().map(|row| row.key).collect()
    }

    #[test]
    fn old_answer_never_taken() {
        let mut shown = Shown::default();
        shown.reset(4);
        assert_eq!(shown.take(&view(4, "", &[1, 2]), 0), Taken::Nothing);
        assert!(shown.rows.is_empty() && !shown.answers(""));
        assert_eq!(shown.take(&view(5, "", &[3]), 0), Taken::Fresh);
        assert!(shown.answers("") && !shown.answers("c"));
    }

    #[test]
    fn refresh_keeps_selected_and_above() {
        let mut shown = Shown::default();
        shown.take(&view(1, "ca", &[1, 2, 3, 4]), 0);
        assert_eq!(shown.take(&view(2, "ca", &[9, 3, 1, 8]), 1), Taken::Refresh);
        assert_eq!(keys(&shown), [1, 2, 9, 3, 8]);
        assert_eq!(shown.take(&view(3, "car", &[7]), 1), Taken::Fresh);
        assert_eq!(keys(&shown), [7]);
    }

    #[test]
    fn gone_row_dropped_even_selected() {
        let mut shown = Shown::default();
        shown.take(&view(1, "ca", &[1, 2, 3]), 0);
        let trimmed = FindView { gone: Arc::from([1]), ..view(2, "ca", &[2, 3]) };
        assert_eq!(shown.take(&trimmed, 0), Taken::Refresh);
        assert_eq!(keys(&shown), [2, 3]);
    }

    #[test]
    fn late_icon_reaches_kept_row() {
        let mut shown = Shown::default();
        shown.take(&view(1, "ca", &[1]), 0);
        let mut later = view(1, "ca", &[1]);
        let icon = Arc::new(egui::ColorImage::filled([1, 1], egui::Color32::WHITE));
        later.rows = vec![FindRow { icon: Some(icon), ..row(1) }].into();
        assert_eq!(shown.take(&later, 0), Taken::Nothing);
        assert!(shown.rows[0].icon.is_some());
    }

    #[test]
    fn held_enter_fires_or_goes_stale() {
        let now = Instant::now();
        let mut shown = Shown::default();
        let mut hold = Hold::default();
        hold.hold(How::Open, now);
        shown.take(&view(1, "ca", &[1]), 0);
        assert_eq!(hold.due(&shown, "car"), None, "still waiting for car");
        shown.take(&view(2, "car", &[2]), 0);
        assert_eq!(hold.due(&shown, "car"), Some(How::Open));
        assert!(!hold.holding());
        hold.hold(How::Reveal, now);
        hold.expire(now + HOLD_FOR + Duration::from_millis(1));
        assert!(!hold.holding());
        hold.hold(How::Open, now);
        shown.take(&view(3, "carx", &[]), 0);
        assert_eq!(hold.due(&shown, "carx"), None, "no rows drops it");
        assert!(!hold.holding());
    }
}
