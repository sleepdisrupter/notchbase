//! The running app. `App` owns every service and the notch windows: the event loop hands it events, it
//! fills the panel each frame and carries out the actions the notch sends back. Each file here adds one
//! `impl App` block for its area, and the folders hold the bigger features.

mod actions;
mod annotate;
mod assistant;
mod capture;
mod clipboard;
mod config;
mod dock;
mod events;
mod find;
mod frame;
mod handler;
mod hotkeys;
mod instance;
mod notches;
mod notifications;
mod now_playing;
mod panel;
mod preview;
mod return_to;
mod setup;
mod shelf;
mod snap;
mod system;
mod tools;
mod updates;
mod videos;
mod welcome;
mod whats_new;

use std::collections::BTreeSet;
use std::time::Instant;

use notchbase_sdk::Waker;
use notchbase_sdk::protocol::{Event, RecordingOptions};
use winit::event_loop::{EventLoop, EventLoopProxy};

use crate::assistant::Assistant;
use crate::bridge::{Locations, Tab, ToolSlot};
use crate::capture::color::Sampler;
use crate::capture::region::{AreaPicker, Picker};
use crate::config::{ConfigStore, LEGACY_FILE};
use crate::filedrag::Inbox as DropInbox;
use crate::installer::Arrival;
use crate::media::Media;
use crate::platform::{self, InstanceLock, KnownDir};
use crate::poll::Mailbox;
use crate::record::Recorder;
use crate::system::bluetooth::Bluetooth;
use crate::system::summon::Summon;
use crate::system::volume::Volume;
use crate::tools::shortcuts::ShortcutRef;
use crate::tools::{Binding, Tools};

use capture::{CaptureReply, PendingCapture};
use clipboard::Clipboard;
use events::UserEvent;
use hotkeys::Hotkeys;
use notches::Notches;
use panel::Panel;
use return_to::ReturnTo;
use setup::Setup;
use shelf::drops::Drops;
use system::System;
use updates::Updates;

pub use instance::{claim_instance, step_aside};

type Replies = Mailbox<(ToolSlot, Event)>;

pub fn run(arrival: Arrival, instance: InstanceLock) {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    platform::setup_event_loop(&mut builder);
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(err) => {
            eprintln!("notchbase: could not start: {err}");
            return;
        }
    };
    platform::let_hooks_hear_our_windows(&event_loop);
    let quit = events::sender(&event_loop.create_proxy());
    platform::on_console_quit(move || quit(UserEvent::Quit));
    let nudge = events::wake(&event_loop.create_proxy());
    crate::stall::watch(move || nudge());
    let mut app = App::new(event_loop.create_proxy(), arrival, instance);
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("notchbase: {err}");
    }
    app.finish_annotating();
    drop(app);
    platform::exit_now();
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    notches: Notches,
    picker: Picker,
    sampler: Sampler,
    framer: AreaPicker,
    recorder: Recorder,
    recording_dim: capture::dim::DimState,
    media: Media,
    volume: Volume,
    summon: Summon,
    bluetooth: Bluetooth,
    calendar: Option<crate::system::calendar::SystemCalendar>,
    system: System,
    volume_quiet_until: Option<Instant>,
    previewed_recording: Option<Instant>,
    pending_capture: Option<PendingCapture>,
    capture_reply: Option<CaptureReply>,
    recording_options: Option<RecordingOptions>,
    config: ConfigStore,
    tools: Tools,
    replies: Replies,
    bindings: Vec<Binding>,
    tabs: Vec<Tab>,
    locations: Locations,
    hotkeys: Hotkeys,
    keys: Option<ShortcutRef>,
    typing: bool,
    held_by: BTreeSet<ToolSlot>,
    shortcut_problem: Option<String>,
    rearrange: bool,
    /// No screen was there at launch (the Mac was locked, or every display was off): the
    /// notch opens, and everything else starts, once one appears.
    waiting_for_screen: bool,
    timer_second: u64,
    startup_messages: Vec<String>,
    front_app: Option<ReturnTo>,
    panel: Panel,
    drops: Drops,
    drop_inbox: DropInbox,
    clipboard: Clipboard,
    inbox: Assistant,
    prompts_seen: u64,
    pinned_prompt: Option<u64>,
    shows_seen: u64,
    agents_seen: u64,
    agents: assistant::agents::Agents,
    airdrops: shelf::airdrop::AirDrops,
    find: find::Finding,
    seen: notifications::Seen,
    news: whats_new::News,
    welcome: welcome::Welcome,
    dock_intro: dock::DockIntro,
    updates: Updates,
    setup: Setup,
    snap: snap::SnapState,
    videos: videos::Grabbing,
    annotating: annotate::Annotating,
    ringer: platform::Ringer,
    _instance: InstanceLock,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>, arrival: Arrival, instance: InstanceLock) -> Self {
        let legacy = platform::data_dir().map(|dir| dir.join(LEGACY_FILE));
        let (config, config_problem) = ConfigStore::open(platform::config_file(), legacy);
        let news = whats_new::News::at_start(&config.config().updates.seen, config_problem.is_none());
        let (welcome, welcome_view) = welcome::Welcome::at_start(config.config(), config_problem.is_none());
        let dock_intro = dock::DockIntro::at_start(config.config(), config_problem.is_none());
        let wake = events::wake(&proxy);
        let tools_wake = wake.clone();
        let (tools, mut startup_messages) = Tools::load(config.config(), &Waker::new(move || tools_wake()));
        startup_messages.extend(config_problem);
        startup_messages.extend(whats_new::greeting(arrival, news.unseen()));
        let updates = Updates::start(wake.clone());
        let find = find::Finding::start(wake.clone(), config.config(), !preview::previewing());
        let summoned = events::sender(&proxy);
        let summon = Summon::start(std::sync::Arc::new(move || summoned(UserEvent::Summon)), wake.clone());
        let mut app = Self {
            notches: Notches::default(),
            picker: Picker::new(),
            sampler: Sampler::new(),
            framer: AreaPicker::for_areas(),
            recording_dim: capture::dim::DimState::default(),
            recorder: Recorder::new(wake.clone()),
            media: Media::start(),
            volume: Volume::start(false),
            summon,
            bluetooth: Bluetooth::start(),
            calendar: None,
            system: System::default(),
            volume_quiet_until: None,
            previewed_recording: None,
            pending_capture: None,
            capture_reply: None,
            recording_options: None,
            config,
            tools,
            replies: Replies::new(wake.clone()),
            bindings: Vec::new(),
            tabs: Tab::defaults(),
            locations: Locations {
                tools: platform::tools_dir().map(|dir| dir.to_string_lossy().into_owned()),
                config: platform::config_file().map(|file| file.to_string_lossy().into_owned()),
            },
            hotkeys: Hotkeys::new(),
            keys: None,
            typing: false,
            held_by: BTreeSet::new(),
            shortcut_problem: None,
            rearrange: false,
            waiting_for_screen: false,
            timer_second: 0,
            startup_messages,
            front_app: None,
            panel: Panel::new(&wake),
            drops: Drops::default(),
            drop_inbox: DropInbox::new(platform::known_dir(KnownDir::Downloads), {
                let wake = wake.clone();
                move || wake()
            }),
            clipboard: Clipboard::default(),
            inbox: Assistant::start(wake.clone()),
            prompts_seen: 0,
            pinned_prompt: None,
            shows_seen: 0,
            agents_seen: 0,
            agents: assistant::agents::Agents::default(),
            airdrops: shelf::airdrop::AirDrops::new(wake.clone()),
            find,
            seen: notifications::Seen::default(),
            news,
            welcome,
            dock_intro,
            updates,
            setup: Setup::read(),
            snap: snap::SnapState::default(),
            videos: videos::Grabbing::new(wake.clone()),
            annotating: annotate::Annotating::new(wake),
            ringer: platform::Ringer::default(),
            _instance: instance,
            proxy,
        };
        app.panel.welcome = welcome_view;
        crate::filedrag::prefer_pages(videos::wanted);
        app.adopt_tools();
        app.start_volume_indicator();
        app.follow_system_key();
        app
    }

    fn remember_front_app(&mut self) {
        self.front_app = ReturnTo::remembered(self.annotating.keyboard(), platform::remember_front_app);
    }

    fn restore_front_app(&mut self) {
        if let Some(remembered) = self.front_app.take() {
            self.hand_focus_back(remembered);
        }
    }

    /// Something notchbase just opened has the keyboard now, so leaving the notch must not hand it back.
    fn forget_the_way_back(&mut self) {
        self.front_app = None;
        self.notches.windows_mut().for_each(|(_, nw)| nw.forget_hover_app());
    }

    fn hand_focus_back(&mut self, remembered: ReturnTo) {
        match remembered.resolved(self.annotating.keyboard(), platform::remember_front_app) {
            Some(ReturnTo::App(app)) => platform::restore_front_app(app),
            Some(ReturnTo::Annotator) => self.annotating.take_back_the_keyboard(),
            None => {}
        }
    }
}
