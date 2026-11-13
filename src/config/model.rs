//! The shape of `config.json`. Anything missing takes its default, so a partial file is valid. The Configuration
//! page of the wiki describes every key.

use std::collections::BTreeMap;
use std::time::Duration;

use notchbase_sdk::protocol::Values;
use serde::{Deserialize, Serialize};

use super::annotate::AnnotateConfig;
use super::updates::UpdatesConfig;

/// The file's format. Older files are upgraded once, by `migrate::upgrade`.
pub const VERSION: u32 = 7;
/// The tabs notchbase has itself. Any other id in `layout.tabs` is a tool's.
pub const CORE_TABS: [&str; 6] = ["media", "tools", "shelf", "sound", "stats", "agents"];
/// The tabs a new file starts with, in order.
pub const DEFAULT_TABS: [&str; 5] = ["media", "tools", "shelf", "agents", "stats"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub displays: Displays,
    pub notch: NotchConfig,
    pub notices: Notices,
    pub music: MusicConfig,
    pub volume: VolumeConfig,
    pub brightness: BrightnessConfig,
    pub bluetooth: BluetoothConfig,
    pub media: MediaConfig,
    pub stats: StatsConfig,
    pub assistant: AssistantConfig,
    pub updates: UpdatesConfig,
    pub awake: AwakeConfig,
    pub snap: SnapConfig,
    pub videos: VideosConfig,
    pub annotate: AnnotateConfig,
    pub find: FindConfig,
    pub permissions: Permissions,
    pub dock: DockConfig,
    pub layout: Layout,
    pub tools: BTreeMap<String, ToolConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: VERSION,
            displays: Displays::default(),
            notch: NotchConfig::default(),
            notices: Notices::default(),
            music: MusicConfig::default(),
            volume: VolumeConfig::default(),
            brightness: BrightnessConfig::default(),
            bluetooth: BluetoothConfig::default(),
            media: MediaConfig::default(),
            stats: StatsConfig::default(),
            assistant: AssistantConfig::default(),
            updates: UpdatesConfig::default(),
            awake: AwakeConfig::default(),
            snap: SnapConfig::default(),
            videos: VideosConfig::default(),
            annotate: AnnotateConfig::default(),
            find: FindConfig::default(),
            permissions: Permissions::default(),
            dock: DockConfig::default(),
            layout: Layout::default(),
            tools: BTreeMap::new(),
        }
    }
}

/// notchbase dock, the Dock replacement on a Mac. It is off until someone turns it on, and until then notchbase never
/// starts it and the macOS Dock is left alone. `introduced` remembers the one-time notice that offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    pub enabled: bool,
    pub introduced: bool,
}

/// What has been asked for already and where the walkthrough stands, so nothing asks twice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    pub full_disk_asked: bool,
    pub find_disk_asked: bool,
    pub find_access_hidden: bool,
    pub walkthrough: Walkthrough,
    pub asked: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Walkthrough {
    #[default]
    Pending,
    Done,
}

impl Permissions {
    pub fn was_asked(&self, key: &str) -> bool {
        self.asked.iter().any(|asked| asked == key)
    }

    pub fn note_asked(&mut self, key: &str) {
        if !self.was_asked(key) {
            self.asked.push(key.to_owned());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FindConfig {
    pub enabled: bool,
    pub excluded: Vec<String>,
    pub volumes: bool,
    pub max_entries: u64,
    pub learn: bool,
    pub recents: bool,
    pub system_key: bool,
    pub hidden: bool,
    pub pins: Vec<FindPin>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FindPin {
    pub path: String,
    pub name: String,
    pub app: bool,
    pub folder: bool,
}

impl Default for FindConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            excluded: Vec::new(),
            volumes: true,
            max_entries: 0,
            learn: true,
            recents: true,
            system_key: true,
            hidden: false,
            pins: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Displays {
    pub all: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotchConfig {
    pub open_on_hover: bool,
    pub wake_at_edge: bool,
    pub flee_from_keys: bool,
    pub haptics: bool,
    pub dodge_menus: bool,
    pub lock_screen: bool,
    pub hover_delay_ms: u64,
    pub close_delay_ms: u64,
    pub start_page: String,
    pub collapse_before_capture_ms: u64,
}

impl Default for NotchConfig {
    fn default() -> Self {
        Self {
            open_on_hover: true,
            wake_at_edge: true,
            flee_from_keys: true,
            haptics: true,
            dodge_menus: true,
            lock_screen: true,
            hover_delay_ms: 120,
            close_delay_ms: 220,
            start_page: "tools".into(),
            collapse_before_capture_ms: 260,
        }
    }
}

impl NotchConfig {
    pub fn hover_delay(&self) -> Duration {
        Duration::from_millis(self.hover_delay_ms)
    }

    pub fn close_delay(&self) -> Duration {
        Duration::from_millis(self.close_delay_ms)
    }

    pub fn collapse_before_capture(&self) -> Duration {
        Duration::from_millis(self.collapse_before_capture_ms)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notices {
    pub message_ms: u64,
    pub volume_ms: u64,
    pub brightness_ms: u64,
    pub device_connected_ms: u64,
    pub device_disconnected_ms: u64,
    pub shelf_ms: u64,
}

impl Default for Notices {
    fn default() -> Self {
        Self {
            message_ms: 3500,
            volume_ms: 1600,
            brightness_ms: 1600,
            device_connected_ms: 3200,
            device_disconnected_ms: 2200,
            shelf_ms: 4000,
        }
    }
}

impl Notices {
    pub fn message(&self) -> Duration {
        Duration::from_millis(self.message_ms)
    }

    pub fn volume(&self) -> Duration {
        Duration::from_millis(self.volume_ms)
    }

    pub fn brightness(&self) -> Duration {
        Duration::from_millis(self.brightness_ms)
    }

    pub fn device(&self, connected: bool) -> Duration {
        Duration::from_millis(if connected { self.device_connected_ms } else { self.device_disconnected_ms })
    }

    pub fn shelf(&self) -> Duration {
        Duration::from_millis(self.shelf_ms)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicConfig {
    pub in_notch: bool,
    pub dance: bool,
    pub open_to_player: bool,
    pub reveal_ms: u64,
    pub hover_reveal_ms: u64,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self { in_notch: true, dance: true, open_to_player: true, reveal_ms: 4000, hover_reveal_ms: 300 }
    }
}

impl MusicConfig {
    pub fn reveal(&self) -> Duration {
        Duration::from_millis(self.reveal_ms)
    }

    pub fn hover_reveal(&self) -> Duration {
        Duration::from_millis(self.hover_reveal_ms)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeConfig {
    pub indicator: bool,
    pub quiet_after_mixer_ms: u64,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self { indicator: true, quiet_after_mixer_ms: 1200 }
    }
}

impl VolumeConfig {
    pub fn quiet_after_mixer(&self) -> Duration {
        Duration::from_millis(self.quiet_after_mixer_ms)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightnessConfig {
    pub indicator: bool,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self { indicator: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BluetoothConfig {
    pub indicator: bool,
    pub battery: bool,
    pub poll_ms: u64,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self { indicator: true, battery: true, poll_ms: 1500 }
    }
}

impl BluetoothConfig {
    pub fn poll(&self) -> Duration {
        Duration::from_millis(self.poll_ms.max(250))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub poll_ms: u64,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self { poll_ms: 800 }
    }
}

impl MediaConfig {
    pub fn poll(&self) -> Duration {
        Duration::from_millis(self.poll_ms.max(100))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatsConfig {
    pub refresh_ms: u64,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self { refresh_ms: 1000 }
    }
}

impl StatsConfig {
    pub fn refresh(&self) -> Duration {
        Duration::from_millis(self.refresh_ms.max(250))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantConfig {
    pub enabled: bool,
    pub provider: String,
    pub questions_in_notch: bool,
    pub progress_in_notch: bool,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self { enabled: true, provider: "claude".into(), questions_in_notch: true, progress_in_notch: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapConfig {
    pub enabled: bool,
    pub gap: f32,
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self { enabled: true, gap: 8.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideosConfig {
    pub enabled: bool,
    pub quality: u32,
    pub audio: String,
    pub sign_in: bool,
}

impl VideosConfig {
    pub const SMALLEST: u32 = 144;
    pub const TALLEST: u32 = 4320;

    /// `quality` as the tallest picture to download, kept between 144 and 4320 pixels.
    pub fn tallest(&self) -> u32 {
        self.quality.clamp(Self::SMALLEST, Self::TALLEST)
    }

    /// `mp3` when that's asked for, and `m4a` for anything else.
    pub fn audio_format(&self) -> &'static str {
        if self.audio.trim().eq_ignore_ascii_case("mp3") { "mp3" } else { "m4a" }
    }
}

impl Default for VideosConfig {
    fn default() -> Self {
        Self { enabled: true, quality: Self::TALLEST, audio: "m4a".to_owned(), sign_in: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AwakeConfig {
    pub force: bool,
    pub hold_lid_closed: bool,
}

impl Default for AwakeConfig {
    fn default() -> Self {
        Self { force: false, hold_lid_closed: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub tabs: Vec<String>,
    pub tools: Vec<String>,
}

impl Default for Layout {
    fn default() -> Self {
        Self { tabs: DEFAULT_TABS.map(str::to_owned).to_vec(), tools: Vec::new() }
    }
}

/// A tool's entry under `tools`: whether it runs, its shortcuts (`None` deliberately unbound) and its settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolConfig {
    pub enabled: bool,
    pub shortcuts: BTreeMap<String, Option<String>>,
    pub settings: Values,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self { enabled: true, shortcuts: BTreeMap::new(), settings: Values::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_library_setting_is_ignored() {
        let old: Config = serde_json::from_str(r#"{ "library": { "automatic": false } }"#).unwrap();
        assert_eq!(old, Config::default());
        assert!(!serde_json::to_string_pretty(&old).unwrap().contains("library"));
    }
}
