//! App updates: looks for a new version, downloads it, and installs it when the computer is idle or when asked.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::{Align, LiveNode, Node, TextNode, TextStyle, Tone};
use winit::event_loop::ActiveEventLoop;

use crate::bridge::{Action, Activity, AgentLive, NotificationKind, NotificationView, ToolSlot, UpdateView};
use crate::config::Config;
use crate::installer::Arrival;
use crate::poll::Wake;
use crate::update::{self, Status, Updater, Version};

use super::App;

const QUIET_FOR: Duration = Duration::from_secs(20);
const LOOK_EVERY: Duration = Duration::from_secs(60);
const SHOW_FINISHED: Duration = Duration::from_millis(900);
const RING: f32 = 13.0;
const UPDATER: ToolSlot = ToolSlot(u16::MAX);
const UPDATING: &str = "Updating";
const NOT_SAVED: &str = "Couldn't save the setting";
const NOT_INSTALLED: &str = "Couldn't install the update";
const NOT_UPDATED: &str = "Couldn't update notchbase";
const IDLE_INSTALL: &str = "It installs by itself when the notch is idle";
const WHENEVER: &str = "Update whenever you like";
const RESTART: &str = "notchbase restarts to finish";
const UPDATE_NOW: &str = "Update Now";
const RESTART_NOW: &str = "Restart Now";
const TRY_AGAIN: &str = "Try Again";

pub(super) struct Updates {
    updater: Updater,
    quiet_since: Option<Instant>,
    installing: Option<(Instant, PathBuf)>,
    shown: Option<LiveNode>,
    urgent: bool,
    announced: Option<Version>,
    looked_at: Option<Instant>,
}

impl Updates {
    pub(super) fn start(wake: Wake) -> Self {
        Self {
            updater: Updater::start(wake),
            quiet_since: None,
            installing: None,
            shown: None,
            urgent: false,
            announced: None,
            looked_at: None,
        }
    }

    pub(super) fn notification(&self, automatic: bool) -> Option<NotificationView> {
        let (key, title, detail, progress, action) = match self.updater.status() {
            Status::Available(version) => (
                format!("update-{version}"),
                format!("notchbase {version} is available"),
                if automatic { IDLE_INSTALL } else { WHENEVER }.to_owned(),
                None,
                Some(UPDATE_NOW),
            ),
            Status::Downloading(version, progress) => (
                format!("update-{version}"),
                format!("Downloading notchbase {version}"),
                format!("{}%", (progress * 100.0).round() as u32),
                Some(progress),
                None,
            ),
            Status::Ready(version, _) => (
                format!("update-{version}"),
                format!("notchbase {version} is ready"),
                if automatic { IDLE_INSTALL } else { RESTART }.to_owned(),
                None,
                Some(RESTART_NOW),
            ),
            Status::Failed(problem) => {
                ("update-failed".to_owned(), NOT_UPDATED.to_owned(), problem, None, Some(TRY_AGAIN))
            }
            _ => return None,
        };
        let action = action.map(|label| (label.to_owned(), Action::UpdateNow));
        Some(NotificationView { key, kind: NotificationKind::Update, title, detail, progress, action })
    }

    pub(super) fn view(&self, config: &Config) -> UpdateView {
        let status = self.updater.status();
        UpdateView {
            on: config.updates.automatic && status != Status::Unavailable,
            available: status != Status::Unavailable,
            detail: detail(&status),
        }
    }

    pub(super) fn live(&self) -> Option<(ToolSlot, LiveNode)> {
        self.shown.clone().map(|node| (UPDATER, node))
    }

    fn progress(&self) -> Option<f32> {
        match self.updater.status() {
            _ if self.installing.is_some() => Some(1.0),
            Status::Downloading(_, progress) => Some(progress),
            _ => None,
        }
    }

    fn deadline(&self) -> Option<Instant> {
        let waiting = matches!(self.updater.status(), Status::Available(_) | Status::Ready(..));
        let settles = self.quiet_since.filter(|_| waiting).map(|since| since + QUIET_FOR);
        [settles, self.installing.as_ref().map(|(at, _)| *at)].into_iter().flatten().min()
    }
}

impl App {
    pub(super) fn sync_updates(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let automatic = self.config.config().updates.automatic;
        let quiet = self.quiet_for_update();
        let updates = &mut self.updates;
        updates.quiet_since = quiet.then(|| updates.quiet_since.unwrap_or(now));
        if !quiet && !updates.urgent {
            updates.installing = None;
        }
        let idle = automatic && updates.quiet_since.is_some_and(|since| now >= since + QUIET_FOR);
        let settled = idle || updates.urgent;
        match updates.updater.status() {
            Status::Available(_) if settled => updates.updater.fetch(),
            Status::Failed(_) => updates.urgent = false,
            Status::Ready(_, path) if settled && updates.installing.is_none() => {
                updates.installing = Some((now + SHOW_FINISHED, path));
            }
            _ => {}
        }
        if let Some((at, path)) = updates.installing.clone()
            && now >= at
        {
            updates.installing = None;
            match update::install(&path) {
                Ok(()) => return event_loop.exit(),
                Err(err) => {
                    updates.updater.failed(NOT_INSTALLED.into());
                    self.report(NOT_INSTALLED, err);
                }
            }
        }
        self.show_update_progress();
        self.look_for_updates(now);
    }

    fn look_for_updates(&mut self, now: Instant) {
        let looking = self.visible_page().is_some();
        let updates = &mut self.updates;
        if looking && updates.looked_at.is_none_or(|at| now.duration_since(at) >= LOOK_EVERY) {
            updates.looked_at = Some(now);
            updates.updater.check_soon();
        }
        if let Status::Available(version) = updates.updater.status()
            && updates.announced != Some(version)
        {
            updates.announced = Some(version);
            self.notify(&format!("notchbase {version} is available"));
            self.redraw_notch();
        }
    }

    pub(super) fn update_wakeup(&self) -> Option<Instant> {
        self.updates.deadline()
    }

    pub(super) fn set_auto_update(&mut self, on: bool) {
        if let Err(err) = self.config.update(|config| config.updates.automatic = on) {
            self.report(NOT_SAVED, err);
        }
        self.updates.updater.check_soon();
        self.redraw_notch();
    }

    pub(super) fn follow_auto_update(&mut self, previous: &Config) {
        if previous.updates.automatic != self.config.config().updates.automatic {
            self.updates.updater.check_soon();
        }
    }

    pub(super) fn update_now(&mut self) {
        match self.updates.updater.status() {
            Status::Available(_) | Status::Downloading(..) | Status::Ready(..) => {
                self.updates.urgent = true;
                self.updates.updater.fetch();
                self.update_notch(|notch| notch.collapse());
            }
            _ => self.updates.updater.check_soon(),
        }
        self.redraw_notch();
    }

    pub(super) fn update_notification(&self) -> Option<NotificationView> {
        self.updates.notification(self.config.config().updates.automatic)
    }

    pub(super) fn activities(&self) -> Vec<Activity> {
        let tools = self.tools.views().iter().filter_map(|view| Some(Activity::Tool(view.slot, view.live.clone()?)));
        let updater = self.updates.live().map(|(slot, node)| Activity::Tool(slot, node));
        in_order(self.video_activities(), self.agents.live(), tools, updater)
    }

    pub(super) fn show_activities(&mut self) {
        let activities = self.activities();
        self.update_notch(|notch| {
            notch.set_activities(activities.clone());
        });
        self.redraw_notch();
    }

    fn show_update_progress(&mut self) {
        let shown = self.updates.progress().map(updating);
        if shown == self.updates.shown {
            return;
        }
        self.updates.shown = shown;
        self.show_activities();
    }

    fn quiet_for_update(&self) -> bool {
        self.visible_page().is_none()
            && self.keys.is_none()
            && !self.typing
            && !self.capturing()
            && self.pending_capture.is_none()
            && self.tools.views().iter().all(|view| view.live.is_none())
            && !self.agents_busy()
    }
}

pub(super) fn greeting(arrival: Arrival) -> Option<String> {
    match arrival {
        Arrival::Fresh => None,
        Arrival::Updated => Some(format!("Updated to notchbase {}", update::current_version())),
        Arrival::Installed if crate::installer::opens_at_login() => {
            Some("notchbase is installed and opens at login".into())
        }
        Arrival::Installed => Some("notchbase is installed".into()),
    }
}

fn in_order(
    videos: impl Iterator<Item = Activity>,
    agent: Option<AgentLive>,
    tools: impl Iterator<Item = Activity>,
    updater: Option<Activity>,
) -> Vec<Activity> {
    videos.chain(agent.map(Activity::Agent)).chain(tools).chain(updater).collect()
}

fn updating(progress: f32) -> LiveNode {
    let ring = Node::Ring { value: progress, size: RING, tone: Tone::White, span: None, stroke: None, center: None };
    let text = TextNode {
        text: UPDATING.into(),
        style: TextStyle::Body,
        tone: Tone::Secondary,
        wrap: false,
        align: Align::Start,
    };
    LiveNode { leading: ring, trailing: Node::Text(text), controls: None }
}

fn detail(status: &Status) -> String {
    let current = update::current_version();
    match status {
        Status::Unavailable => "Updates aren't available".into(),
        Status::Waiting => format!("Version {current}"),
        Status::Current => format!("Up to date \u{b7} {current}"),
        Status::Available(version) | Status::Ready(version, _) => {
            format!("{version} installs when the notch is idle")
        }
        Status::Downloading(version, progress) => {
            format!("Downloading {version} \u{b7} {}%", (progress * 100.0).round() as u32)
        }
        Status::Failed(problem) => problem.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_says_what_updater_does() {
        let next = Version::parse("9.1.0").unwrap();
        assert_eq!(detail(&Status::Downloading(next, 0.424)), "Downloading 9.1.0 \u{b7} 42%");
        assert_eq!(detail(&Status::Available(next)), "9.1.0 installs when the notch is idle");
        assert!(detail(&Status::Current).starts_with("Up to date"));
        assert_eq!(detail(&Status::Failed("Can't reach the update server".into())), "Can't reach the update server");
    }

    #[test]
    fn ring_fills_with_download() {
        let Node::Ring { value, .. } = updating(0.3).leading else { panic!("a ring") };
        assert_eq!(value, 0.3);
    }

    #[test]
    fn activities_in_order() {
        let mood = crate::bridge::Mood::Thinking;
        let agent = AgentLive { provider: crate::assistant::Provider::Claude, mood, since_ms: 0, count: 1 };
        let tools = [ToolSlot(1), ToolSlot(2)].map(|slot| Activity::Tool(slot, updating(0.5)));
        let download = crate::bridge::VideoLive {
            id: 1,
            site: crate::webvideo::Site::YouTube,
            audio: true,
            stage: crate::bridge::VideoStage::Finding,
            share: 0.0,
            title: String::new(),
            detail: String::new(),
            cover: None,
        };
        let updater = Activity::Tool(UPDATER, updating(0.5));
        let videos = std::iter::once(Activity::Video(download));
        let shown = in_order(videos, Some(agent), tools.into_iter(), Some(updater));
        assert!(matches!(
            shown.as_slice(),
            [
                Activity::Video(_),
                Activity::Agent(_),
                Activity::Tool(ToolSlot(1), _),
                Activity::Tool(ToolSlot(2), _),
                Activity::Tool(UPDATER, _)
            ]
        ));
    }

    #[test]
    fn launch_after_update_says_so() {
        assert!(greeting(Arrival::Updated).is_some_and(|text| text.starts_with("Updated to notchbase ")));
        assert!(greeting(Arrival::Installed).is_some());
        assert!(greeting(Arrival::Fresh).is_none());
    }
}
