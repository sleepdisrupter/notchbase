use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Pos2, Rect, pos2};
use notchbase_sdk::protocol::{NoticeNode, Start};
use notchbase_sdk::{Driver, Notice, Runtime, Tool, Waker};

use crate::assistant::Provider;
use crate::audio::Mixer;
use crate::bridge::{
    Action, AgentsView, AppView, Locations, PanelData, Tab, ToolInfo, ToolSetting, ToolSlot, ToolView, UpdateView,
};
use crate::media::{NowPlaying, Timeline};
use crate::notch::Notch;
use crate::notch::geometry::CANVAS;
use crate::notch::notice::ToolNotice;
use crate::platform::Style;
use crate::shelf::{Notice as ShelfNotice, Shelf};
use crate::stats::Snapshot;
use crate::tools::ImageStore;
use crate::tools::shortcuts::{ShortcutRef, ShortcutView};
use crate::ui::font;

pub(in crate::notch) const OVER: Pos2 = pos2(340.0, 28.0);
pub(in crate::notch) const AWAY: Pos2 = pos2(40.0, 280.0);
const SHORTCUTS: [(u16, &str, &str); 3] =
    [(0, "pick", "Pick Color"), (1, "capture", "Screenshot"), (2, "record", "Record Screen")];

pub(in crate::notch) fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

pub(in crate::notch) fn island() -> Notch {
    Notch::with_preview(Style::Island, None, None)
}

pub(in crate::notch) fn notch() -> Notch {
    Notch::with_preview(Style::Notch, None, None)
}

pub(in crate::notch) fn settle(n: &mut Notch) {
    for _ in 0..600 {
        n.step(1.0 / 120.0);
    }
}

pub(in crate::notch) fn draw_out(n: &mut Notch, now: Instant, at: Pos2) {
    n.tick(now, Some(pos2(at.x, 0.0)));
    settle(n);
    n.tick(now, Some(at));
    settle(n);
}

pub(in crate::notch) const REST: Duration = Duration::from_millis(260);

pub(in crate::notch) struct DefaultConfigAfter;

impl Drop for DefaultConfigAfter {
    fn drop(&mut self) {
        crate::config::publish(crate::config::Config::default());
    }
}

pub(in crate::notch) fn hover_open_off() -> DefaultConfigAfter {
    let mut config = crate::config::Config::default();
    config.notch.open_on_hover = false;
    crate::config::publish(config);
    DefaultConfigAfter
}

pub(in crate::notch) fn wake_anywhere_near() -> DefaultConfigAfter {
    let mut config = crate::config::Config::default();
    config.notch.wake_at_edge = false;
    crate::config::publish(config);
    DefaultConfigAfter
}

pub(in crate::notch) fn peek(n: &mut Notch, now: Instant, at: Pos2) -> Instant {
    draw_out(n, now, at);
    n.tick(now, Some(at));
    let rested = now + REST;
    n.tick(rested, Some(at));
    rested
}

pub(in crate::notch) fn click(rig: &Rig, n: &mut Notch, at: Pos2) -> Vec<Action> {
    let keys = shortcuts(None);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut actions = rig.frame(n, &keys, vec![egui::Event::PointerMoved(at)]);
    actions.extend(rig.frame(n, &keys, vec![button(true)]));
    actions.extend(rig.frame(n, &keys, vec![button(false)]));
    actions
}

pub(in crate::notch) fn sample_notice(duration_ms: u64) -> ToolNotice {
    let notice: NoticeNode = Notice::new(notchbase_sdk::prelude::text("Hello"), notchbase_sdk::prelude::text("there"))
        .duration(Duration::from_millis(duration_ms))
        .into_node();
    ToolNotice::new(ToolSlot(0), notice)
}

pub(in crate::notch) fn tool_view<T: Tool>(slot: u16) -> ToolView {
    let mut runtime = Runtime::<T>::new(Waker::noop());
    runtime.start(Start::default());
    let views = runtime.views();
    let manifest = T::manifest();
    let settings =
        manifest.settings.iter().map(|spec| ToolSetting { value: spec.default.clone(), spec: spec.clone() }).collect();
    ToolView {
        slot: ToolSlot(slot),
        id: manifest.id,
        name: manifest.name,
        tile: views.tile,
        page: views.page,
        live: views.live,
        settings,
        failure: None,
    }
}

pub(in crate::notch) struct Rig {
    pub(in crate::notch) ctx: egui::Context,
    mixer: Mixer,
    stats: Snapshot,
    pub(in crate::notch) images: ImageStore,
    tabs: Vec<Tab>,
    catalog: Vec<ToolInfo>,
    locations: Locations,
    agents: AgentsView,
    pub(in crate::notch) find: crate::bridge::FindView,
    pub(in crate::notch) typing: bool,
    pub(in crate::notch) welcome: crate::bridge::WelcomeView,
    pub(in crate::notch) tools: Vec<ToolView>,
    pub(in crate::notch) shelf: Shelf,
    pub(in crate::notch) notice: Option<ShelfNotice>,
    pub(in crate::notch) news: bool,
}

impl Rig {
    pub(in crate::notch) fn new() -> Self {
        let ctx = egui::Context::default();
        font::install(&ctx);
        let shelf = Shelf::new(Arc::new(|| {}));
        let tools = vec![
            tool_view::<color_picker::ColorPicker>(0),
            tool_view::<::screenshot::Screenshot>(1),
            tool_view::<screen_recorder::ScreenRecorder>(2),
        ];
        Self {
            ctx,
            mixer: Mixer::new(),
            stats: Snapshot::default(),
            images: ImageStore::default(),
            tabs: Tab::defaults(),
            catalog: tools
                .iter()
                .map(|tool| ToolInfo {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    enabled: true,
                    ..ToolInfo::default()
                })
                .collect(),
            locations: Locations {
                tools: Some("C:/notchbase/tools".into()),
                config: Some("C:/notchbase/config.json".into()),
            },
            agents: AgentsView::default(),
            find: crate::bridge::FindView::default(),
            typing: false,
            welcome: crate::bridge::WelcomeView::default(),
            tools,
            shelf,
            notice: None,
            news: false,
        }
    }

    pub(super) fn with_every_tile() -> Self {
        let mut rig = Self::new();
        rig.tools.push(tool_view::<::timer::Timer>(3));
        rig.tools.push(tool_view::<keep_awake::KeepAwake>(4));
        rig
    }

    pub(in crate::notch) fn frame(
        &self,
        n: &mut Notch,
        shortcuts: &[ShortcutView],
        events: Vec<egui::Event>,
    ) -> Vec<Action> {
        self.frame_by(n, shortcuts, events, 1.0 / 60.0)
    }

    pub(in crate::notch) fn frame_by(
        &self,
        n: &mut Notch,
        shortcuts: &[ShortcutView],
        events: Vec<egui::Event>,
        dt: f32,
    ) -> Vec<Action> {
        let (actions, mut output) = self.run(n, shortcuts, events, dt);
        output.textures_delta.clear();
        let _ = self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        actions
    }

    pub(in crate::notch) fn run(
        &self,
        n: &mut Notch,
        shortcuts: &[ShortcutView],
        events: Vec<egui::Event>,
        dt: f32,
    ) -> (Vec<Action>, egui::FullOutput) {
        let input =
            egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, CANVAS)), events, ..Default::default() };
        let sample = n.previewed_agents();
        let stats = n.previewed_history().map(|history| {
            let (listing, apps) = n.previewed_apps().unzip();
            let hardware = apps.as_ref().map(|_| (0.62, 11 * (1 << 30)));
            let disk = n.previewed_storage();
            Snapshot {
                history,
                listing: listing.or(disk.as_ref().map(|_| crate::stats::Resource::Disk)),
                apps: apps.unwrap_or_default(),
                cpu: hardware.map(|(cpu, _)| cpu),
                mem_used: hardware.map(|(_, used)| used),
                mem_total: hardware.map(|_| 16 * (1 << 30)),
                disk_free: disk.as_ref().map(|(_, free, _)| *free),
                disk_total: disk.as_ref().map(|(_, _, total)| *total),
                storage: disk.map(|(storage, _, _)| storage).unwrap_or_default(),
                ..self.stats.clone()
            }
        });
        let song = n.previewed_music().flatten();
        let data = PanelData {
            agents: sample.as_ref().unwrap_or(&self.agents),
            stats: stats.as_ref().unwrap_or(&self.stats),
            now_playing: song.as_ref(),
            ..self.data(shortcuts)
        };
        let mut actions = Vec::new();
        let output = self.ctx.run_ui(input, |ui| actions = n.ui(ui, &data, dt));
        (actions, output)
    }

    pub(in crate::notch) fn data<'a>(&'a self, shortcuts: &'a [ShortcutView]) -> PanelData<'a> {
        PanelData {
            shortcuts,
            tabs: &self.tabs,
            tools: &self.tools,
            catalog: &self.catalog,
            locations: &self.locations,
            images: &self.images,
            mixer: &self.mixer,
            now_playing: None,
            stats: &self.stats,
            shelf: &self.shelf,
            shelf_notice: self.notice.as_ref(),
            agents: &self.agents,
            find: &self.find,
            welcome: &self.welcome,
            typing: self.typing,
            assistant: Some(Provider::Claude),
            questions_in_notch: true,
            progress_in_notch: true,
            volume_indicator: true,
            brightness_indicator: true,
            all_displays: false,
            wake_at_edge: true,
            flee_from_keys: true,
            haptics: true,
            dodge_menus: true,
            snap_windows: true,
            lock_screen: true,
            app: AppView {
                installed: true,
                can_install: true,
                open_at_login: true,
                updates: UpdateView { on: true, available: true, detail: "Up to date".into() },
                notifications: Vec::new(),
                unread: 0,
                news: self.news,
            },
            dock: &FIXTURE_DOCK,
        }
    }
}

const FIXTURE_DOCK: crate::bridge::DockView =
    crate::bridge::DockView { found: true, running: true, ..crate::bridge::DockView::DEFAULT };

pub(in crate::notch) fn shortcuts(recording: Option<usize>) -> Vec<ShortcutView> {
    SHORTCUTS
        .iter()
        .enumerate()
        .map(|(i, &(slot, id, title))| ShortcutView {
            which: ShortcutRef { tool: ToolSlot(slot), index: 0 },
            id: id.into(),
            title: title.into(),
            keys: vec!["Ctrl".into(), "Alt".into(), if i == 0 { "C" } else { "S" }.into()],
            recording: recording == Some(i),
            problem: None,
        })
        .collect()
}

pub(in crate::notch) fn song(playing: bool, position: f64) -> NowPlaying {
    NowPlaying {
        title: "Song".into(),
        artist: "Band".into(),
        artwork: None,
        playing,
        timeline: Timeline::new(position, 200.0, Instant::now()),
    }
}

pub(in crate::notch) fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}
