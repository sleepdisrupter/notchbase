//! notchbase dock, the separate Dock replacement, as the Settings page sees it: its settings from its own `dock.json`,
//! and the changes the page asks for.

/// The dock's settings as they are now. `found` is false where the dock has never run (no `dock.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct DockView {
    pub found: bool,
    pub running: bool,
    /// The edge of the screen the dock stands on.
    pub side: DockSide,
    pub tile_size: f32,
    pub magnification: bool,
    pub show_names: bool,
    /// How dark the glass is, 0 to 1.
    pub glass: f32,
    pub window_previews: bool,
    pub avoid_windows: bool,
    pub hide_in_fullscreen: bool,
    /// A dock on every screen, rather than only the main one.
    pub all_displays: bool,
    pub mission_control: DockMissionControl,
    pub launch_pop: bool,
    pub minimize_liquid: bool,
    /// Seconds.
    pub minimize_duration: f32,
    pub replace_switcher: bool,
    /// Spotify's or Music's icon swells on the beat while it plays.
    pub music_pulse: bool,
    /// Downloads as a stack on the right.
    pub show_downloads: bool,
    /// How Downloads (and any folder in the dock) opens, and the order it shows its things in.
    pub stack_view: DockStackView,
    pub stack_sort: DockStackSort,
}

impl Default for DockView {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl DockView {
    /// The dock's own defaults, as it starts without a `dock.json`.
    pub const DEFAULT: Self = Self {
        found: false,
        running: false,
        side: DockSide::Bottom,
        tile_size: 48.0,
        magnification: true,
        show_names: false,
        glass: 0.5,
        window_previews: true,
        avoid_windows: true,
        hide_in_fullscreen: true,
        all_displays: false,
        mission_control: DockMissionControl::Cover,
        launch_pop: true,
        minimize_liquid: true,
        minimize_duration: 0.4,
        replace_switcher: true,
        music_pulse: true,
        show_downloads: true,
        stack_view: DockStackView::Automatic,
        stack_sort: DockStackSort::Added,
    };
}

/// The edge of the screen the dock stands on: along the bottom, or down the left or right side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockSide {
    Left,
    #[default]
    Bottom,
    Right,
}

impl DockSide {
    /// In the order they sit on the screen, as Settings lists them.
    pub const ALL: [Self; 3] = [Self::Left, Self::Bottom, Self::Right];

    /// The name `dock.json` and `dock-areas.json` use.
    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Bottom => "bottom",
            Self::Right => "right",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockMissionControl {
    #[default]
    Cover,
    Fade,
}

/// A fan for a few items and a grid for many, or always one of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockStackView {
    #[default]
    Automatic,
    Fan,
    Grid,
}

impl DockStackView {
    pub const ALL: [Self; 3] = [Self::Automatic, Self::Fan, Self::Grid];

    /// The name `dock.json` uses.
    pub fn key(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Fan => "fan",
            Self::Grid => "grid",
        }
    }
}

/// Newest added, newest changed, or by name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockStackSort {
    #[default]
    Added,
    Modified,
    Name,
}

impl DockStackSort {
    pub const ALL: [Self; 3] = [Self::Added, Self::Modified, Self::Name];

    pub fn key(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Name => "name",
        }
    }
}

/// One change to the dock, asked for from Settings › Dock.
#[derive(Clone, Debug, PartialEq)]
pub enum DockChange {
    Running(bool),
    Side(DockSide),
    TileSize(f32),
    Magnification(bool),
    ShowNames(bool),
    Glass(f32),
    WindowPreviews(bool),
    AvoidWindows(bool),
    HideInFullscreen(bool),
    AllDisplays(bool),
    /// macOS only: Windows has no Mission Control.
    #[cfg_attr(windows, allow(dead_code))]
    MissionControl(DockMissionControl),
    LaunchPop(bool),
    MinimizeLiquid(bool),
    MinimizeDuration(f32),
    /// macOS only: the Windows dock has no switcher of its own.
    #[cfg_attr(windows, allow(dead_code))]
    ReplaceSwitcher(bool),
    MusicPulse(bool),
    ShowDownloads(bool),
    StackView(DockStackView),
    StackSort(DockStackSort),
}
