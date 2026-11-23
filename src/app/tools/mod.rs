//! Tools on the app's side: loading them, their settings, tabs and layout, and the events they're sent.

mod feeds;
pub(super) mod requests;
mod shortcuts;

use std::path::PathBuf;
use std::time::Instant;

use notchbase_sdk::protocol::{Event, HandlerId, Icon, Manifest, Surface, UiValue, Value};
use winit::event_loop::ActiveEventLoop;

use crate::bridge::{Page, Tab, ToolSlot};
use crate::config::{CORE_TABS, Config, hotkey};
use crate::platform;
use crate::tools::{ToolHost, Tools, bindings};

use super::App;
use super::config::assistant_of;
use super::events::UserEvent;

const TOOL_PROBLEM: &str = "Couldn't save the tool setting";
const OPEN_PROBLEM: &str = "Couldn't open it";

impl App {
    pub(super) fn adopt_tools(&mut self) {
        let tools = &self.tools;
        let saved =
            self.config.update(|config| remember_tools(config, tools.hosts().iter().map(|host| &host.manifest)));
        if let Err(err) = saved {
            eprintln!("notchbase: couldn't add the tools to config.json: {err}");
        }
        self.bindings = bindings(&self.tools, self.config.config());
        self.tabs = tabs(self.config.config(), self.tools.hosts());
    }

    pub(super) fn start_tools(&mut self, event_loop: &ActiveEventLoop) {
        let requests = self.tools.start(self.config.config(), Instant::now());
        self.perform_all(event_loop, requests);
        self.refresh_tools();
    }

    pub(super) fn reload_tools(&mut self, event_loop: &ActiveEventLoop) {
        self.cancel_capture();
        if let Some(slot) = std::mem::take(&mut self.held_by).pop_first() {
            self.hold_for(slot, false);
        }
        self.let_computer_sleep();
        let wake = super::events::wake(&self.proxy);
        let wake = notchbase_sdk::Waker::new(move || wake());
        let (tools, problems) = Tools::load(self.config.config(), &wake);
        let images = std::mem::take(&mut self.tools.images);
        self.tools = tools;
        self.tools.images = images;
        for problem in problems {
            self.notify(&problem);
        }
        self.adopt_tools();
        self.register_shortcuts();
        self.start_tools(event_loop);
    }

    pub(super) fn follow_plugins(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let Some(names) = self.tools.rebuilt_plugins(now) else { return };
        self.reload_tools(event_loop);
        if !names.is_empty() {
            self.notify(&format!("Reloaded {}", names.join(", ")));
        }
    }

    pub(super) fn pump_tools(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        self.feed_tools();
        for (slot, event) in self.replies.take() {
            self.tools.send(slot, event);
        }
        let requests = self.tools.pump(now);
        self.perform_all(event_loop, requests);
        self.refresh_tools();
    }

    pub(super) fn tool_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        slot: ToolSlot,
        surface: Surface,
        handler: HandlerId,
        value: UiValue,
    ) {
        self.tools.send(slot, Event::Ui { surface, handler, value });
        self.pump_tools(event_loop, Instant::now());
    }

    pub(super) fn pick_tool_folder(&mut self, slot: ToolSlot, key: String) {
        let start =
            self.tools.setting(slot, &key).and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty());
        let start = start.map(PathBuf::from);
        let send = super::events::sender(&self.proxy);
        platform::pick_folder(start, move |picked| {
            if let Some(path) = picked {
                send(UserEvent::FolderPicked(slot, key, path));
            }
        });
    }

    pub(super) fn set_tool_setting(&mut self, slot: ToolSlot, key: String, value: Value) {
        let Some(id) = self.tools.host(slot).map(|host| host.id().to_owned()) else { return };
        let saved = self.config.update(|config| crate::tools::save_setting(config, id, key, value));
        if let Err(err) = saved {
            self.report(TOOL_PROBLEM, err);
        }
        self.tools.configure(self.config.config());
        self.redraw_notch();
    }

    pub(super) fn set_tool_enabled(&mut self, event_loop: &ActiveEventLoop, id: String, on: bool) {
        let saved = self.config.update(|config| config.tools.entry(id).or_default().enabled = on);
        if let Err(err) = saved {
            self.report(TOOL_PROBLEM, err);
        }
        self.reload_tools(event_loop);
    }

    pub(super) fn move_tool(&mut self, slot: ToolSlot, before: Option<ToolSlot>) {
        let id_of = |slot: ToolSlot| self.tools.host(slot).map(|host| host.id().to_owned());
        let Some(id) = id_of(slot) else { return };
        let before = before.and_then(id_of);
        let saved = self.config.update(|config| move_before(&mut config.layout.tools, &id, before.as_deref()));
        if let Err(err) = saved {
            self.report(TOOL_PROBLEM, err);
        }
        self.reorder_tools();
    }

    pub(super) fn reorder_tools(&mut self) {
        self.tools.reorder(self.config.config());
        self.refresh_tools();
        self.redraw_notch();
    }

    pub(super) fn open_path(&mut self, path: &str) {
        let target = std::path::Path::new(path);
        if target.extension().is_none() && !target.exists() {
            let _ = std::fs::create_dir_all(target);
        }
        if let Err(err) = crate::platform::open(path) {
            self.report(OPEN_PROBLEM, err);
        }
    }

    pub(super) fn tool_wakeup(&self) -> Option<Instant> {
        self.tools.next_wakeup()
    }

    fn refresh_tools(&mut self) {
        if !self.tools.refresh(self.config.config()) {
            return;
        }
        self.tabs = tabs(self.config.config(), self.tools.hosts());
        self.show_activities();
    }
}

fn move_before(listed: &mut Vec<String>, id: &str, before: Option<&str>) {
    listed.retain(|listed| listed != id);
    let at = before.and_then(|before| listed.iter().position(|listed| listed == before)).unwrap_or(listed.len());
    listed.insert(at, id.to_owned());
}

fn remember_tools<'a>(config: &mut Config, manifests: impl IntoIterator<Item = &'a Manifest>) {
    let manifests: Vec<&Manifest> = manifests.into_iter().collect();
    for manifest in &manifests {
        let id = manifest.id.clone();
        let entry = config.tools.entry(id.clone()).or_default();
        for spec in &manifest.settings {
            entry.settings.entry(spec.key.clone()).or_insert_with(|| spec.default.clone());
        }
        for spec in &manifest.shortcuts {
            let default = spec.default.as_deref().and_then(hotkey::parse).map(|hotkey| hotkey::format(&hotkey));
            entry.shortcuts.entry(spec.id.clone()).or_insert(default);
        }
        if !config.layout.tools.contains(&id) {
            config.layout.tools.push(id.clone());
        }
    }
    let known = |key: &str| config.tools.contains_key(key);
    let keeps = |key: &str| match manifests.iter().find(|manifest| manifest.id == key) {
        Some(manifest) => asks_for_tab(manifest),
        None => CORE_TABS.contains(&key) || known(key),
    };
    let kept: Vec<String> = config.layout.tabs.iter().filter(|key| keeps(key)).cloned().collect();
    config.layout.tabs = kept;
}

fn asks_for_tab(manifest: &Manifest) -> bool {
    manifest.header_tab && manifest.page.is_some()
}

pub(super) fn tabs(config: &Config, hosts: &[ToolHost]) -> Vec<Tab> {
    let tool = |host: &ToolHost| {
        let manifest = &host.manifest;
        let title = manifest.page.clone().filter(|_| manifest.header_tab)?;
        Some(Tab {
            page: Page::Tool(host.slot),
            key: manifest.id.clone(),
            title,
            icon: manifest.page_icon.unwrap_or(Icon::Star),
        })
    };
    let named = |key: &String| match CORE_TABS.contains(&key.as_str()) {
        true => Page::from_core_key(key).map(Tab::core),
        false => hosts.iter().find(|host| &host.manifest.id == key).and_then(tool),
    };
    let agents = assistant_of(config).is_some();
    let mut tabs: Vec<Tab> = Vec::new();
    for tab in config.layout.tabs.iter().filter_map(named).chain(hosts.iter().filter_map(tool)) {
        if (agents || tab.page != Page::Agents) && !tabs.iter().any(|placed| placed.key == tab.key) {
            tabs.push(tab);
        }
    }
    tabs
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::Waker;

    use super::*;
    use crate::config::DEFAULT_TABS;

    #[test]
    fn moved_tool_lands_before_target() {
        let listed = || ["a", "b", "c", "d"].map(str::to_owned).to_vec();
        let moved = |id: &str, before: Option<&str>| {
            let mut listed = listed();
            move_before(&mut listed, id, before);
            listed.join("")
        };
        assert_eq!(moved("d", Some("b")), "adbc");
        assert_eq!(moved("a", Some("d")), "bcad");
        assert_eq!(moved("b", None), "acdb", "nothing after it means the end");
        assert_eq!(moved("c", Some("gone")), "abdc");
        assert_eq!(moved("new", Some("a")), "newabcd", "a tool the list never had still gets its place");
    }

    #[test]
    fn new_tools_saved_once() {
        let mut config = Config::default();
        let (tools, _) = Tools::load(&config, &Waker::noop());
        let manifests = || tools.hosts().iter().map(|host| &host.manifest);
        remember_tools(&mut config, manifests());
        let listed = [
            "color-picker",
            "screenshot",
            "screen-recorder",
            "dictation",
            "timer",
            "keep-awake",
            "roam",
            "alerts",
            "calendar",
            "battery",
            "camera-mic",
            "downloads",
            "focus-status",
        ];
        assert_eq!(config.layout.tools, listed);
        assert_eq!(config.layout.tabs, DEFAULT_TABS, "the timer's page opens from its tile, so no tool adds a tab");
        let picker = &config.tools["color-picker"];
        assert_eq!(picker.shortcuts["pick"].as_deref(), Some("Ctrl+Shift+C"));
        assert_eq!(picker.settings["format"], Value::String("hex".into()));
        config.tools.get_mut("color-picker").unwrap().shortcuts.insert("pick".into(), None);
        remember_tools(&mut config, manifests());
        assert_eq!(config.tools["color-picker"].shortcuts["pick"], None, "a cleared shortcut stays cleared");
    }

    #[test]
    fn fresh_tools_need_no_reload() {
        let (mut tools, _) = Tools::load(&Config::default(), &Waker::noop());
        let t0 = Instant::now();
        for seconds in 0..4 {
            let now = t0 + std::time::Duration::from_secs(seconds);
            assert_eq!(tools.rebuilt_plugins(now), None, "a reload already covers what is in the folder");
        }
    }

    #[test]
    fn page_tools_stay_in_tools_tab() {
        let mut config = Config::default();
        config.layout.tabs.push("launcher".into());
        let notes = Manifest::new("notes", "Notes").page("Notes");
        remember_tools(&mut config, [&notes]);
        assert_eq!(config.layout.tabs, DEFAULT_TABS, "tool pages open from the Tools tab, never from the header");
    }

    #[test]
    fn tabs_follow_layout() {
        let mut config = Config::default();
        config.layout.tabs = vec!["stats".into(), "nope".into(), "settings".into(), "media".into()];
        let keys: Vec<String> = tabs(&config, &[]).into_iter().map(|tab| tab.key).collect();
        assert_eq!(keys, ["stats", "media"]);
    }

    #[test]
    fn asked_tab_after_core_tabs() {
        let config = Config::default();
        let (tools, _) = Tools::load(&config, &Waker::noop());
        let tabs = tabs(&config, tools.hosts());
        let keys: Vec<&str> = tabs.iter().map(|tab| tab.key.as_str()).collect();
        assert_eq!(keys, ["media", "tools", "shelf", "agents", "stats", "calendar"]);
        let calendar = tabs.last().unwrap();
        assert_eq!((calendar.title.as_str(), calendar.icon), ("Calendar", Icon::Calendar));
        assert!(matches!(calendar.page, Page::Tool(_)));
    }

    #[test]
    fn listed_tool_tab_keeps_place() {
        let mut config = Config::default();
        config.layout.tabs =
            ["calendar", "media", "tools", "calendar", "shelf", "sound", "stats"].map(String::from).to_vec();
        let (tools, _) = Tools::load(&config, &Waker::noop());
        let keys: Vec<String> = tabs(&config, tools.hosts()).into_iter().map(|tab| tab.key).collect();
        assert_eq!(keys, ["calendar", "media", "tools", "shelf", "sound", "stats"]);
        remember_tools(&mut config, tools.hosts().iter().map(|host| &host.manifest));
        assert_eq!(config.layout.tabs[0], "calendar", "a tool's tab position survives a restart");
        assert!(config.layout.tabs.contains(&"sound".to_owned()), "a listed Sound tab is kept");
    }

    #[test]
    fn agents_tab_needs_assistant() {
        let mut config = Config::default();
        let keys = |config: &Config| tabs(config, &[]).into_iter().map(|tab| tab.key).collect::<Vec<String>>();
        assert_eq!(keys(&config), ["media", "tools", "shelf", "agents", "stats"]);
        let agents = tabs(&config, &[]).into_iter().find(|tab| tab.page == Page::Agents).unwrap();
        assert_eq!((agents.title.as_str(), agents.icon), ("Agents", Icon::Spark));
        config.assistant.enabled = false;
        assert_eq!(keys(&config), ["media", "tools", "shelf", "stats"]);
        config.assistant.enabled = true;
        config.assistant.provider = "chatgpt".into();
        assert!(keys(&config).contains(&"agents".to_owned()));
    }

    #[test]
    fn default_header_skips_sound() {
        assert!(!DEFAULT_TABS.contains(&"sound"));
        assert!(CORE_TABS.contains(&"sound"));
        let keys: Vec<String> = Tab::defaults().into_iter().map(|tab| tab.key).collect();
        assert_eq!(keys, DEFAULT_TABS);
    }

    #[test]
    fn only_tab_tools_keep_place() {
        let mut config = Config::default();
        let (tools, _) = Tools::load(&config, &Waker::noop());
        remember_tools(&mut config, tools.hosts().iter().map(|host| &host.manifest));
        config.layout.tabs = ["timer", "stats", "calendar", "gone"].map(String::from).to_vec();
        config.tools.entry("gone".into()).or_default();
        remember_tools(&mut config, tools.hosts().iter().map(|host| &host.manifest));
        assert_eq!(
            config.layout.tabs,
            ["stats", "calendar", "gone"],
            "a tool that isn't loaded right now keeps its place"
        );
    }
}
