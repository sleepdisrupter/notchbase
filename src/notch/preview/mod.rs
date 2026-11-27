//! Sample states for NOTCHBASE_PREVIEW and the snapshot tests. Each preview pins the notch on a page, a notice or
//! a live activity, filled with sample data instead of the real services.

mod agents;
mod find;
mod stats;
mod welcome;
mod whats_new;

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage};
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::{LiveNode, NoticeNode};

use crate::assistant::{Offer, Provider, Question};
use crate::bridge::{
    Action, Activity, AgentLive, Ask, Choices, Mood, NotificationKind, NotificationView, Page, ToolSlot, VideoLive,
    VideoStage,
};
use crate::system::bluetooth::{Device, DeviceKind};

use super::activity::ActivityKind;
use super::activity::device::DeviceNotice;
use super::moves::transition::Motion;
use super::notice::ToolNotice;
use super::{Mode, Notch};

pub const SAMPLE_IMAGE: ImageId = ImageId(0);
const SAMPLE_COLOR: Color = Color::rgb(0x3A, 0x7B, 0xFF);
const SAMPLE_SLOT: ToolSlot = ToolSlot(0);
const SAMPLE_SETTINGS_SEARCH: &str = "screen";
const SAMPLE_MESSAGE: &str = "Allow Screen Recording for notchbase in System Settings";
const SAMPLE_SHOT: [u32; 2] = [2560, 1600];
const SAMPLE_SHOT_CONTROL: &str = "Annotate";
const SAMPLE_DROP_FILES: usize = 3;
const SAMPLE_RECORDING: &str = "0:42";
const SAMPLE_ELAPSED_MS: u64 = 42_000;
const SAMPLE_DEVICE: &str = "AirPods Pro";
const SAMPLE_BATTERY: u8 = 82;
const PINNED_FOR: Duration = Duration::from_secs(60 * 60 * 24 * 365);
const ENV_VAR: &str = "NOTCHBASE_PREVIEW";
const SAMPLE_PERMISSION: &str = "Run this command?";
const SAMPLE_COMMAND: &str = "cargo build --release && cargo test --quiet";
const SAMPLE_QUESTION: &str = "Everything passes. Should I publish the new release?";
const SAMPLE_OFFERS: [(&str, &str); 4] = [
    ("Publish now", "Tag 0.2.0 and upload"),
    ("Publish as beta", "Only testers get it"),
    ("Wait", "Keep it local for now"),
    ("Show me the diff", "Review before anything ships"),
];
const SAMPLE_FEATURES: [(&str, &str); 3] =
    [("Clipboard history", "Last 20 copies"), ("Calendar", "Next meeting"), ("Battery", "Charging alerts")];
const SAMPLE_PROJECT: &str = "notchbase";
const SAMPLE_RUN_MS: u64 = 3 * 60_000 + 12_000;
const SAMPLE_VIDEO: &str = "Me at the zoo";
const SAMPLE_VIDEO_PACE: &str = "3.1 MB/s \u{b7} 0:12 left";
const SAMPLE_VIDEO_SHARE: f32 = 0.62;
const SAMPLE_UPDATE: &str = "notchbase 0.5.0 is available";
const SAMPLE_UPDATE_DETAIL: &str = "It installs by itself when the notch is idle";
const SAMPLE_UPDATE_ACTION: &str = "Update Now";

/// A sample state, named by NOTCHBASE_PREVIEW or by a snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preview {
    Idle,
    Attracted,
    Peek,
    Expanded,
    Sound,
    Stats,
    StatsHistory,
    StatsApps,
    StatsDisk,
    Copied,
    Shot,
    Recording,
    Recorded,
    Alert,
    Playing,
    Docked,
    Loud,
    Media,
    MediaQuiet,
    MediaMixer,
    Volume,
    Brightness,
    Device,
    DeviceGone,
    Message,
    Shelf,
    Drop,
    Settings,
    SettingsSearch,
    Notifications,
    AssistantSettings,
    DockSettings,
    Catalog,
    Permission,
    Question,
    Choices,
    Working,
    Doing,
    Waiting,
    AgentDone,
    Sides,
    MusicSides,
    Agents,
    AgentsExpanded,
    AgentsWaiting,
    AgentsEmpty,
    AgentsMany,
    ShelfConvert,
    ShelfAirDrop,
    DockIntro,
    DockIntroOpen,
    Layouts,
    LayoutsStacked,
    VideoDrop,
    VideoLayouts,
    VideoFinding,
    VideoDownloading,
    VideoSaved,
    VideoSides,
    VideoPanel,
    Find,
    FindResults,
    FindActions,
    FindNone,
    FindIndexing,
    FindAccess,
    FindOff,
    WhatsNew,
    Updated,
    Welcome,
    WelcomeReady,
    WelcomeStep,
    WelcomeAsked,
    WelcomeAllowed,
    WelcomeDone,
}

impl Preview {
    pub(super) fn page(self) -> Option<Page> {
        Some(match self {
            Self::Sound => Page::Sound,
            Self::Stats | Self::StatsHistory | Self::StatsApps | Self::StatsDisk => Page::Stats,
            Self::Shelf | Self::ShelfConvert | Self::ShelfAirDrop => Page::Shelf,
            Self::Media | Self::MediaQuiet | Self::MediaMixer => Page::Media,
            Self::Agents | Self::AgentsExpanded | Self::AgentsWaiting | Self::AgentsEmpty | Self::AgentsMany => {
                Page::Agents
            }
            other => return find::page(other).or_else(|| whats_new::page(other)).or_else(|| welcome::page(other)),
        })
    }

    pub(super) fn from_env() -> Option<Self> {
        std::env::var(ENV_VAR).ok().and_then(|value| Self::parse(&value))
    }

    pub(super) fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "idle" => Self::Idle,
            "attracted" => Self::Attracted,
            "peek" => Self::Peek,
            "expanded" | "tools" | "recording-tools" | "shortcut" | "shortcut-taken" => Self::Expanded,
            "sound" => Self::Sound,
            "stats" => Self::Stats,
            "stats-history" | "usage-history" => Self::StatsHistory,
            "stats-apps" | "apps" => Self::StatsApps,
            "stats-disk" | "storage" => Self::StatsDisk,
            "copied" => Self::Copied,
            "shot" => Self::Shot,
            "recording" => Self::Recording,
            "recorded" => Self::Recorded,
            "alert" => Self::Alert,
            "playing" | "music" => Self::Playing,
            "docked" => Self::Docked,
            "loud" | "music-volume" => Self::Loud,
            "media" => Self::Media,
            "media-quiet" | "nothing-playing" => Self::MediaQuiet,
            "media-mixer" | "media-expanded" => Self::MediaMixer,
            "volume" => Self::Volume,
            "brightness" => Self::Brightness,
            "bluetooth" | "device" => Self::Device,
            "bluetooth-off" | "device-gone" => Self::DeviceGone,
            "message" => Self::Message,
            "shelf" => Self::Shelf,
            "drop" => Self::Drop,
            "layouts" | "snap" => Self::Layouts,
            "layouts-stacked" => Self::LayoutsStacked,
            "video-drop" => Self::VideoDrop,
            "video-layouts" => Self::VideoLayouts,
            "video-finding" => Self::VideoFinding,
            "video" | "video-downloading" => Self::VideoDownloading,
            "video-saved" => Self::VideoSaved,
            "video-sides" => Self::VideoSides,
            "video-panel" => Self::VideoPanel,
            "settings" => Self::Settings,
            "settings-search" | "search-settings" => Self::SettingsSearch,
            "notifications" | "update" => Self::Notifications,
            "assistant-settings" | "assistant" => Self::AssistantSettings,
            "dock-settings" | "dock" => Self::DockSettings,
            "catalog" | "installed-tools" => Self::Catalog,
            "permission" => Self::Permission,
            "question" => Self::Question,
            "choices" | "multi-select" => Self::Choices,
            "working" | "agent" | "thinking" => Self::Working,
            "doing" | "agent-doing" => Self::Doing,
            "needs-you" | "agent-waiting" => Self::Waiting,
            "agent-done" | "done" => Self::AgentDone,
            "sides" | "activities" => Self::Sides,
            "music-sides" => Self::MusicSides,
            "agents" => Self::Agents,
            "agents-expanded" => Self::AgentsExpanded,
            "agents-waiting" => Self::AgentsWaiting,
            "agents-empty" => Self::AgentsEmpty,
            "agents-many" | "agents-overflow" => Self::AgentsMany,
            "shelf-convert" | "convert" => Self::ShelfConvert,
            "shelf-airdrop" | "airdrop" => Self::ShelfAirDrop,
            "dock-intro" | "dock-notice" => Self::DockIntro,
            "dock-intro-open" | "dock-card" => Self::DockIntroOpen,
            other => return find::parse(other).or_else(|| whats_new::parse(other)).or_else(|| welcome::parse(other)),
        })
    }
}

impl Notch {
    pub(super) fn pin_preview(&mut self, preview: Preview) {
        if preview != Preview::Idle {
            self.attract.snap_to(1.0);
        }
        match preview {
            Preview::Idle | Preview::Attracted => {}
            Preview::Find
            | Preview::FindResults
            | Preview::FindActions
            | Preview::FindNone
            | Preview::FindIndexing
            | Preview::FindAccess
            | Preview::FindOff => find::pin(self, preview),
            Preview::WhatsNew | Preview::Updated => whats_new::pin(self, preview),
            Preview::Welcome
            | Preview::WelcomeReady
            | Preview::WelcomeStep
            | Preview::WelcomeAsked
            | Preview::WelcomeAllowed
            | Preview::WelcomeDone => welcome::pin(self, preview),
            Preview::Recording => {
                self.set_activities(vec![sample_recording()]);
            }
            Preview::Peek => self.mode = Mode::Peek,
            Preview::Expanded => self.mode = Mode::Expanded,
            Preview::Sound
            | Preview::Stats
            | Preview::StatsHistory
            | Preview::StatsApps
            | Preview::StatsDisk
            | Preview::Shelf
            | Preview::MediaQuiet
            | Preview::Agents
            | Preview::AgentsWaiting
            | Preview::AgentsEmpty
            | Preview::AgentsMany => self.pin_page(preview.page().unwrap_or(Page::Tools)),
            Preview::Copied => self.pin_notice(sample_copied()),
            Preview::Shot => self.pin_notice(sample_shot()),
            Preview::Recorded => self.pin_notice(sample_recorded()),
            Preview::Alert => self.pin_notice(sample_alert()),
            Preview::Playing => self.pin_sample_music(false),
            Preview::Docked => self.pin_sample_music(true),
            Preview::Loud => self.pin_sample_music_with_volume(),
            Preview::Volume => {
                let kind = self.pin_sample_volume();
                self.pin_activity(kind);
            }
            Preview::Brightness => {
                let kind = self.pin_sample_brightness();
                self.pin_activity(kind);
            }
            Preview::Media => {
                self.pin_sample_music(false);
                self.pin_page(Page::Media);
            }
            Preview::AgentsExpanded => {
                self.pin_page(Page::Agents);
                self.open_first_agent_row();
            }
            Preview::MediaMixer => {
                self.pin_sample_music(false);
                self.pin_page(Page::Media);
                self.pages.unfold_mixer();
            }
            Preview::Device => self.pin_activity(ActivityKind::Device(sample_device(true))),
            Preview::DeviceGone => self.pin_activity(ActivityKind::Device(sample_device(false))),
            Preview::Message => self.pin_activity(ActivityKind::Message(SAMPLE_MESSAGE.to_owned())),
            Preview::Drop => {
                self.mode = Mode::Drop;
                self.drop_files = SAMPLE_DROP_FILES;
            }
            Preview::Layouts => {
                self.mode = Mode::Drop;
                self.picking = true;
                self.usual_pick = Some(super::drop_zone::layouts::Pick::Zone(crate::snap::Zone::Left));
            }
            Preview::LayoutsStacked => {
                self.mode = Mode::Drop;
                self.picking = true;
                self.hot_pick = Some(super::drop_zone::layouts::Pick::Zone(crate::snap::Zone::MiddleThird));
            }
            Preview::VideoDrop => {
                self.mode = Mode::Drop;
                self.drop_video = Some(crate::webvideo::Site::YouTube);
                self.video_aim = Some(crate::webvideo::Kind::Video);
            }
            Preview::VideoLayouts => {
                self.mode = Mode::Drop;
                self.picking = true;
                self.drop_video = Some(crate::webvideo::Site::YouTube);
                self.video_aim = Some(crate::webvideo::Kind::Video);
            }
            Preview::VideoFinding => {
                self.set_activities(vec![sample_video(VideoStage::Finding, 0.0)]);
            }
            Preview::VideoDownloading => {
                self.set_activities(vec![sample_video(VideoStage::Downloading, SAMPLE_VIDEO_SHARE)]);
            }
            Preview::VideoSaved => {
                self.set_activities(vec![sample_video(VideoStage::Saved, 1.0)]);
            }
            Preview::VideoSides => {
                let first = match sample_video(VideoStage::Downloading, 0.3) {
                    Activity::Video(live) => {
                        Activity::Video(VideoLive { id: 2, site: crate::webvideo::Site::Instagram, ..live })
                    }
                    other => other,
                };
                let downloading = sample_video(VideoStage::Downloading, SAMPLE_VIDEO_SHARE);
                self.set_activities(vec![first, downloading, sample_recording()]);
            }
            Preview::VideoPanel => {
                let downloading = match sample_video(VideoStage::Downloading, SAMPLE_VIDEO_SHARE) {
                    Activity::Video(live) => {
                        Activity::Video(VideoLive { cover: Some(Arc::new(sample_artwork())), ..live })
                    }
                    other => other,
                };
                self.set_activities(vec![downloading]);
                self.pin_page(Page::Tools);
            }
            Preview::Settings => self.pin_page(Page::Settings),
            Preview::SettingsSearch => {
                self.pin_page(Page::Settings);
                self.pages.search_settings(SAMPLE_SETTINGS_SEARCH);
            }
            Preview::Notifications => self.pin_page(Page::Notifications),
            Preview::AssistantSettings => {
                self.pin_page(Page::Settings);
                self.pages.show_assistant_settings();
            }
            Preview::DockSettings => {
                self.pin_page(Page::Settings);
                self.pages.show_dock_settings();
            }
            Preview::Catalog => {
                self.pin_page(Page::Settings);
                self.pages.show_tool_list();
            }
            Preview::Permission => self.pin_ask(sample_ask(Choices::Permission { always: true })),
            Preview::Question => {
                let question = sample_question("Release", SAMPLE_QUESTION, &SAMPLE_OFFERS, false);
                self.pin_ask(sample_ask(Choices::Questions(vec![question])));
            }
            Preview::Choices => {
                let features =
                    sample_question("Next up", "Which features should I build next?", &SAMPLE_FEATURES, true);
                let release = sample_question("Release", SAMPLE_QUESTION, &SAMPLE_OFFERS[..2], false);
                self.pin_ask(sample_ask(Choices::Questions(vec![features, release])));
            }
            Preview::Working | Preview::Doing | Preview::Waiting => {
                let mood = match preview {
                    Preview::Doing => Mood::Doing("Editing"),
                    Preview::Waiting => Mood::Waiting,
                    _ => Mood::Thinking,
                };
                self.set_activities(vec![sample_agent(mood)]);
            }
            Preview::Sides => {
                let sides = vec![sample_agent(Mood::Doing("Editing")), sample_recording(), sample_timer()];
                self.set_activities(sides);
            }
            Preview::MusicSides => {
                self.pin_sample_music(true);
                self.set_activities(vec![sample_agent(Mood::Thinking), sample_recording()]);
            }
            Preview::ShelfConvert => {
                self.pin_page(Page::Shelf);
                self.pages.stage_formats();
            }
            Preview::ShelfAirDrop => {
                self.pin_page(Page::Shelf);
                self.pages.stage_airdrop();
            }
            Preview::DockIntro => self.pin_notice(super::notice::dock_intro::intro()),
            Preview::DockIntroOpen => self.pin_notice(super::notice::dock_intro::opened()),
            Preview::AgentDone => {
                let finished = crate::assistant::Finished {
                    id: 1,
                    provider: Provider::Claude,
                    project: SAMPLE_PROJECT.into(),
                    message: String::new(),
                    took: Duration::from_millis(SAMPLE_RUN_MS),
                    ended_ms: now_ms(),
                };
                self.pin_notice(super::live::agents::done(&finished));
            }
        }
        self.preview = Some(preview);
        self.retarget(Motion::Pop);
    }

    pub fn previewed_notifications(&self) -> Option<Vec<NotificationView>> {
        (self.preview == Some(Preview::Notifications)).then(|| vec![sample_update()])
    }

    fn pin_page(&mut self, page: Page) {
        self.mode = Mode::Expanded;
        self.pager.show(page);
    }

    fn pin_ask(&mut self, ask: Ask) {
        self.ask = Some(ask);
        self.card = true;
        self.mode = Mode::Ask;
    }

    pub(super) fn pin_notice(&mut self, notice: NoticeNode) {
        self.pin_activity(ActivityKind::Tool(ToolNotice::new(SAMPLE_SLOT, notice)));
    }

    fn pin_activity(&mut self, kind: ActivityKind) {
        self.activity.kind = Some(kind);
        self.activity.until = Some(Instant::now() + PINNED_FOR);
        self.mode = Mode::Activity;
    }
}

pub(super) fn sample_copied() -> NoticeNode {
    let swatches = adaptive(swatch(SAMPLE_COLOR).size(18.0), swatch(SAMPLE_COLOR).size(20.0));
    Notice::new(swatches, text(SAMPLE_COLOR.hex()).value()).pop().into_node()
}

fn sample_update() -> NotificationView {
    NotificationView {
        key: "update-preview".into(),
        kind: NotificationKind::Update,
        title: SAMPLE_UPDATE.into(),
        detail: SAMPLE_UPDATE_DETAIL.into(),
        progress: None,
        action: Some((SAMPLE_UPDATE_ACTION.into(), Action::UpdateNow)),
    }
}

fn sample_shot() -> NoticeNode {
    let [width, height] = SAMPLE_SHOT;
    let aspect = width as f32 / height as f32;
    let thumbnail = image(ImageSource::Capture(SAMPLE_IMAGE)).size(30.0, 30.0 / aspect).radius(5.0);
    let size = text(format!("{width}×{height}")).value().secondary();
    let island = row![text("Copied").callout(), size].spacing(6.0);
    let notch = icon(Icon::Checkmark).size(12.0).tone(Tone::Green);
    let shown = Notice::new(thumbnail, adaptive(notch, island)).pop().size(NoticeSize::Large).into_node();
    let annotate = notchbase_sdk::element::lower_static(button(SAMPLE_SHOT_CONTROL).on_press(()));
    NoticeNode { controls: Some(annotate), ..shown }
}

fn sample_recorded() -> NoticeNode {
    let island = row![text("Saved").callout(), text(SAMPLE_RECORDING).value().secondary()].spacing(6.0);
    let check = adaptive(badge(Icon::Checkmark, Tone::Green, 16.0), badge(Icon::Checkmark, Tone::Green, 18.0));
    let length = text(SAMPLE_RECORDING).value().secondary();
    Notice::new(check, adaptive(length, island)).pop().size(NoticeSize::Small).into_node()
}

fn sample_alert() -> NoticeNode {
    Notice::new(badge(Icon::Bell, Tone::Orange, 30.0), text("Deploy finished").headline())
        .below(text("notchbase 0.2 is live on all regions").callout().secondary().wrap())
        .banner()
        .shake()
        .into_node()
}

fn sample_live() -> LiveNode {
    let leading = notchbase_sdk::element::lower_static(dot(Tone::Red, 9.0));
    let trailing = notchbase_sdk::element::lower_static(timer(now_ms().saturating_sub(SAMPLE_ELAPSED_MS)));
    LiveNode { leading, trailing, controls: None, urgent: false }
}

fn sample_device(connected: bool) -> DeviceNotice {
    let battery = connected.then_some(SAMPLE_BATTERY);
    let device = Device { id: "sample".into(), name: SAMPLE_DEVICE.into(), kind: DeviceKind::Headphones, battery };
    DeviceNotice { device, connected }
}

fn sample_question(header: &str, text: &str, offers: &[(&str, &str)], multi: bool) -> Question {
    let offers =
        offers.iter().map(|(label, detail)| Offer { label: (*label).into(), detail: (*detail).into() }).collect();
    Question { header: header.into(), text: text.into(), offers, multi }
}

fn sample_recording() -> Activity {
    Activity::Tool(SAMPLE_SLOT, sample_live())
}

fn sample_timer() -> Activity {
    let leading = notchbase_sdk::element::lower_static(ring(0.62).size(13.0).tone(Tone::Orange));
    let trailing = notchbase_sdk::element::lower_static(timer(now_ms() + 4 * 60_000).remaining());
    Activity::Tool(ToolSlot(SAMPLE_SLOT.0 + 1), LiveNode { leading, trailing, controls: None, urgent: false })
}

fn sample_agent(mood: Mood) -> Activity {
    let since_ms = now_ms().saturating_sub(SAMPLE_RUN_MS);
    Activity::Agent(AgentLive { provider: Provider::Claude, mood, since_ms, count: 1 })
}

fn sample_ask(choices: Choices) -> Ask {
    let (title, detail) = match &choices {
        Choices::Permission { .. } => (SAMPLE_PERMISSION.to_owned(), Some(SAMPLE_COMMAND.to_owned())),
        Choices::Questions(questions) => (questions[0].text.clone(), None),
        Choices::Elsewhere => (SAMPLE_QUESTION.to_owned(), None),
    };
    Ask { id: 1, provider: Provider::Claude, context: "notchbase".to_owned(), title, detail, choices, waiting: 1 }
}

pub(super) fn sample_artwork() -> ColorImage {
    let side = 96;
    let pixels = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as f32 / side as f32, (i / side) as f32 / side as f32);
            let glow = (1.0 - ((x - 0.62).hypot(y - 0.38) * 2.2)).clamp(0.0, 1.0);
            let r = 40.0 + 200.0 * glow + 60.0 * y;
            let g = 30.0 + 90.0 * glow * glow;
            let b = 120.0 + 100.0 * (1.0 - y);
            Color32::from_rgb(r.min(255.0) as u8, g.min(255.0) as u8, b.min(255.0) as u8)
        })
        .collect();
    ColorImage::new([side, side], pixels)
}

pub fn sample_thumbnail() -> ColorImage {
    let (w, h) = (48, 30);
    let pixels = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            Color32::from_rgb((40.0 + 180.0 * x) as u8, (90.0 + 80.0 * y) as u8, 230)
        })
        .collect();
    ColorImage::new([w, h], pixels)
}

fn sample_video(stage: VideoStage, share: f32) -> Activity {
    Activity::Video(VideoLive {
        id: 1,
        site: crate::webvideo::Site::YouTube,
        audio: false,
        stage,
        share,
        title: SAMPLE_VIDEO.into(),
        detail: SAMPLE_VIDEO_PACE.into(),
        cover: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::geometry::TUCKED;
    use crate::notch::tests::fixtures::{AWAY, Rig, shortcuts};
    use crate::platform::Style;

    #[test]
    fn preview_pins_state() {
        assert_eq!(Preview::parse(" Expanded "), Some(Preview::Expanded));
        assert_eq!(Preview::parse("picker"), None);
        assert_eq!(Preview::parse("stats-history"), Some(Preview::StatsHistory));
        assert_eq!(Preview::parse("stats-apps"), Some(Preview::StatsApps));
        assert_eq!(Preview::parse("agents-expanded"), Some(Preview::AgentsExpanded));
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Copied));
        assert!(matches!(n.activity.kind, Some(ActivityKind::Tool(_))));
        assert_eq!(n.mode, Mode::Activity);
        assert!(!n.is_animating());
        assert!(!n.tick(Instant::now() + Duration::from_secs(10), Some(AWAY)));
        n.collapse();
        assert_eq!(n.mode, Mode::Activity);
        assert_eq!(n.next_wakeup(), None);
        let n = Notch::with_preview(Style::Island, None, Some(Preview::Stats));
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Stats));
        assert_eq!(Preview::parse("nothing-playing"), Some(Preview::MediaQuiet));
        let mut sound = Notch::with_preview(Style::Island, None, Some(Preview::Sound));
        let mut tabs = crate::bridge::Tab::defaults();
        tabs.pop();
        sound.sync_tabs(&tabs);
        assert_eq!(sound.pager.page, Page::Sound, "a previewed page outlives a tab change");
        let quiet = Notch::with_preview(Style::Island, None, Some(Preview::MediaQuiet));
        assert_eq!(quiet.pager.page, Page::Media);
        assert!(matches!(quiet.previewed_music(), Some(None)), "no song, even with one playing");
        let playing = Notch::with_preview(Style::Island, None, Some(Preview::Media));
        assert!(playing.previewed_music().is_some_and(|song| song.is_some()));
        assert_eq!(Preview::parse("media-expanded"), Some(Preview::MediaMixer));
        let mixer = Notch::with_preview(Style::Island, None, Some(Preview::MediaMixer));
        assert_eq!(mixer.pager.page, Page::Media);
        assert!(mixer.previewed_music().is_some_and(|song| song.is_some()));
        let notes = Notch::with_preview(Style::Island, None, Some(Preview::Notifications));
        assert_eq!((notes.mode, notes.pager.page), (Mode::Expanded, Page::Notifications));
        assert_eq!(notes.previewed_notifications().map(|list| list.len()), Some(1));
        assert_eq!(n.previewed_notifications(), None);
        let tucked = Notch::with_preview(Style::Island, None, Some(Preview::Idle));
        assert_eq!(tucked.silhouette().h, TUCKED.y);
        let out = Notch::with_preview(Style::Island, None, Some(Preview::Attracted));
        assert_eq!(out.silhouette().rect(), out.body_rect(out.idle_geometry()));
        assert!(!out.is_animating());
    }

    fn shelf_rig() -> Rig {
        let mut rig = Rig::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        rig.shelf.add(["Cargo.toml", "Cargo.lock", "rustfmt.toml"].map(|name| root.join(name)).to_vec());
        rig
    }

    #[test]
    fn shelf_previews_paint_both_styles() {
        for style in [Style::Island, Style::Notch] {
            let size = Some(egui::vec2(185.0, 32.0));
            for preview in [Preview::ShelfConvert, Preview::ShelfAirDrop] {
                let rig = shelf_rig();
                let mut n = Notch::with_preview(style, size, Some(preview));
                let keys = shortcuts(None);
                let actions: Vec<Action> = (0..3).flat_map(|_| rig.frame(&mut n, &keys, Vec::new())).collect();
                let sent = |action: &Action| matches!(action, Action::AirDrop(_));
                assert!(!actions.iter().any(sent), "{preview:?}");
                assert_eq!(n.visible_page(), Some(Page::Shelf));
            }
        }
    }
}
