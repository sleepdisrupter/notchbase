use std::time::Instant;

use notchbase_sdk::Waker;
use notchbase_sdk::protocol::{
    Download, DownloadChange, Event, Power, RecordingState, Request, Sensors, UiValue, Value,
};

use super::*;
use crate::config::{Config, ToolConfig, hotkey};

const BUNDLED: [&str; 13] = [
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

fn quiet() -> Config {
    let mut config = Config::default();
    let mut alerts = ToolConfig::default();
    alerts.settings.insert("enabled".into(), Value::Bool(false));
    config.tools.insert("alerts".into(), alerts);
    config
}

fn loaded(config: &Config) -> Tools {
    let (mut tools, problems) = Tools::load(config, &Waker::noop());
    assert!(problems.is_empty(), "{problems:?}");
    tools.start(config, Instant::now());
    tools
}

fn ids(tools: &Tools) -> Vec<&str> {
    tools.hosts().iter().map(ToolHost::id).collect()
}

#[test]
fn bundled_tools_start_in_order() {
    let mut config = quiet();
    assert_eq!(ids(&loaded(&config)), BUNDLED);
    config.layout.tools = vec!["screen-recorder".into(), "color-picker".into()];
    let unlisted = BUNDLED.into_iter().filter(|id| !["screen-recorder", "color-picker"].contains(id));
    assert_eq!(
        ids(&loaded(&config)),
        ["screen-recorder", "color-picker"].into_iter().chain(unlisted).collect::<Vec<_>>()
    );
}

#[test]
fn reorder_moves_without_restart() {
    let mut config = quiet();
    let mut tools = loaded(&config);
    let slot = |tools: &Tools, id: &str| tools.hosts().iter().find(|host| host.id() == id).map(|host| host.slot);
    let place = |tools: &Tools, id: &str| ids(tools).iter().position(|listed| *listed == id);
    let (picker, recorder) = (slot(&tools, "color-picker"), slot(&tools, "screen-recorder"));
    assert!(place(&tools, "color-picker") < place(&tools, "screen-recorder"));
    config.layout.tools = vec!["screen-recorder".into(), "color-picker".into()];
    tools.reorder(&config);
    assert_eq!(ids(&tools)[..2], ["screen-recorder", "color-picker"]);
    assert_eq!((slot(&tools, "color-picker"), slot(&tools, "screen-recorder")), (picker, recorder));
    let listed: Vec<&str> = tools.catalog().iter().map(|info| info.id.as_str()).collect();
    assert_eq!(listed[..2], ["screen-recorder", "color-picker"]);
    assert!(tools.refresh(&config), "the views follow the new order");
    assert_eq!(tools.views()[0].id, "screen-recorder");
}

#[test]
fn disabled_tool_stays_in_catalog() {
    let mut config = quiet();
    config.tools.insert("screenshot".into(), ToolConfig { enabled: false, ..ToolConfig::default() });
    let tools = loaded(&config);
    assert_eq!(ids(&tools), BUNDLED.into_iter().filter(|id| *id != "screenshot").collect::<Vec<_>>());
    let listed: Vec<(&str, bool)> = tools.catalog().iter().map(|info| (info.id.as_str(), info.enabled)).collect();
    assert_eq!(listed, BUNDLED.map(|id| (id, id != "screenshot")));
    assert!(tools.catalog().iter().all(|info| info.location.is_none() && info.problem.is_none()));
}

#[test]
fn views_carry_tiles_and_settings() {
    let mut config = quiet();
    let mut picker = ToolConfig::default();
    picker.settings.insert("format".into(), Value::String("hsl".into()));
    config.tools.insert("color-picker".into(), picker);
    let mut tools = loaded(&config);
    assert!(tools.refresh(&config));
    assert!(!tools.refresh(&config));
    let picker = &tools.views()[0];
    assert_eq!(picker.tile.as_ref().map(|tile| tile.title.as_str()), Some("Pick Color"));
    assert_eq!(picker.settings[0].value, Value::String("hsl".into()));
}

#[test]
fn config_shortcuts_override_manifest() {
    let mut config = quiet();
    let tools = loaded(&config);
    let defaults = bindings(&tools, &config);
    let pick = defaults.iter().find(|binding| binding.id == "pick").unwrap();
    assert_eq!(pick.hotkey, hotkey::parse("Ctrl+Shift+C"));
    let mut recorder = ToolConfig::default();
    recorder.shortcuts.insert("record".into(), None);
    config.tools.insert("screen-recorder".into(), recorder);
    let chosen = bindings(&tools, &config);
    assert_eq!(chosen.iter().find(|binding| binding.id == "record").unwrap().hotkey, None);
}

#[test]
fn late_subscriber_hears_recording() {
    let config = quiet();
    let mut tools = loaded(&config);
    tools.feed_recording(RecordingState::Recording { since: 7 });
    tools.pump(Instant::now());
    tools.refresh(&config);
    let recorder = tools.views().iter().find(|view| view.id == "screen-recorder").unwrap();
    assert!(recorder.live.is_some());
}

#[test]
fn system_tools_ask_for_their_feeds() {
    let config = quiet();
    let tools = loaded(&config);
    assert!(tools.wants_power() && tools.wants_sensors() && tools.wants_downloads() && tools.wants_focus());
    let mut config = quiet();
    for id in ["battery", "camera-mic", "downloads"] {
        config.tools.insert(id.into(), ToolConfig { enabled: false, ..ToolConfig::default() });
    }
    let tools = loaded(&config);
    assert!(!tools.wants_power() && !tools.wants_sensors() && !tools.wants_downloads());
}

#[test]
fn power_changes_become_notices() {
    let config = quiet();
    let mut tools = loaded(&config);
    let slot = |tools: &Tools, id: &str| tools.hosts().iter().find(|host| host.id() == id).unwrap().slot;
    tools.feed_power(Power { level: Some(64), plugged: false, charging: false });
    assert!(tools.pump(Instant::now()).is_empty(), "the first reading is not news");
    tools.feed_power(Power { level: Some(64), plugged: true, charging: true });
    tools.feed_power(Power { level: Some(64), plugged: true, charging: true });
    let requests = tools.pump(Instant::now());
    let owners: Vec<ToolSlot> = requests.iter().map(|(owner, _)| *owner).collect();
    assert_eq!(owners, [slot(&tools, "battery")], "a repeated reading is fed once");
    assert!(requests.iter().all(|(_, request)| matches!(request, Request::Notice { .. })));
}

#[test]
fn camera_and_downloads_become_live() {
    let config = quiet();
    let mut tools = loaded(&config);
    tools.feed_sensors(Sensors { camera: true, ..Sensors::default() });
    let download = Download { id: "a.dmg.part".into(), name: "a.dmg".into(), received: 4, total: Some(8) };
    tools.feed_download(DownloadChange::Progress { download: download.clone() });
    tools.pump(Instant::now());
    tools.refresh(&config);
    let live = |tools: &Tools, id: &str| tools.views().iter().find(|view| view.id == id).unwrap().live.is_some();
    assert!(live(&tools, "camera-mic") && live(&tools, "downloads"));
    tools.feed_sensors(Sensors::default());
    tools.feed_download(DownloadChange::Finished { download, path: "a.dmg".into() });
    let requests = tools.pump(Instant::now());
    assert!(requests.iter().any(|(_, request)| matches!(request, Request::Shelve { paths } if paths == &["a.dmg"])));
    tools.refresh(&config);
    assert!(!live(&tools, "camera-mic") && !live(&tools, "downloads"));
}

#[test]
fn keep_awake_tile_asks_the_host() {
    let config = quiet();
    let mut tools = loaded(&config);
    tools.refresh(&config);
    let view = tools.views().iter().find(|view| view.id == "keep-awake").unwrap();
    let (slot, press) = (view.slot, view.tile.as_ref().and_then(|tile| tile.on_press).expect("a tile to press"));
    let pressed = Event::Ui { surface: notchbase_sdk::protocol::Surface::Tile, handler: press, value: UiValue::Press };
    tools.send(slot, pressed);
    let saved = Request::SaveSetting { key: "session".into(), value: "forever".into() };
    assert_eq!(tools.pump(Instant::now()), [(slot, Request::KeepAwake { awake: true }), (slot, saved)]);
}

#[test]
fn lid_switch_saves_to_awake_settings() {
    let mut config = quiet();
    let mut tools = loaded(&config);
    tools.refresh(&config);
    let lid = |tools: &Tools| {
        let view = tools.views().iter().find(|view| view.id == keep_awake::ID).unwrap();
        let setting = view.settings.iter().find(|setting| setting.spec.key == keep_awake::LID).unwrap();
        (setting.spec.visible, setting.value.clone())
    };
    assert_eq!(lid(&tools), (cfg!(target_os = "macos"), Value::Bool(true)));
    save_setting(&mut config, keep_awake::ID.into(), keep_awake::LID.into(), Value::Bool(false));
    assert!(!config.awake.hold_lid_closed);
    assert!(config.tools.get(keep_awake::ID).is_none_or(|tool| !tool.settings.contains_key(keep_awake::LID)));
    tools.configure(&config);
    tools.refresh(&config);
    assert_eq!(lid(&tools).1, Value::Bool(false));
    save_setting(&mut config, keep_awake::ID.into(), "in_notch".into(), Value::Bool(false));
    assert_eq!(config.tools[keep_awake::ID].settings["in_notch"], Value::Bool(false));
}

#[test]
fn shortcut_presses_reach_their_tool() {
    let config = quiet();
    let mut tools = loaded(&config);
    let slot = tools.hosts().iter().find(|host| host.id() == "screenshot").unwrap().slot;
    tools.send(slot, Event::Shortcut { id: "capture".into() });
    let requests = tools.pump(Instant::now());
    assert!(matches!(requests[..], [(owner, Request::CaptureScreen { .. })] if owner == slot));
}

#[test]
fn page_only_tool_gets_a_tile() {
    let manifest = Manifest::new("launcher", "Launcher").page("Apps").page_icon(notchbase_sdk::protocol::Icon::Grid);
    let tile = launcher_tile(&manifest).expect("a tile on the Tools tab");
    assert_eq!(tile.title, "Apps");
    assert!(tile.on_press.is_none(), "clicking it opens the page");
    assert!(launcher_tile(&Manifest::new("plain", "Plain")).is_none());
    assert!(
        launcher_tile(&Manifest::new("tabbed", "Tabbed").page("Tabbed").header_tab()).is_none(),
        "its tab opens it"
    );
}
