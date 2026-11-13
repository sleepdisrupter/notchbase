//! Every tool in the notch: the bundled ones and plugins from the tools folders. They start, get events and feeds,
//! and hand back their views through `ToolHost`.

mod bindings;
mod bundled;
mod host;
mod images;
pub mod outside;
mod plugin;
pub(crate) mod shortcuts;
mod system;
mod wasm;
mod watch;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use notchbase_sdk::Waker;
use notchbase_sdk::protocol::{
    AudioInput, CalendarFeed, Dirs, Event, Focus, Icon, Manifest, Node, NotchState, NowPlaying, Platform, Power,
    RecordingState, RecordingUpdate, Request, Sensors, Start, TileNode, Tone, Value, Values, Wants,
};

use crate::bridge::{ToolInfo, ToolSetting, ToolSlot, ToolView};
use crate::config::Config;
use crate::platform::{self, KnownDir};

pub use bindings::{Binding, bindings};
pub use host::ToolHost;
pub use images::{DOCK_PICTURE, ImageStore};

const TOOL_DATA: &str = "tools";
const LAUNCHER_ICON: f32 = 15.0;
const LAUNCHER_CHEVRON: f32 = 11.0;

/// The last value of each feed, so a tool that starts listening later gets it at once.
#[derive(Default)]
struct Feeds {
    recording: Option<RecordingState>,
    inputs: Option<Vec<AudioInput>>,
    media: Option<Option<NowPlaying>>,
    notch: Option<NotchState>,
    calendar: Option<CalendarFeed>,
    power: Option<Power>,
    locked: Option<bool>,
    sensors: Option<Sensors>,
    focus: Option<Focus>,
}

pub struct Tools {
    hosts: Vec<ToolHost>,
    views: Vec<ToolView>,
    catalog: Vec<ToolInfo>,
    seen: Option<Vec<(u64, bool)>>,
    feeds: Feeds,
    watch: watch::Watch,
    pub images: ImageStore,
}

type Found = (Box<dyn notchbase_sdk::Driver>, Manifest, Option<PathBuf>, Arc<AtomicBool>);

impl Tools {
    pub fn load(config: &Config, wake: &Waker) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut broken = Vec::new();
        let mut found: Vec<Found> = Vec::new();
        let dirs = platform::tools_dirs();
        let watch = watch::Watch::new(watch::stamps(&dirs));
        for construct in bundled::all() {
            let woken = Arc::new(AtomicBool::new(false));
            let driver = construct(tool_waker(&woken, wake));
            let manifest = driver.manifest();
            found.push((driver, manifest, None, woken));
        }
        for path in dirs.iter().flat_map(|dir| plugin::libraries(dir)) {
            let woken = Arc::new(AtomicBool::new(false));
            match open(&path, tool_waker(&woken, wake)) {
                Ok(driver) => {
                    let manifest = driver.manifest();
                    eprintln!("notchbase: loaded the {} tool from {}", manifest.id, path.display());
                    // A plugin with a bundled tool's id replaces it, and a later plugin replaces an earlier one.
                    found.retain(|(_, known, _, _)| known.id != manifest.id);
                    found.push((driver, manifest, Some(path), woken));
                }
                Err(err) => {
                    problems.push(format!("Couldn't load the tool at {}: {err}", path.display()));
                    broken.push(unloadable(&path, err));
                }
            }
        }
        found.sort_by_key(|(_, manifest, _, _)| rank(config, &manifest.id));
        let enabled = |manifest: &Manifest| config.tools.get(&manifest.id).is_none_or(|tool| tool.enabled);
        let mut catalog: Vec<ToolInfo> = found
            .iter()
            .map(|(_, manifest, location, _)| ToolInfo {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                location: location.as_ref().map(|path| path.to_string_lossy().into_owned()),
                enabled: enabled(manifest),
                problem: None,
                background: manifest.background,
            })
            .collect();
        catalog.extend(broken);
        let hosts = found
            .into_iter()
            .filter(|(_, manifest, _, _)| enabled(manifest))
            .enumerate()
            .map(|(index, (driver, manifest, _, woken))| {
                ToolHost::new(ToolSlot(index as u16 + 1), driver, manifest, woken)
            })
            .collect();
        let tools = Self {
            hosts,
            views: Vec::new(),
            catalog,
            seen: None,
            feeds: Feeds::default(),
            watch,
            images: ImageStore::default(),
        };
        (tools, problems)
    }

    pub fn reorder(&mut self, config: &Config) {
        self.hosts.sort_by_key(|host| rank(config, host.id()));
        self.catalog.sort_by_key(|info| rank(config, &info.id));
        self.seen = None;
    }

    /// The plugins rebuilt since loading, once their files stop changing.
    pub fn rebuilt_plugins(&mut self, now: Instant) -> Option<Vec<String>> {
        self.watch.changed(now, || watch::stamps(&platform::tools_dirs()))
    }

    pub fn catalog(&self) -> &[ToolInfo] {
        &self.catalog
    }

    pub fn start(&mut self, config: &Config, now: Instant) -> Vec<(ToolSlot, Request)> {
        let mut requests = Vec::new();
        for host in &mut self.hosts {
            let start = Start {
                settings: settings_of(config, host.id(), &host.manifest),
                dirs: dirs(host.id()),
                platform: Platform::current(),
                host_version: env!("CARGO_PKG_VERSION").to_owned(),
                clock_24h: platform::clock_24h(),
                time_zone: jiff::tz::TimeZone::system().iana_name().map(str::to_owned),
            };
            let slot = host.slot;
            requests.extend(host.start(start, now).into_iter().map(|request| (slot, request)));
        }
        self.catch_up();
        requests
    }

    pub fn hosts(&self) -> &[ToolHost] {
        &self.hosts
    }

    pub fn host(&self, slot: ToolSlot) -> Option<&ToolHost> {
        self.hosts.iter().find(|host| host.slot == slot)
    }

    pub fn setting(&self, slot: ToolSlot, key: &str) -> Option<&Value> {
        let view = self.views.iter().find(|view| view.slot == slot)?;
        view.settings.iter().find(|setting| setting.spec.key == key).map(|setting| &setting.value)
    }

    pub fn send(&mut self, slot: ToolSlot, event: Event) {
        if let Some(host) = self.hosts.iter_mut().find(|host| host.slot == slot) {
            host.queue(event);
        }
    }

    pub fn broadcast(&mut self, event: &Event, wanted: impl Fn(&Wants) -> bool) {
        for host in self.hosts.iter_mut().filter(|host| wanted(host.wants())) {
            host.queue(event.clone());
        }
    }

    pub fn configure(&mut self, config: &Config) {
        self.seen = None;
        for host in &mut self.hosts {
            let values = settings_of(config, host.id(), &host.manifest);
            host.queue(Event::Settings { values });
        }
    }

    pub fn feed_recording(&mut self, state: RecordingState) {
        if self.feeds.recording == Some(state) {
            return;
        }
        self.feeds.recording = Some(state);
        let event = Event::Recording { update: RecordingUpdate::State { state } };
        self.broadcast(&event, |wants| wants.recording);
    }

    pub fn announce_recording(&mut self, update: RecordingUpdate) {
        self.broadcast(&Event::Recording { update }, |wants| wants.recording);
    }

    pub fn feed_inputs(&mut self, inputs: Vec<AudioInput>) {
        if self.feeds.inputs.as_ref() == Some(&inputs) {
            return;
        }
        self.feeds.inputs = Some(inputs.clone());
        self.broadcast(&Event::AudioInputs { inputs }, |wants| wants.audio_inputs);
    }

    pub fn feed_media(&mut self, now_playing: Option<NowPlaying>) {
        if self.feeds.media.as_ref() == Some(&now_playing) {
            return;
        }
        self.feeds.media = Some(now_playing.clone());
        self.broadcast(&Event::Media { now_playing }, |wants| wants.media);
    }

    pub fn feed_notch(&mut self, state: NotchState) {
        if self.feeds.notch.as_ref() == Some(&state) {
            return;
        }
        self.feeds.notch = Some(state.clone());
        self.broadcast(&Event::Notch { state }, |wants| wants.notch);
    }

    pub fn feed_calendar(&mut self, feed: CalendarFeed) {
        if self.feeds.calendar.as_ref() == Some(&feed) {
            return;
        }
        self.feeds.calendar = Some(feed.clone());
        self.broadcast(&Event::Calendar { feed }, |wants| wants.calendar);
    }

    pub fn wants_calendar(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().calendar)
    }

    pub fn calendar_range(&self) -> Option<[u64; 2]> {
        self.hosts.iter().map(|host| host.wants().calendar_range).fold(None, notchbase_sdk::widest)
    }

    pub fn wants_audio_inputs(&self) -> bool {
        self.hosts.iter().any(|host| host.wants().audio_inputs)
    }

    pub fn pump(&mut self, now: Instant) -> Vec<(ToolSlot, Request)> {
        let mut requests = Vec::new();
        for host in &mut self.hosts {
            crate::stall::detail(host.id());
            let before = host.wants().clone();
            let slot = host.slot;
            requests.extend(host.pump(now).into_iter().map(|request| (slot, request)));
            feed_new_wants(&self.feeds, host, &before);
        }
        crate::stall::detail("");
        requests
    }

    /// Rebuilds the views when a tool's revision or failure changed. True if what's shown is different.
    pub fn refresh(&mut self, config: &Config) -> bool {
        let revisions: Vec<(u64, bool)> = self.hosts.iter().map(ToolHost::revision).collect();
        if self.seen.as_ref() == Some(&revisions) {
            return false;
        }
        self.seen = Some(revisions);
        let views: Vec<ToolView> = self.hosts.iter_mut().map(|host| view_of(host, config)).collect();
        if views == self.views {
            return false;
        }
        for view in &views {
            if let Some(info) = self.catalog.iter_mut().find(|info| info.id == view.id) {
                info.problem = view.failure.clone();
            }
        }
        self.views = views;
        true
    }

    pub fn views(&self) -> &[ToolView] {
        &self.views
    }

    pub fn next_wakeup(&self) -> Option<Instant> {
        self.hosts.iter().filter_map(ToolHost::next_wakeup).min()
    }

    pub fn shows_timers(&self) -> bool {
        let timed = |node: &Node| matches!(node, Node::Timer { .. });
        self.views.iter().any(|view| {
            view.tile
                .as_ref()
                .is_some_and(|tile| [&tile.accessory, &tile.footer].into_iter().flatten().any(|node| node.any(&timed)))
                || view.page.as_ref().is_some_and(|page| page.any(&timed))
        })
    }

    fn catch_up(&mut self) {
        for host in &mut self.hosts {
            feed_new_wants(&self.feeds, host, &Wants::default());
        }
    }
}

fn unloadable(path: &std::path::Path, problem: String) -> ToolInfo {
    let name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    ToolInfo {
        id: name.clone(),
        name,
        version: String::new(),
        location: Some(path.to_string_lossy().into_owned()),
        enabled: false,
        problem: Some(problem),
        background: false,
    }
}

fn feed_new_wants(feeds: &Feeds, host: &mut ToolHost, before: &Wants) {
    let now = host.wants().clone();
    if now.recording
        && !before.recording
        && let Some(state) = feeds.recording
    {
        host.queue(Event::Recording { update: RecordingUpdate::State { state } });
    }
    if now.audio_inputs
        && !before.audio_inputs
        && let Some(inputs) = &feeds.inputs
    {
        host.queue(Event::AudioInputs { inputs: inputs.clone() });
    }
    if now.media
        && !before.media
        && let Some(now_playing) = &feeds.media
    {
        host.queue(Event::Media { now_playing: now_playing.clone() });
    }
    if now.notch
        && !before.notch
        && let Some(state) = &feeds.notch
    {
        host.queue(Event::Notch { state: state.clone() });
    }
    if now.calendar
        && !before.calendar
        && let Some(feed) = &feeds.calendar
    {
        host.queue(Event::Calendar { feed: feed.clone() });
    }
    system::catch_up(feeds, host, before);
}

fn view_of(host: &mut ToolHost, config: &Config) -> ToolView {
    let values = settings_of(config, host.id(), &host.manifest);
    let settings = host
        .manifest
        .settings
        .iter()
        .map(|spec| ToolSetting {
            spec: spec.clone(),
            value: values.get(&spec.key).cloned().unwrap_or(spec.default.clone()),
        })
        .collect();
    let failure = host.failure().map(str::to_owned);
    let (slot, id, name) = (host.slot, host.id().to_owned(), host.manifest.name.clone());
    let views = host.views().clone();
    let tile = views.tile.or_else(|| launcher_tile(&host.manifest));
    ToolView { slot, id, name, tile, page: views.page, live: views.live, settings, failure }
}

/// A tool with a page but no tile gets a plain tile that opens the page, unless the page is a header tab.
fn launcher_tile(manifest: &Manifest) -> Option<TileNode> {
    let title = manifest.page.clone().filter(|_| !manifest.header_tab)?;
    let icon = Node::Icon { icon: manifest.page_icon.unwrap_or(Icon::Star), size: LAUNCHER_ICON, tone: Tone::Primary };
    let chevron = Node::Icon { icon: Icon::ChevronRight, size: LAUNCHER_CHEVRON, tone: Tone::Secondary };
    Some(TileNode {
        title,
        icon,
        well: None,
        tint: None,
        wide: false,
        enabled: true,
        on_press: None,
        shortcut: None,
        accessory: Some(chevron),
        footer: None,
    })
}

/// A tool's settings, saved ones over the manifest's defaults. Keep Awake's two switches live in the awake config.
pub fn settings_of(config: &Config, id: &str, manifest: &Manifest) -> Values {
    let mut values = manifest.defaults();
    if let Some(tool) = config.tools.get(id) {
        values.extend(tool.settings.clone());
    }
    if id == keep_awake::ID {
        values.insert(keep_awake::FORCED.to_owned(), config.awake.force.into());
        values.insert(keep_awake::LID.to_owned(), config.awake.hold_lid_closed.into());
    }
    values
}

pub fn save_setting(config: &mut Config, id: String, key: String, value: Value) {
    if id == keep_awake::ID && key == keep_awake::LID {
        config.awake.hold_lid_closed = value.as_bool().unwrap_or(config.awake.hold_lid_closed);
        return;
    }
    config.tools.entry(id).or_default().settings.insert(key, value);
}

fn open(path: &std::path::Path, waker: Waker) -> Result<Box<dyn notchbase_sdk::Driver>, String> {
    if path.extension().is_some_and(|extension| extension == wasm::EXTENSION) {
        return Ok(Box::new(wasm::WasmDriver::load(path)?));
    }
    Ok(Box::new(plugin::PluginDriver::load(path, waker)?))
}

fn rank(config: &Config, id: &str) -> usize {
    config.layout.tools.iter().position(|listed| listed == id).unwrap_or(usize::MAX)
}

/// Marks the tool as woken, so its next pump runs even with no events, then wakes the app.
fn tool_waker(woken: &Arc<AtomicBool>, wake: &Waker) -> Waker {
    let (woken, wake) = (woken.clone(), wake.clone());
    Waker::new(move || {
        woken.store(true, Ordering::Release);
        wake.wake();
    })
}

pub fn data_dir(id: &str) -> Option<PathBuf> {
    platform::data_dir().map(|dir| dir.join(TOOL_DATA).join(id))
}

fn dirs(id: &str) -> Dirs {
    let text = |path: Option<PathBuf>| path.map(|path| path.to_string_lossy().into_owned());
    let data = data_dir(id);
    Dirs {
        data: text(data).unwrap_or_default(),
        home: text(platform::known_dir(KnownDir::Home)),
        pictures: text(platform::known_dir(KnownDir::Pictures)),
        videos: text(platform::known_dir(KnownDir::Videos)),
        desktop: text(platform::known_dir(KnownDir::Desktop)),
        downloads: text(platform::known_dir(KnownDir::Downloads)),
        documents: text(platform::known_dir(KnownDir::Documents)),
    }
}

#[cfg(test)]
mod tests;
