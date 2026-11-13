//! The first-run walkthrough: which permission steps to show, and moving on as each one is answered.

use std::time::{Duration, Instant};

use winit::window::WindowId;

use crate::bridge::{Page, StepView, WelcomeStage, WelcomeView};
use crate::config::{Config, Walkthrough};
use crate::system::permissions::{self, Access, Prober, Standing};

use super::App;
use super::preview::previewing;

const LOOK_EVERY: Duration = Duration::from_millis(500);
const ADVANCE_AFTER: Duration = Duration::from_millis(900);
const NOT_SAVED: &str = "Couldn't save the setting";

type Look = fn(Access) -> Option<Standing>;

#[derive(Default)]
pub(super) struct Welcome {
    open: bool,
    looked_at: Option<Instant>,
    granted_at: Option<Instant>,
    prober: Prober,
}

impl Welcome {
    pub(super) fn at_start(config: &Config, config_reads: bool) -> (Self, WelcomeView) {
        Self::starting(config, config_reads && !previewing(), permissions::standing)
    }

    fn starting(config: &Config, config_reads: bool, look: Look) -> (Self, WelcomeView) {
        if !config_reads || config.permissions.walkthrough != Walkthrough::Pending {
            return (Self::default(), WelcomeView::default());
        }
        let steps = steps(|access| !config.permissions.was_asked(access.key()), look);
        let resumed = !config.permissions.asked.is_empty();
        let mut view = WelcomeView { due: true, first_run: true, steps, ..WelcomeView::default() };
        if resumed {
            view.stage = first_open(&view.steps, 0).map_or(WelcomeStage::AllSet, WelcomeStage::Step);
        }
        (Self { open: true, ..Self::default() }, view)
    }
}

fn steps(keep: impl Fn(Access) -> bool, look: Look) -> Vec<StepView> {
    let wanted = Access::ALL.into_iter().filter(|&access| keep(access));
    let standing = |access| look(access).map(|standing| StepView { access, standing, asked: false });
    wanted.filter_map(standing).filter(|step| step.standing != Standing::Granted).collect()
}

fn newest(fresh: Option<Standing>, here: Option<Standing>) -> Standing {
    if fresh == Some(Standing::Granted) || here == Some(Standing::Granted) {
        return Standing::Granted;
    }
    fresh.or(here).unwrap_or(Standing::Undecided)
}

fn first_open(steps: &[StepView], from: usize) -> Option<usize> {
    (from..steps.len()).find(|&index| steps[index].standing != Standing::Granted)
}

fn advanced(view: &WelcomeView) -> Option<WelcomeStage> {
    let from = match view.stage {
        WelcomeStage::Hello => 0,
        WelcomeStage::Step(index) => index + 1,
        WelcomeStage::AllSet => return None,
    };
    Some(first_open(&view.steps, from).map_or(WelcomeStage::AllSet, WelcomeStage::Step))
}

impl App {
    pub(super) fn open_welcome_at_start(&mut self) {
        if !self.panel.welcome.due {
            return;
        }
        if let Some(nw) = self.notches.active_mut() {
            nw.notch.open_page(Page::Welcome);
            nw.redraw();
        }
    }

    pub(super) fn show_permissions(&mut self, sender: Option<WindowId>) {
        if !(self.welcome.open && self.panel.welcome.first_run) {
            let mut view = WelcomeView { steps: steps(|_| true, permissions::standing), ..WelcomeView::default() };
            view.stage = first_open(&view.steps, 0).map_or(WelcomeStage::AllSet, WelcomeStage::Step);
            self.panel.welcome = view;
            self.welcome = Welcome { open: true, ..Welcome::default() };
        }
        let window = match sender {
            Some(id) => self.notches.window_mut(id),
            None => self.notches.active_mut(),
        };
        if let Some(nw) = window {
            nw.notch.open_page(Page::Welcome);
            nw.redraw();
        }
    }

    pub(super) fn follow_welcome(&mut self, now: Instant) {
        if !self.welcome.open {
            return;
        }
        let in_view = self.welcome_in_view();
        self.welcome.prober.follow(in_view && permissions::SUPPORTED);
        if !in_view {
            self.welcome.looked_at = None;
            return;
        }
        if self.welcome.looked_at.is_none_or(|at| now.duration_since(at) >= LOOK_EVERY) {
            self.welcome.looked_at = Some(now);
            self.look_again(now);
        }
        let granted = self.panel.welcome.step().is_some_and(|step| step.standing == Standing::Granted);
        if !granted {
            self.welcome.granted_at = None;
            return;
        }
        let since = *self.welcome.granted_at.get_or_insert(now);
        if now.duration_since(since) >= ADVANCE_AFTER {
            self.welcome_next();
        }
    }

    pub(super) fn welcome_wakeup(&self) -> Option<Instant> {
        if !self.welcome.open || !self.welcome_in_view() {
            return None;
        }
        let look = self.welcome.looked_at.map(|at| at + LOOK_EVERY);
        let advance = self.welcome.granted_at.map(|at| at + ADVANCE_AFTER);
        look.into_iter().chain(advance).min()
    }

    pub(super) fn welcome_start(&mut self) {
        if self.panel.welcome.stage == WelcomeStage::Hello {
            self.move_on();
        }
    }

    pub(super) fn welcome_ask(&mut self, access: Access) {
        self.note_asked([access]);
        permissions::ask(access);
        if let Some(step) = self.panel.welcome.steps.iter_mut().find(|step| step.access == access) {
            step.asked = true;
        }
        self.welcome.looked_at = None;
        self.redraw_notch();
    }

    pub(super) fn welcome_next(&mut self) {
        if let Some(step) = self.panel.welcome.step() {
            self.note_asked([step.access]);
        }
        self.move_on();
    }

    pub(super) fn welcome_skip(&mut self) {
        let all: Vec<Access> = self.panel.welcome.steps.iter().map(|step| step.access).collect();
        self.note_asked(all);
        self.panel.welcome.stage = WelcomeStage::AllSet;
        self.welcome.granted_at = None;
        self.redraw_notch();
    }

    pub(super) fn welcome_done(&mut self) {
        if self.panel.welcome.first_run
            && let Err(err) = self.config.update(|config| config.permissions.walkthrough = Walkthrough::Done)
        {
            self.report(NOT_SAVED, err);
        }
        self.welcome = Welcome::default();
        self.panel.welcome.due = false;
        self.redraw_notch();
    }

    fn move_on(&mut self) {
        if let Some(stage) = advanced(&self.panel.welcome) {
            self.panel.welcome.stage = stage;
        }
        self.welcome.granted_at = None;
        self.welcome.looked_at = None;
        self.redraw_notch();
    }

    fn look_again(&mut self, now: Instant) {
        let fresh = self.welcome.prober.latest(now);
        let mut changed = false;
        for step in &mut self.panel.welcome.steps {
            let fresh = fresh.as_ref().and_then(|standings| standings.get(&step.access).copied());
            let standing = newest(fresh, permissions::standing(step.access));
            changed |= std::mem::replace(&mut step.standing, standing) != standing;
        }
        if changed {
            self.redraw_notch();
        }
    }

    fn note_asked(&mut self, asked: impl IntoIterator<Item = Access>) {
        let keys: Vec<&str> = asked.into_iter().map(Access::key).collect();
        let result = self.config.update(|config| {
            for key in &keys {
                config.permissions.note_asked(key);
            }
        });
        if let Err(err) = result {
            self.report(NOT_SAVED, err);
        }
    }

    fn welcome_in_view(&self) -> bool {
        self.notches.iter().any(|nw| nw.notch.visible_page() == Some(Page::Welcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(access: Access, standing: Standing) -> StepView {
        StepView { access, standing, asked: false }
    }

    fn tour(stage: WelcomeStage) -> WelcomeView {
        let steps = vec![
            step(Access::Accessibility, Standing::Undecided),
            step(Access::Bluetooth, Standing::Granted),
            step(Access::Calendar, Standing::Denied),
        ];
        WelcomeView { due: true, first_run: true, stage, steps }
    }

    #[test]
    fn helper_answer_beats_memory() {
        use Standing::{Denied, Granted, Undecided};
        assert_eq!(newest(Some(Granted), Some(Undecided)), Granted, "macOS keeps the old answer cached in here");
        assert_eq!(newest(Some(Denied), Some(Undecided)), Denied);
        assert_eq!(newest(Some(Undecided), Some(Granted)), Granted, "the helper may be a second behind");
        assert_eq!(newest(None, Some(Denied)), Denied, "no helper yet: ask here");
        assert_eq!(newest(None, None), Undecided);
    }

    #[test]
    fn starts_at_first_open_step() {
        assert_eq!(advanced(&tour(WelcomeStage::Hello)), Some(WelcomeStage::Step(0)));
    }

    #[test]
    fn allowed_step_skipped() {
        assert_eq!(advanced(&tour(WelcomeStage::Step(0))), Some(WelcomeStage::Step(2)));
    }

    #[test]
    fn last_step_ends_all_set() {
        assert_eq!(advanced(&tour(WelcomeStage::Step(2))), Some(WelcomeStage::AllSet));
    }

    #[test]
    fn nothing_after_end() {
        assert_eq!(advanced(&tour(WelcomeStage::AllSet)), None);
    }

    #[test]
    fn all_allowed_goes_straight_on() {
        let view = WelcomeView { steps: Vec::new(), ..tour(WelcomeStage::Hello) };
        assert_eq!(advanced(&view), Some(WelcomeStage::AllSet));
    }

    fn new_mac(access: Access) -> Option<Standing> {
        match access {
            Access::Automation => None,
            Access::Bluetooth => Some(Standing::Granted),
            _ => Some(Standing::Undecided),
        }
    }

    #[test]
    fn only_new_installs_walk_through() {
        let fresh = Config::default();
        let (welcome, view) = Welcome::starting(&fresh, true, new_mac);
        assert!(welcome.open && view.due && view.first_run);
        assert_eq!(view.stage, WelcomeStage::Hello);
        let (welcome, view) = Welcome::starting(&fresh, false, new_mac);
        assert!(!welcome.open && !view.due, "a config with a mistake can't remember finishing it");
        let mut done = Config::default();
        done.permissions.walkthrough = Walkthrough::Done;
        let (welcome, view) = Welcome::starting(&done, true, new_mac);
        assert!(!welcome.open && !view.due);
    }

    #[test]
    fn steps_only_for_open_questions() {
        let (_, view) = Welcome::starting(&Config::default(), true, new_mac);
        let asked: Vec<Access> = view.steps.iter().map(|step| step.access).collect();
        let expected = [
            Access::Accessibility,
            Access::Calendar,
            Access::Downloads,
            Access::SystemAudio,
            Access::ScreenRecording,
            Access::FullDisk,
        ];
        assert_eq!(asked, expected, "Bluetooth is already on and no player is open");
        let (_, nothing) = Welcome::starting(&Config::default(), true, |_| Some(Standing::Granted));
        assert!(nothing.steps.is_empty(), "a computer that needs nothing still gets its hello");
        assert_eq!(nothing.stage, WelcomeStage::Hello);
    }

    #[test]
    fn relaunch_resumes_walkthrough() {
        let mut config = Config::default();
        config.permissions.note_asked(Access::Accessibility.key());
        let (_, view) = Welcome::starting(&config, true, new_mac);
        assert_eq!(view.stage, WelcomeStage::Step(0), "no second hello");
        assert_eq!(view.steps[0].access, Access::Calendar);
    }
}
