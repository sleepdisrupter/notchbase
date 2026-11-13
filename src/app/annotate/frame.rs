//! Reads what the annotator window reports each frame: the newest picture, the brush and whether it closed.

use std::time::Instant;

use crate::annotate::{Brush, Outcome};
use crate::config::AnnotateConfig;

use super::copying::Copying;
use super::reports::Report;
use super::{Annotating, App};

const BRUSH_NOT_SAVED: &str = "Couldn't remember the brush";

#[derive(Debug, Default, PartialEq)]
pub(super) struct Heard {
    drawn: Option<u64>,
    pub(super) brush: Option<Brush>,
    close: bool,
}

impl Heard {
    pub(super) fn of(outcomes: Vec<Outcome>) -> Self {
        let mut heard = Self::default();
        for outcome in outcomes {
            match outcome {
                Outcome::Changed(revision) => heard.drawn = Some(revision),
                Outcome::Remember(brush) => heard.brush = Some(brush),
                Outcome::Close => heard.close = true,
            }
        }
        heard
    }

    pub(super) fn arm(&self, copying: &mut Copying, now: Instant, at_the_keyboard: bool) {
        if let Some(revision) = self.drawn {
            copying.drew(revision, now, at_the_keyboard);
        }
    }
}

impl App {
    pub(in crate::app) fn sync_annotator(&mut self, now: Instant) {
        self.hear_the_copier();
        self.settle_the_keyboard();
        let Some(open) = &mut self.annotating.open else { return };
        let (heard, at_the_keyboard) = (Heard::of(open.window.take_outcomes()), open.window.focused());
        heard.arm(&mut open.copying, now, at_the_keyboard);
        if open.copying.take_due(now, at_the_keyboard) {
            open.hand_to_the_clipboard();
        }
        if open.window.repaint_at().is_some_and(|at| at <= now) {
            open.window.redraw();
        }
        if let Some(brush) = heard.brush {
            self.remember_brush(brush);
        }
        if heard.close {
            self.close_annotator();
        }
    }

    fn hear_the_copier(&mut self) {
        let reports = self.annotating.reports.take();
        for problem in self.annotating.hear(reports) {
            self.screenshot_not_copied(&problem);
        }
    }

    fn settle_the_keyboard(&mut self) {
        let (notches, snap, dim) = (&self.notches, &self.snap, &self.recording_dim);
        let (picker, framer, sampler) = (&self.picker, &self.framer, &self.sampler);
        self.annotating.settle_keyboard(|id| {
            notches.owns(id) || snap.owns(id) || dim.owns(id) || picker.owns(id) || framer.owns(id) || sampler.owns(id)
        });
    }

    pub(super) fn remember_brush(&mut self, brush: Brush) {
        if let Err(err) = self.config.update(|config| config.annotate = AnnotateConfig::from(brush)) {
            self.report(BRUSH_NOT_SAVED, err);
        }
    }
}

impl Annotating {
    fn hear(&mut self, reports: Vec<(u64, Report)>) -> Vec<String> {
        let mut problems = Vec::new();
        for (session, report) in reports {
            self.retiring.heard(session, report.revision());
            let still_owed = self.retiring.owes(session);
            let open = self.open.as_mut().filter(|open| open.session == session);
            match (report, open) {
                (Report::Copied(revision), Some(open)) => open.copied(revision),
                (Report::Copied(_), None) => {}
                (Report::NotCopied(revision, problem), open) => {
                    let on_screen = open.map(|open| {
                        open.copying.refused(revision);
                        open.window.not_copied(revision);
                        open.window.revision()
                    });
                    if is_the_last_word(revision, on_screen.or(still_owed)) {
                        problems.push(problem);
                    }
                }
            }
        }
        problems
    }
}

fn is_the_last_word(failed: u64, newest: Option<u64>) -> bool {
    newest.is_none_or(|newest| newest == failed)
}

#[cfg(test)]
mod tests {
    use crate::annotate::Copier;
    use crate::ui::color;

    use super::*;

    const SESSION: u64 = 4;
    const OLDER_PICTURE: u64 = 1;
    const LAST_PICTURE: u64 = 2;
    const BUSY: &str = "The clipboard is busy";

    fn quiet() -> Annotating {
        Annotating::new(std::sync::Arc::new(|| {}))
    }

    fn closed_owing_its_last_picture() -> Annotating {
        let copier = Copier::start_with(|_| Ok(()), |_| {}, |_, _| {}).expect("a copy thread");
        let mut annotating = quiet();
        annotating.retiring.keep(SESSION, Some(LAST_PICTURE), Some(copier));
        annotating
    }

    fn refused(session: u64, revision: u64) -> Vec<(u64, Report)> {
        vec![(session, Report::NotCopied(revision, BUSY.to_owned()))]
    }

    #[test]
    fn quiet_frame_asks_nothing() {
        assert_eq!(Heard::of(Vec::new()), Heard::default());
    }

    #[test]
    fn frame_reads_latest_picture_and_brush() {
        let (thin, thick) = (Brush::new(color::GREEN, 4.0), Brush::new(color::GREEN, 18.0));
        let outcomes =
            vec![Outcome::Changed(3), Outcome::Remember(thin), Outcome::Changed(4), Outcome::Remember(thick)];
        assert_eq!(Heard::of(outcomes), Heard { drawn: Some(4), brush: Some(thick), close: false });
    }

    #[test]
    fn closing_keeps_last_stroke() {
        let brush = Brush::new(color::INK, 9.0);
        let heard = Heard::of(vec![Outcome::Changed(7), Outcome::Remember(brush), Outcome::Close]);
        assert_eq!(heard, Heard { drawn: Some(7), brush: Some(brush), close: true });
    }

    #[test]
    fn picture_finished_on_close_is_owed() {
        let (now, brush) = (Instant::now(), Brush::new(color::GREEN, 7.0));
        let finished = Heard::of(vec![Outcome::Changed(LAST_PICTURE), Outcome::Remember(brush)]);
        let mut copying = Copying::default();
        finished.arm(&mut copying, now, false);
        assert_eq!((copying.awaited(), finished.brush), (Some(LAST_PICTURE), Some(brush)));
        assert!(copying.take(), "the flush that follows hands it over");
    }

    #[test]
    fn idle_close_owes_nothing() {
        let mut copying = Copying::default();
        Heard::of(Vec::new()).arm(&mut copying, Instant::now(), false);
        assert_eq!(copying.awaited(), None);
        assert!(!copying.take());
    }

    #[test]
    fn failed_copy_said_unless_newer() {
        assert!(is_the_last_word(3, Some(3)), "the picture on screen is the one that failed");
        assert!(!is_the_last_word(3, Some(4)), "the newer picture will report for itself");
        assert!(is_the_last_word(3, None), "the window has closed, so nothing newer is coming");
    }

    #[test]
    fn old_failure_after_close_quiet() {
        let mut annotating = closed_owing_its_last_picture();
        assert_eq!(annotating.hear(refused(SESSION, OLDER_PICTURE)), Vec::<String>::new());
        assert_eq!(annotating.hear(refused(SESSION, LAST_PICTURE)), [BUSY], "the last picture has the last word");
    }

    #[test]
    fn last_failure_after_close_said() {
        assert_eq!(quiet().hear(refused(SESSION, OLDER_PICTURE)), [BUSY]);
        let mut another_window = closed_owing_its_last_picture();
        assert_eq!(another_window.hear(refused(SESSION + 1, OLDER_PICTURE)), [BUSY], "its debt is its own");
    }

    #[test]
    fn brush_saved_as_hex_and_step() {
        let saved = AnnotateConfig::from(Brush::new(color::GREEN, 11.3));
        assert_eq!((saved.color.as_str(), saved.brush_size()), ("#3FC48A", 11.5));
    }
}
