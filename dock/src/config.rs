//! The dock's own settings and pinned apps, in `dock.json` beside the saved Dock state. The first run copies them
//! from the system Dock; after that the system Dock's list is never written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "dock.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Apps kept in the dock, left to right.
    pub pinned: Vec<PathBuf>,
    /// Folders (stacks) and files on the right, before the Trash.
    pub others: Vec<PathBuf>,
    /// Downloads as a stack on the right, first of the folders, whatever `others` says. Removing its tile turns
    /// this off; Settings › Dock turns it back on.
    pub show_downloads: bool,
    /// The screen edge the dock stands on: `"bottom"`, `"left"` or `"right"`.
    pub side: crate::side::Side,
    /// Icon size at rest, in points.
    pub tile_size: f32,
    /// Icon size right under the pointer, in points.
    pub large_size: f32,
    pub magnification: bool,
    /// What opening an app from the dock looks like.
    pub launch_effect: LaunchEffect,
    /// A name tooltip over the hovered icon.
    pub show_names: bool,
    /// Shorten zoomed, filled and tiled windows so they end above the dock (needs Accessibility).
    pub avoid_windows: bool,
    /// How long minimizing (and restoring) a window takes, in seconds.
    pub minimize_duration: f32,
    /// How windows go down into the dock: `"liquid"`, drawn by the dock, or `"system"`, macOS's own effect.
    pub minimize_effect: MinimizeEffect,
    /// Slide down out of sight over a full-screen app, as the Dock does; the bottom edge calls it back.
    pub hide_in_fullscreen: bool,
    /// Cmd+Tab shows the dock's own switcher (on the pointer's screen, only apps with a window open there) instead
    /// of macOS's.
    pub replace_switcher: bool,
    /// Resting on a running app's icon shows its open windows side by side (pictures need Screen Recording).
    pub window_previews: bool,
    /// How black the glass is at the top of the dock and at the bottom, 0 (clear) to 1 (solid).
    pub glass_tint_top: f32,
    pub glass_tint_bottom: f32,
    /// How menus and window previews sit above the dock: `"float"`, a glass panel of their own a little above it, or
    /// `"neck"`, joined to the dock by a liquid neck.
    pub popup_attach: PopupAttach,
    /// What happens when macOS shows its own Dock (Mission Control): `"cover"` keeps this dock where it is, over a
    /// system Dock made tiny for the purpose, settling a touch and darkening with the desktop, or `"fade"`, which
    /// leaves the system Dock's size alone and fades this one out.
    pub mission_control: MissionControl,
    /// How a folder (Downloads) opens: `"automatic"` fans out a few items and shows a grid for more, or always
    /// `"fan"` or `"grid"`.
    pub stack_view: StackView,
    /// The order a folder's items are shown in, the top of its pile first: `"added"`, `"modified"` or `"name"`.
    pub stack_sort: StackSort,
    /// Which screens show the dock: `"primary"`, the one with the menu bar, or `"all"`, one dock on each.
    pub displays: Displays,
    /// While Spotify or Music plays, its icon swells gently on the beat.
    pub music_pulse: bool,
    /// On a Mac, the beat comes from the player's own sound. Hearing it takes ScreenCaptureKit, so macOS shows its
    /// screen recording indicator the whole time music plays: off (as it starts), the icon breathes slowly instead.
    pub music_beat: bool,
    /// Settings for one app that differ from the ones above, by the app's path. Set from its menu's Options.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub apps: BTreeMap<PathBuf, AppSettings>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PopupAttach {
    #[default]
    Float,
    Neck,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinimizeEffect {
    #[default]
    Liquid,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MissionControl {
    #[default]
    Cover,
    Fade,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Displays {
    #[default]
    Primary,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StackView {
    #[default]
    Automatic,
    Fan,
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StackSort {
    #[default]
    Added,
    Modified,
    Name,
}

/// One app's own settings. Anything left out follows the dock-wide setting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_previews: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_effect: Option<LaunchEffect>,
}

/// `"pop"`: the body swells up under the icon once, like the notch's own launch swell, all inside the dock. `"window"`: black liquid flows from the
/// icon into the app's window. `"off"`: neither. Older files' `true` and `false` mean `"pop"` and `"off"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchEffect {
    #[default]
    Pop,
    Window,
    Off,
}

impl<'de> Deserialize<'de> for LaunchEffect {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            On(bool),
            Named(String),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::On(true) => Self::Pop,
            Raw::On(false) => Self::Off,
            Raw::Named(name) => match name.as_str() {
                "window" => Self::Window,
                "off" => Self::Off,
                _ => Self::Pop,
            },
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pinned: Vec::new(),
            others: Vec::new(),
            show_downloads: true,
            side: crate::side::Side::Bottom,
            tile_size: 48.0,
            large_size: 72.0,
            magnification: true,
            launch_effect: LaunchEffect::Pop,
            show_names: false,
            avoid_windows: true,
            minimize_duration: 0.4,
            minimize_effect: MinimizeEffect::Liquid,
            hide_in_fullscreen: true,
            replace_switcher: true,
            window_previews: true,
            glass_tint_top: 0.5,
            glass_tint_bottom: 0.4,
            popup_attach: PopupAttach::Float,
            mission_control: MissionControl::Cover,
            stack_view: StackView::Automatic,
            stack_sort: StackSort::Added,
            displays: Displays::Primary,
            music_pulse: true,
            music_beat: false,
            apps: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(FILE)
    }

    /// Reads `dock.json`, or makes it from what `seed` gives on the first run.
    pub fn load_or_seed(dir: &Path, seed: impl FnOnce() -> Option<Self>) -> Self {
        if let Ok(text) = std::fs::read_to_string(Self::path(dir)) {
            match serde_json::from_str::<Self>(&text) {
                Ok(config) => return config.sane(),
                Err(err) => eprintln!("notchbase-dock: {FILE} could not be read ({err}); using defaults"),
            }
            return Self::default();
        }
        let config = seed().unwrap_or_default().sane();
        config.save(dir);
        config
    }

    pub fn save(&self, dir: &Path) {
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let _ = std::fs::create_dir_all(dir);
        let partial = Self::path(dir).with_extension("json.partial");
        if std::fs::write(&partial, text).is_ok() {
            let _ = std::fs::rename(&partial, Self::path(dir));
        }
    }

    /// Reads `dock.json` as it is now, if it can be read.
    pub fn read(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(dir)).ok()?;
        serde_json::from_str::<Self>(&text).ok().map(Self::sane)
    }

    pub fn sane(mut self) -> Self {
        self.minimize_duration =
            if self.minimize_duration.is_finite() { self.minimize_duration.clamp(0.15, 1.5) } else { 0.4 };
        self.glass_tint_top = self.glass_tint_top.clamp(0.0, 1.0);
        self.glass_tint_bottom = self.glass_tint_bottom.clamp(0.0, 1.0);
        self.tile_size = self.tile_size.clamp(16.0, 128.0);
        self.large_size = self.large_size.clamp(self.tile_size, 160.0);
        if !self.magnification {
            self.large_size = self.tile_size;
        }
        self
    }

    /// The folders shown on the right, in order: Downloads first when it's shown, then the others.
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if self.show_downloads
            && let Some(downloads) = downloads_folder()
        {
            folders.push(downloads);
        }
        for path in &self.others {
            if !folders.contains(path) {
                folders.push(path.clone());
            }
        }
        folders
    }

    /// Effective magnified size: always a little swell so the liquid has something to move with, and never more
    /// than 25% so it stays calm.
    pub fn magnified(&self) -> f32 {
        if self.magnification {
            self.large_size.clamp(self.tile_size * 1.16, self.tile_size * 1.25)
        } else {
            self.tile_size
        }
    }

    /// Whether `app` shows its windows when its icon is rested on.
    pub fn previews_for(&self, app: &Path) -> bool {
        self.apps.get(app).and_then(|a| a.window_previews).unwrap_or(self.window_previews)
    }

    /// How opening `app` from the dock looks.
    pub fn launch_effect_for(&self, app: &Path) -> LaunchEffect {
        self.apps.get(app).and_then(|a| a.launch_effect).unwrap_or(self.launch_effect)
    }

    pub fn is_pinned(&self, path: &Path) -> bool {
        self.pinned.iter().any(|p| p == path)
    }

    pub fn pin(&mut self, path: PathBuf, at: usize) {
        self.pinned.retain(|p| p != &path);
        let at = at.min(self.pinned.len());
        self.pinned.insert(at, path);
    }

    pub fn unpin(&mut self, path: &Path) {
        self.pinned.retain(|p| p != path);
    }
}

/// `~/Downloads`.
pub fn downloads_folder() -> Option<PathBuf> {
    crate::os::home().map(|home| home.join("Downloads"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_round_trip_and_older_files_get_the_defaults() {
        let older: super::Config = serde_json::from_str(r#"{"tile_size": 52}"#).unwrap();
        assert_eq!(older.minimize_effect, super::MinimizeEffect::Liquid);
        assert!(older.hide_in_fullscreen && older.replace_switcher);
        assert!((older.minimize_duration - 0.4).abs() < 1e-6);
        assert_eq!(older.side, crate::side::Side::Bottom, "older files keep the dock along the bottom");
        let left: super::Config = serde_json::from_str(r#"{"side": "left"}"#).unwrap();
        assert_eq!(left.side, crate::side::Side::Left);
        let mut config = super::Config {
            minimize_effect: super::MinimizeEffect::System,
            hide_in_fullscreen: false,
            replace_switcher: false,
            minimize_duration: 0.3,
            ..super::Config::default()
        };
        config.glass_tint_top = 0.8;
        let text = serde_json::to_string(&config).unwrap();
        assert!(text.contains(r#""minimize_effect":"system""#));
        let back: super::Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back, config);
        let wild = super::Config { minimize_duration: 9.0, glass_tint_top: -1.0, ..super::Config::default() }.sane();
        assert!((wild.minimize_duration - 1.5).abs() < 1e-6 && wild.glass_tint_top == 0.0);
    }

    #[test]
    fn app_settings_override_the_dock_wide_ones() {
        let app = std::path::PathBuf::from("/Applications/Mail.app");
        let text = r#"{"apps": {"/Applications/Mail.app": {"window_previews": false}}}"#;
        let config: super::Config = serde_json::from_str(text).unwrap();
        assert!(!config.previews_for(&app));
        assert!(config.previews_for(std::path::Path::new("/Applications/Notes.app")));
    }

    #[test]
    fn launch_effect_reads_old_and_new_values() {
        let read =
            |v: &str| serde_json::from_str::<Config>(&format!("{{\"launch_effect\": {v}}}")).unwrap().launch_effect;
        assert_eq!(read("true"), LaunchEffect::Pop);
        assert_eq!(read("false"), LaunchEffect::Off);
        assert_eq!(read("\"window\""), LaunchEffect::Window);
        assert_eq!(read("\"pop\""), LaunchEffect::Pop);
        assert_eq!(serde_json::to_string(&LaunchEffect::Window).unwrap(), "\"window\"");
    }

    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-dock-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seeds_once_then_reads_its_own_file() {
        let dir = scratch("config");
        let seeded = Config::load_or_seed(&dir, || {
            Some(Config { pinned: vec!["/Applications/Zen.app".into()], ..Config::default() })
        });
        assert_eq!(seeded.pinned.len(), 1);
        let again = Config::load_or_seed(&dir, || panic!("the file exists, so no seeding"));
        assert_eq!(again, seeded);
    }

    #[test]
    fn pinning_moves_rather_than_duplicates() {
        let mut c = Config { pinned: vec!["/a".into(), "/b".into(), "/c".into()], ..Config::default() };
        c.pin("/c".into(), 0);
        assert_eq!(c.pinned, vec![PathBuf::from("/c"), "/a".into(), "/b".into()]);
        c.pin("/c".into(), 9);
        assert_eq!(c.pinned, vec![PathBuf::from("/a"), "/b".into(), "/c".into()], "past the end goes last");
        c.unpin(Path::new("/b"));
        assert_eq!(c.pinned.len(), 2);
    }

    #[test]
    fn sizes_are_kept_in_range() {
        let c = Config { tile_size: 2.0, large_size: 999.0, ..Config::default() }.sane();
        assert_eq!((c.tile_size, c.large_size), (16.0, 160.0));
        let flat = Config { magnification: false, ..Config::default() }.sane();
        assert_eq!(flat.magnified(), flat.tile_size);
    }
}
