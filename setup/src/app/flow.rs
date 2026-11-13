//! Which page shows, and what the buttons and keys do on each one.

use std::sync::Arc;

use egui::{Context, Key, ViewportCommand};
use notchbase_release::Version;

use super::Setup;
use crate::install::{self, Installed, Job, Latest, Lookup, Progress, Stage};
use crate::pages::{Intent, Task};
use crate::shot::Pin;

const FILL_SPEED: f32 = 0.9;
const FILLED: f32 = 0.999;
// Made-up state for the screenshots `shot` takes.
const SAMPLE_PROGRESS: f32 = 0.58;
const SAMPLE_REMOVING: f32 = 0.42;
const SAMPLE_VERSION: &str = "0.2.0";
const SAMPLE_SIZE: u64 = 16_331_264;
const SAMPLE_PROBLEM: &str = "Can\u{2019}t reach the update server";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Page {
    Welcome,
    Confirm,
    Working,
    Done,
    Failed,
}

pub(super) const PAGES: [Page; 5] = [Page::Welcome, Page::Confirm, Page::Working, Page::Done, Page::Failed];

impl Setup {
    pub(super) fn pin(&mut self, pin: Pin) {
        let sample = Installed { version: Version::parse(SAMPLE_VERSION) };
        self.installed = matches!(pin, Pin::Installed | Pin::Confirm).then_some(sample);
        self.version = Version::parse(SAMPLE_VERSION);
        self.size = Some(SAMPLE_SIZE);
        match pin {
            Pin::Welcome | Pin::Installed => {}
            Pin::Confirm => (self.task, self.page) = (Task::Remove, Page::Confirm),
            Pin::Working => (self.page, self.shown) = (Page::Working, SAMPLE_PROGRESS),
            Pin::Removing => {
                (self.task, self.page, self.stage, self.shown) =
                    (Task::Remove, Page::Working, Stage::Removing, SAMPLE_REMOVING);
            }
            Pin::Done => (self.page, self.shown) = (Page::Done, 1.0),
            Pin::Failed => (self.page, self.problem) = (Page::Failed, SAMPLE_PROBLEM.into()),
        }
    }

    pub(super) fn advance(&mut self, dt: f32) {
        let Some(job) = &self.job else { return };
        let Progress { stage, fraction } = job.progress();
        if let Stage::Failed(problem) = stage {
            (self.page, self.problem, self.job) = (Page::Failed, problem, None);
            return;
        }
        self.shown = approach(self.shown, fraction, dt * FILL_SPEED);
        if stage == Stage::Done && self.shown >= FILLED {
            (self.page, self.job) = (Page::Done, None);
            self.installed = install::installed();
        }
        self.stage = stage;
    }

    pub(super) fn act(&mut self, intent: Intent, ctx: &Context) {
        match intent {
            Intent::Toggle(setting) => self.options.toggle(setting),
            Intent::Install => {
                if let Latest::Found(version, release) = self.lookup.latest() {
                    (self.version, self.size) = (Some(version), release.size);
                    self.begin(Task::Install, Stage::Downloading);
                    self.job = Some(Job::install(self.options, release, Arc::clone(&self.wake)));
                }
            }
            Intent::AskRemove => (self.task, self.page) = (Task::Remove, Page::Confirm),
            Intent::Remove => {
                self.begin(Task::Remove, Stage::Removing);
                self.job = Some(Job::uninstall(Arc::clone(&self.wake)));
            }
            Intent::Back => (self.task, self.page) = (Task::Install, Page::Welcome),
            Intent::Retry => {
                if matches!(self.lookup.latest(), Latest::Failed(_)) {
                    self.lookup = Lookup::start(Arc::clone(&self.wake));
                }
                (self.task, self.page, self.installed) = (Task::Install, Page::Welcome, install::installed());
            }
            Intent::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
        }
    }

    fn begin(&mut self, task: Task, stage: Stage) {
        (self.task, self.stage, self.shown, self.page) = (task, stage, 0.0, Page::Working);
    }

    pub(super) fn keys(&self, ctx: &Context, latest: &Latest) -> Option<Intent> {
        let (enter, escape) = ctx.input(|input| (input.key_pressed(Key::Enter), input.key_pressed(Key::Escape)));
        key_intent(self.page, latest, enter, escape)
    }
}

fn key_intent(page: Page, latest: &Latest, enter: bool, escape: bool) -> Option<Intent> {
    if escape && page != Page::Working {
        return Some(if page == Page::Confirm { Intent::Back } else { Intent::Close });
    }
    if !enter {
        return None;
    }
    match (page, latest) {
        (Page::Welcome, Latest::Found(..)) => Some(Intent::Install),
        (Page::Welcome, Latest::Failed(_)) | (Page::Failed, _) => Some(Intent::Retry),
        (Page::Done, _) => Some(Intent::Close),
        _ => None,
    }
}

fn approach(from: f32, to: f32, step: f32) -> f32 {
    if to > from { (from + step).min(to) } else { to }
}

#[cfg(test)]
mod tests {
    use notchbase_release::Release;

    use super::*;

    fn found() -> Latest {
        let release = Release { version: "0.3.1".into(), url: String::new(), sha256: String::new(), size: None };
        Latest::Found(Version::parse("0.3.1").unwrap(), release)
    }

    #[test]
    fn escape_closes_or_steps_back() {
        let failed = Latest::Failed("offline".into());
        assert_eq!(key_intent(Page::Welcome, &found(), false, true), Some(Intent::Close));
        assert_eq!(key_intent(Page::Done, &found(), true, true), Some(Intent::Close));
        assert_eq!(key_intent(Page::Failed, &failed, false, true), Some(Intent::Close));
        assert_eq!(key_intent(Page::Confirm, &found(), false, true), Some(Intent::Back));
        assert_eq!(key_intent(Page::Working, &found(), false, true), None);
    }

    #[test]
    fn enter_takes_the_main_action() {
        let failed = Latest::Failed("offline".into());
        assert_eq!(key_intent(Page::Welcome, &found(), true, false), Some(Intent::Install));
        assert_eq!(key_intent(Page::Welcome, &failed, true, false), Some(Intent::Retry));
        assert_eq!(key_intent(Page::Welcome, &Latest::Checking, true, false), None);
        assert_eq!(key_intent(Page::Failed, &found(), true, false), Some(Intent::Retry));
        assert_eq!(key_intent(Page::Done, &Latest::Checking, true, false), Some(Intent::Close));
        assert_eq!(key_intent(Page::Confirm, &found(), true, false), None, "uninstalling needs a click");
        assert_eq!(key_intent(Page::Working, &found(), true, false), None);
    }

    #[test]
    fn no_key_means_no_intent() {
        for page in PAGES {
            assert_eq!(key_intent(page, &found(), false, false), None, "{page:?}");
        }
    }

    #[test]
    fn bar_eases_up_and_drops_at_once() {
        assert_eq!(approach(0.25, 0.875, 0.125), 0.375);
        assert_eq!(approach(0.75, 0.875, 0.25), 0.875);
        assert_eq!(approach(0.875, 0.875, 0.125), 0.875);
        assert_eq!(approach(0.5, 0.0, 0.125), 0.0, "a restart shows at once");
        assert_eq!(approach(0.0, 1.0, 0.0), 0.0);
    }

    #[test]
    fn pages_are_listed_in_order() {
        assert_eq!(PAGES, [Page::Welcome, Page::Confirm, Page::Working, Page::Done, Page::Failed]);
    }
}
