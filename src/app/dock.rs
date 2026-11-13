//! notchbase dock from the app's side: off until someone turns it on, started with notchbase once it is, and introduced
//! once by a notice with Turn On and Not Now.

use std::time::{Duration, Instant};

use winit::window::WindowId;

use crate::bridge::DockChange;
use crate::config::{Config, DockConfig};
use crate::notch::Notch;
use crate::notch::notice::dock_intro::SLOT;
use crate::system::dock::SUPPORTED;

use super::App;
use super::preview::previewing;

/// How long the notch stays quiet, after the walkthrough and any startup notice, before the introduction shows.
const QUIET_FOR: Duration = Duration::from_secs(4);
const FIND_IT: &str = "Turn it on any time in Settings \u{203a} Dock";
const NOT_SAVED: &str = "Couldn't save the dock setting";

pub(super) struct DockIntro {
    pending: bool,
    /// The first frame's look at the dock: starting it when it is on, or adopting one already running.
    settled: bool,
    quiet_since: Option<Instant>,
}

impl DockIntro {
    pub(super) fn at_start(config: &Config, config_reads: bool) -> Self {
        Self::armed(&config.dock, config_reads, previewing(), SUPPORTED)
    }

    fn armed(dock: &DockConfig, config_reads: bool, previewing: bool, supported: bool) -> Self {
        let pending = supported && config_reads && !previewing && !dock.introduced && !dock.enabled;
        Self { pending, settled: previewing || !supported, quiet_since: None }
    }

    fn due(&mut self, quiet: bool, now: Instant) -> bool {
        if !self.pending || !quiet {
            self.quiet_since = None;
            return false;
        }
        now.duration_since(*self.quiet_since.get_or_insert(now)) >= QUIET_FOR
    }

    fn wakeup(&self) -> Option<Instant> {
        self.quiet_since.filter(|_| self.pending).map(|since| since + QUIET_FOR)
    }
}

/// What the first frame does with the dock: start it because it was turned on, or, when it is already running
/// without the setting (someone who used it before the setting existed), keep it on and skip the introduction.
#[derive(Debug, PartialEq, Eq)]
enum AtStart {
    Nothing,
    Start,
    Adopt,
}

fn at_start(dock: &DockConfig, running: bool) -> AtStart {
    match (dock.enabled, running) {
        (true, false) => AtStart::Start,
        (false, true) => AtStart::Adopt,
        _ => AtStart::Nothing,
    }
}

impl App {
    pub(super) fn follow_dock_intro(&mut self, now: Instant) {
        if !self.dock_intro.settled {
            self.dock_intro.settled = true;
            self.settle_dock();
        }
        let quiet = self.quiet_for_dock_intro();
        if self.dock_intro.due(quiet, now) {
            self.dock_intro.pending = false;
            self.remember_dock(|dock| dock.introduced = true);
            self.update_notch(Notch::introduce_dock);
        }
    }

    pub(super) fn dock_intro_wakeup(&self) -> Option<Instant> {
        self.dock_intro.wakeup()
    }

    fn settle_dock(&mut self) {
        let running = self.panel.dock.running_now();
        match at_start(&self.config.config().dock, running) {
            AtStart::Start => self.panel.dock.apply(&DockChange::Running(true)),
            AtStart::Adopt => {
                self.dock_intro.pending = false;
                self.remember_dock(|dock| (dock.enabled, dock.introduced) = (true, true));
            }
            AtStart::Nothing => {}
        }
        // Updated while the dock kept running (it's a separate app): swap it for the copy that came with this
        // version. Once per launch, and only for the dock inside this notchbase.app, so it can't go round in circles.
        if running && self.panel.dock.outdated() {
            self.panel.dock.restart();
        }
    }

    /// Settings › Dock: the switch also turns the setting on or off, so notchbase starts it next time or never does.
    pub(super) fn change_dock(&mut self, change: &DockChange) {
        if let DockChange::Running(on) = *change {
            self.dock_intro.pending = false;
            self.remember_dock(|dock| (dock.enabled, dock.introduced) = (on, true));
        }
        self.panel.dock.apply(change);
    }

    /// Turn On opens Settings › Dock, which says what macOS will ask for before its switch starts the dock. Not Now
    /// says where to find it. Either way the notice never comes back.
    pub(super) fn answer_dock_intro(&mut self, turn_on: bool, sender: Option<WindowId>) {
        self.dock_intro.pending = false;
        self.remember_dock(|dock| dock.introduced = true);
        if !turn_on {
            self.update_notch(Notch::close_dock_intro);
            self.notify(FIND_IT);
            return;
        }
        self.update_notch(|notch| notch.dismiss_notice(SLOT));
        self.panel.dock.reload_now();
        let window = match sender {
            Some(id) => self.notches.window_mut(id),
            None => self.notches.active_mut(),
        };
        if let Some(nw) = window {
            nw.notch.open_dock_settings();
            nw.redraw();
        }
    }

    fn quiet_for_dock_intro(&self) -> bool {
        !self.panel.welcome.due
            && self.notches.all_collapsed()
            && !self.notches.iter().any(|nw| nw.notch.shows_notice())
            && !self.screen_locked()
            && !self.capturing()
            && self.pending_capture.is_none()
            && !self.recorder.is_busy()
            && self.keys.is_none()
            && !self.typing
    }

    fn remember_dock(&mut self, change: impl FnOnce(&mut DockConfig)) {
        if previewing() {
            return;
        }
        if let Err(err) = self.config.update(|config| change(&mut config.dock)) {
            self.report(NOT_SAVED, err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn dock(enabled: bool, introduced: bool) -> DockConfig {
        DockConfig { enabled, introduced }
    }

    #[test]
    fn off_by_default() {
        let config = Config::default();
        assert_eq!(config.dock, dock(false, false), "nobody gets the dock without turning it on");
        let read: Config = serde_json::from_str("{}").unwrap();
        assert!(!read.dock.enabled && !read.dock.introduced, "an older file without the key reads as off");
        assert_eq!(at_start(&config.dock, false), AtStart::Nothing, "notchbase never starts it unasked");
    }

    #[test]
    fn starts_only_once_turned_on() {
        assert_eq!(at_start(&dock(true, true), false), AtStart::Start);
        assert_eq!(at_start(&dock(true, true), true), AtStart::Nothing, "already running");
        assert_eq!(at_start(&dock(false, true), false), AtStart::Nothing, "turned off stays off");
    }

    #[test]
    fn a_dock_already_running_is_kept() {
        assert_eq!(at_start(&dock(false, false), true), AtStart::Adopt);
    }

    #[test]
    fn introduced_once() {
        assert!(DockIntro::armed(&dock(false, false), true, false, true).pending);
        assert!(!DockIntro::armed(&dock(false, true), true, false, true).pending, "seen, after either answer");
        assert!(!DockIntro::armed(&dock(true, false), true, false, true).pending, "already on");
        assert!(!DockIntro::armed(&dock(false, false), false, false, true).pending, "a broken config can't remember");
        assert!(!DockIntro::armed(&dock(false, false), true, true, true).pending, "a preview never asks");
        assert!(!DockIntro::armed(&dock(false, false), true, false, false).pending, "no dock on this system");
    }

    #[test]
    fn seen_flag_round_trips() {
        let mut config = Config::default();
        config.dock.introduced = true;
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.dock, dock(false, true));
        assert!(!DockIntro::armed(&back.dock, true, false, true).pending);
    }

    #[test]
    fn waits_for_quiet() {
        let (mut intro, t0) = (DockIntro::armed(&dock(false, false), true, false, true), Instant::now());
        assert!(!intro.due(true, t0));
        assert_eq!(intro.wakeup(), Some(t0 + QUIET_FOR));
        assert!(!intro.due(false, t0 + SECOND * 2), "a notice or an open panel restarts the wait");
        assert_eq!(intro.wakeup(), None);
        let calm = t0 + SECOND * 3;
        assert!(!intro.due(true, calm));
        assert!(!intro.due(true, calm + QUIET_FOR - SECOND));
        assert!(intro.due(true, calm + QUIET_FOR));
    }

    #[test]
    fn answered_never_due() {
        let (mut intro, t0) = (DockIntro::armed(&dock(false, false), true, false, true), Instant::now());
        intro.pending = false;
        assert!(!intro.due(true, t0));
        assert!(!intro.due(true, t0 + QUIET_FOR * 3));
        assert_eq!(intro.wakeup(), None);
    }
}
