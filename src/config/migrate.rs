//! Brings older settings forward: a `settings.txt` from before `config.json`, and every format change since then.

use notchbase_sdk::protocol::Value;

use super::hotkey;
use super::model::{CORE_TABS, Config, ToolConfig, VERSION, Walkthrough};

/// Where versions before `config.json` kept a few choices, in the data folder.
pub const LEGACY_FILE: &str = "settings.txt";
const UNSET: &str = "none";
/// The shortcuts `settings.txt` held, with the tool and the shortcut each one became.
const SHORTCUTS: [(&str, &str, &str); 3] = [
    ("pick_color", "color-picker", "pick"),
    ("screenshot", "screenshot", "capture"),
    ("record_screen", "screen-recorder", "record"),
];
const RECORDER: &str = "screen-recorder";
const SLOW_HOVER_MS: u64 = 300;
const QUICK_HOVER_MS: u64 = 120;
const QUICK_HOVER_SINCE: u32 = 2;
const FOLDED_SOUND_SINCE: u32 = 3;
const AGENTS_TAB_SINCE: u32 = 4;
const SYSTEM_KEY_SINCE: u32 = 5;
const WALKTHROUGH_SINCE: u32 = 6;
const SLOW_SONG_MS: u64 = 1500;
const QUICK_SONG_MS: u64 = 300;
const QUICK_SONG_SINCE: u32 = 7;
const CALENDAR: &str = "calendar";
const AGENTS: &str = "agents";
const STATS: &str = "stats";
const SOUND_TAB_DEFAULT: [&str; 5] = ["media", "tools", "shelf", "sound", "stats"];
const FOLDED_SOUND_DEFAULT: [&str; 4] = ["media", "tools", "shelf", "stats"];

pub fn from_legacy(text: &str) -> Config {
    let mut config = Config::default();
    // Someone with a `settings.txt` has used notchbase before, so the walkthrough is skipped.
    config.permissions.walkthrough = Walkthrough::Done;
    for (name, value) in entries(text) {
        if let Some((_, tool, shortcut)) = SHORTCUTS.iter().find(|(legacy, _, _)| *legacy == name) {
            let bound = if value == UNSET { None } else { hotkey::parse(value).map(|hotkey| hotkey::format(&hotkey)) };
            if value == UNSET || bound.is_some() {
                tool_mut(&mut config, tool).shortcuts.insert((*shortcut).to_owned(), bound);
            }
            continue;
        }
        let flag = value.parse::<bool>().ok();
        match (name, flag) {
            ("assistant", _) if matches!(value, "claude" | "chatgpt") => config.assistant.provider = value.to_owned(),
            ("assistant_enabled", Some(on)) => config.assistant.enabled = on,
            ("questions_in_notch", Some(on)) => config.assistant.questions_in_notch = on,
            ("volume_indicator", Some(on)) => config.volume.indicator = on,
            ("all_displays", Some(on)) => config.displays.all = on,
            ("record_microphone", Some(on)) => recorder_setting(&mut config, "microphone", Value::Bool(on)),
            ("record_desktop_audio", Some(on)) => recorder_setting(&mut config, "desktop_audio", Value::Bool(on)),
            ("record_microphone_device", _) if !value.is_empty() => {
                recorder_setting(&mut config, "microphone_id", Value::String(value.to_owned()));
            }
            _ => {}
        }
    }
    config
}

/// Brings an older file up to `VERSION`, and says whether it did. Each step runs once, for files older than it.
pub fn upgrade(config: &mut Config) -> bool {
    if config.version >= VERSION {
        return false;
    }
    if config.version < QUICK_HOVER_SINCE && config.notch.hover_delay_ms == SLOW_HOVER_MS {
        config.notch.hover_delay_ms = QUICK_HOVER_MS;
    }
    if config.version < FOLDED_SOUND_SINCE {
        follow_default(&mut config.layout.tabs, &SOUND_TAB_DEFAULT, &FOLDED_SOUND_DEFAULT);
    }
    if config.version < AGENTS_TAB_SINCE {
        add_agents(&mut config.layout.tabs);
    }
    if config.version < SYSTEM_KEY_SINCE {
        // Off was only the old default, so everyone gets the key once.
        config.find.system_key = true;
    }
    if config.version < WALKTHROUGH_SINCE {
        config.permissions.walkthrough = Walkthrough::Done;
    }
    if config.version < QUICK_SONG_SINCE && config.music.hover_reveal_ms == SLOW_SONG_MS {
        config.music.hover_reveal_ms = QUICK_SONG_MS;
    }
    config.version = VERSION;
    true
}

/// Swaps the old default tabs for the new ones, keeping a calendar added at the end. Arranged tabs are left alone.
fn follow_default(tabs: &mut Vec<String>, old: &[&str], new: &[&str]) {
    let calendar = tabs.len() == old.len() + 1 && tabs.last().is_some_and(|tab| tab == CALENDAR);
    if tabs[..tabs.len() - usize::from(calendar)] != *old {
        return;
    }
    *tabs = new.iter().map(|tab| (*tab).to_owned()).collect();
    if calendar {
        tabs.push(CALENDAR.to_owned());
    }
}

/// Adds the agents tab before stats, or after the last built-in tab, unless it's there already.
fn add_agents(tabs: &mut Vec<String>) {
    if tabs.iter().any(|tab| tab == AGENTS) {
        return;
    }
    let after_core = || tabs.iter().rposition(|tab| CORE_TABS.contains(&tab.as_str())).map(|index| index + 1);
    let at = tabs.iter().position(|tab| tab == STATS).or_else(after_core).unwrap_or(tabs.len());
    tabs.insert(at, AGENTS.to_owned());
}

fn entries(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| line.split_once('=')).map(|(name, value)| (name.trim(), value.trim()))
}

fn tool_mut<'a>(config: &'a mut Config, id: &str) -> &'a mut ToolConfig {
    config.tools.entry(id.to_owned()).or_default()
}

fn recorder_setting(config: &mut Config, key: &str, value: Value) {
    tool_mut(config, RECORDER).settings.insert(key.to_owned(), value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_TABS;

    const OLD: &str = "pick_color=shift+control+KeyC\nscreenshot=none\nrecord_screen=alt+control+KeyR\n\
assistant=chatgpt\nassistant_enabled=true\nquestions_in_notch=false\nrecord_microphone=true\n\
record_desktop_audio=false\nvolume_indicator=false\nall_displays=true\nrecord_microphone_device={0.0.1}.{ab}\n";

    #[test]
    fn old_settings_file_carries_over() {
        let config = from_legacy(OLD);
        let shortcut = |tool: &str, id: &str| config.tools[tool].shortcuts[id].clone();
        assert_eq!(shortcut("color-picker", "pick").as_deref(), Some("Ctrl+Shift+C"));
        assert_eq!(shortcut("screenshot", "capture"), None);
        assert_eq!(
            shortcut("screen-recorder", "record").as_deref(),
            Some(hotkey::format(&"alt+control+KeyR".parse().unwrap()).as_str())
        );
        assert_eq!((config.assistant.provider.as_str(), config.assistant.questions_in_notch), ("chatgpt", false));
        assert!(!config.volume.indicator && config.displays.all);
        let recorder = &config.tools["screen-recorder"].settings;
        assert_eq!(recorder["microphone"], Value::Bool(true));
        assert_eq!(recorder["desktop_audio"], Value::Bool(false));
        assert_eq!(recorder["microphone_id"], Value::String("{0.0.1}.{ab}".into()));
    }

    #[test]
    fn nonsense_keeps_the_defaults() {
        let config = from_legacy("assistant=gemini\nvolume_indicator=loud\nscreenshot=Ctrl+Nope+Z+Q\nno separator\n");
        let mut defaults = Config::default();
        defaults.permissions.walkthrough = Walkthrough::Done;
        assert_eq!(config, defaults);
    }

    #[test]
    fn earlier_users_skip_the_walkthrough() {
        let mut before = Config { version: WALKTHROUGH_SINCE - 1, ..Config::default() };
        assert_eq!(before.permissions.walkthrough, Walkthrough::Pending, "the field was missing, so it read as new");
        assert!(upgrade(&mut before));
        assert_eq!(before.permissions.walkthrough, Walkthrough::Done);
        assert_eq!(from_legacy("").permissions.walkthrough, Walkthrough::Done, "nor is anyone with older settings");
        assert_eq!(Config::default().permissions.walkthrough, Walkthrough::Pending, "a new install starts with it due");
    }

    #[test]
    fn due_walkthrough_survives_upgrade() {
        let mut current = Config::default();
        assert!(!upgrade(&mut current));
        assert_eq!(current.permissions.walkthrough, Walkthrough::Pending);
    }

    #[test]
    fn older_config_gets_quicker_hover() {
        let mut config = Config { version: 1, ..Config::default() };
        config.notch.hover_delay_ms = SLOW_HOVER_MS;
        assert!(upgrade(&mut config));
        assert_eq!((config.version, config.notch.hover_delay_ms), (VERSION, QUICK_HOVER_MS));
        assert!(!upgrade(&mut config), "it only happens once");
    }

    #[test]
    fn system_key_taken_once_for_everyone() {
        let mut before = Config { version: SYSTEM_KEY_SINCE - 1, ..Config::default() };
        before.find.system_key = false;
        assert!(upgrade(&mut before));
        assert!(before.find.system_key, "off was only the old default");
        before.find.system_key = false;
        assert!(!upgrade(&mut before), "an upgraded config is left alone");
        assert!(!before.find.system_key, "so turning it off afterwards holds");
        assert!(Config::default().find.system_key, "and a new install starts with it on");
    }

    #[test]
    fn chosen_hover_delay_is_kept() {
        let mut config = Config { version: 1, ..Config::default() };
        config.notch.hover_delay_ms = 500;
        assert!(upgrade(&mut config));
        assert_eq!(config.notch.hover_delay_ms, 500);
        let mut current = Config::default();
        current.notch.hover_delay_ms = SLOW_HOVER_MS;
        assert!(!upgrade(&mut current), "a current config that asks for 300 keeps it");
        assert_eq!(current.notch.hover_delay_ms, SLOW_HOVER_MS);
    }

    #[test]
    fn older_config_lets_song_go_sooner() {
        let mut config = Config { version: QUICK_SONG_SINCE - 1, ..Config::default() };
        config.music.hover_reveal_ms = SLOW_SONG_MS;
        assert!(upgrade(&mut config));
        assert_eq!((config.version, config.music.hover_reveal_ms), (VERSION, QUICK_SONG_MS));
        assert!(!upgrade(&mut config), "it only happens once");
        assert_eq!(Config::default().music.hover_reveal_ms, QUICK_SONG_MS, "and a new install starts there");
    }

    #[test]
    fn chosen_song_linger_is_kept() {
        let mut config = Config { version: QUICK_SONG_SINCE - 1, ..Config::default() };
        config.music.hover_reveal_ms = 800;
        assert!(upgrade(&mut config));
        assert_eq!(config.music.hover_reveal_ms, 800);
        let mut current = Config::default();
        current.music.hover_reveal_ms = SLOW_SONG_MS;
        assert!(!upgrade(&mut current), "a current config that asks for 1500 keeps it");
        assert_eq!(current.music.hover_reveal_ms, SLOW_SONG_MS);
    }

    fn tabs(list: &[&str]) -> Vec<String> {
        list.iter().map(|tab| (*tab).to_owned()).collect()
    }

    fn upgraded_tabs(version: u32, list: &[&str]) -> Vec<String> {
        let mut config = Config { version, ..Config::default() };
        config.layout.tabs = tabs(list);
        upgrade(&mut config);
        config.layout.tabs
    }

    #[test]
    fn old_default_header_loses_sound() {
        assert_eq!(upgraded_tabs(2, &SOUND_TAB_DEFAULT), tabs(&DEFAULT_TABS));
        assert_eq!(upgraded_tabs(1, &SOUND_TAB_DEFAULT), tabs(&DEFAULT_TABS));
    }

    #[test]
    fn calendar_after_old_default_stays() {
        let old = [SOUND_TAB_DEFAULT.as_slice(), &[CALENDAR]].concat();
        let new = [DEFAULT_TABS.as_slice(), &[CALENDAR]].concat();
        assert_eq!(upgraded_tabs(2, &old), tabs(&new));
    }

    #[test]
    fn arranged_header_keeps_sound_tab() {
        let arranged = ["sound", "media", "tools", "shelf", "stats"];
        assert_eq!(upgraded_tabs(2, &arranged), tabs(&["sound", "media", "tools", "shelf", "agents", "stats"]));
        let extra = ["media", "tools", "shelf", "sound", "stats", "timer"];
        assert_eq!(upgraded_tabs(2, &extra), tabs(&["media", "tools", "shelf", "sound", "agents", "stats", "timer"]));
        let calendar_first = ["calendar", "media", "tools", "shelf", "sound", "stats"];
        assert_eq!(
            upgraded_tabs(2, &calendar_first),
            tabs(&["calendar", "media", "tools", "shelf", "sound", "agents", "stats"])
        );
    }

    #[test]
    fn version_three_header_gains_agents() {
        assert_eq!(upgraded_tabs(3, &FOLDED_SOUND_DEFAULT), tabs(&DEFAULT_TABS));
        let old = [FOLDED_SOUND_DEFAULT.as_slice(), &[CALENDAR]].concat();
        let new = [DEFAULT_TABS.as_slice(), &[CALENDAR]].concat();
        assert_eq!(upgraded_tabs(3, &old), tabs(&new));
        assert_eq!(DEFAULT_TABS, ["media", "tools", "shelf", "agents", "stats"]);
    }

    #[test]
    fn reordered_gains_agents_before_stats() {
        let arranged = ["stats", "media", "tools", "shelf"];
        assert_eq!(upgraded_tabs(3, &arranged), tabs(&["agents", "stats", "media", "tools", "shelf"]));
        let calendar_first = ["calendar", "media", "tools", "shelf", "stats"];
        assert_eq!(
            upgraded_tabs(3, &calendar_first),
            tabs(&["calendar", "media", "tools", "shelf", "agents", "stats"])
        );
        let extra = ["media", "tools", "shelf", "stats", "timer"];
        assert_eq!(upgraded_tabs(3, &extra), tabs(&["media", "tools", "shelf", "agents", "stats", "timer"]));
    }

    #[test]
    fn no_stats_puts_agents_after_core() {
        assert_eq!(upgraded_tabs(3, &["media", "shelf"]), tabs(&["media", "shelf", "agents"]));
        assert_eq!(upgraded_tabs(3, &["shelf", "media", "sound"]), tabs(&["shelf", "media", "sound", "agents"]));
        assert_eq!(upgraded_tabs(3, &[]), tabs(&["agents"]));
        assert_eq!(upgraded_tabs(3, &["calendar"]), tabs(&["calendar", "agents"]));
    }

    #[test]
    fn agents_goes_before_trailing_calendar() {
        let trailing = ["media", "tools", "shelf", "calendar"];
        assert_eq!(upgraded_tabs(3, &trailing), tabs(&["media", "tools", "shelf", "agents", "calendar"]));
        let both = ["tools", "media", "calendar", "timer"];
        assert_eq!(upgraded_tabs(3, &both), tabs(&["tools", "media", "agents", "calendar", "timer"]));
    }

    #[test]
    fn header_with_agents_is_left_alone() {
        let placed = ["agents", "media", "stats"];
        assert_eq!(upgraded_tabs(3, &placed), tabs(&placed));
    }

    #[test]
    fn current_header_stays_without_agents() {
        assert_eq!(upgraded_tabs(VERSION, &FOLDED_SOUND_DEFAULT), tabs(&FOLDED_SOUND_DEFAULT));
    }

    #[test]
    fn current_config_keeps_sound_tab() {
        assert_eq!(upgraded_tabs(VERSION, &SOUND_TAB_DEFAULT), tabs(&SOUND_TAB_DEFAULT));
        let mut config = Config::default();
        config.layout.tabs = tabs(&SOUND_TAB_DEFAULT);
        config.notch.hover_delay_ms = SLOW_HOVER_MS;
        assert!(!upgrade(&mut config));
    }

    #[test]
    fn version_two_keeps_its_hover_delay() {
        let mut config = Config { version: 2, ..Config::default() };
        config.notch.hover_delay_ms = SLOW_HOVER_MS;
        assert!(upgrade(&mut config));
        assert_eq!((config.version, config.notch.hover_delay_ms), (VERSION, SLOW_HOVER_MS));
    }
}
