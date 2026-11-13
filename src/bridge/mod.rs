//! The types the app and the notch pass each other. Each frame the app fills a `PanelData`, and the
//! notch hands back the `Action`s for whatever was pressed.

mod action;
mod agents;
mod app;
mod dock;
mod live;
mod page;
mod tools;
mod welcome;

pub use crate::find::view::*;
pub use action::*;
pub use agents::*;
pub use app::*;
pub use dock::*;
pub use live::*;
pub use page::*;
pub use tools::*;
pub use welcome::*;

use crate::assistant::Provider;
use crate::audio::Mixer;
use crate::media::NowPlaying;
use crate::shelf::{Notice, Shelf};
use crate::stats::Snapshot;
use crate::tools::ImageStore;
use crate::tools::shortcuts::ShortcutView;

pub struct PanelData<'a> {
    pub shortcuts: &'a [ShortcutView],
    pub tabs: &'a [Tab],
    pub tools: &'a [ToolView],
    pub catalog: &'a [ToolInfo],
    pub locations: &'a Locations,
    pub images: &'a ImageStore,
    pub mixer: &'a Mixer,
    pub now_playing: Option<&'a NowPlaying>,
    pub stats: &'a Snapshot,
    pub shelf: &'a Shelf,
    pub shelf_notice: Option<&'a Notice>,
    pub agents: &'a AgentsView,
    pub find: &'a FindView,
    pub welcome: &'a WelcomeView,
    pub typing: bool,
    pub assistant: Option<Provider>,
    pub questions_in_notch: bool,
    pub progress_in_notch: bool,
    pub volume_indicator: bool,
    pub brightness_indicator: bool,
    pub all_displays: bool,
    pub wake_at_edge: bool,
    pub flee_from_keys: bool,
    pub haptics: bool,
    pub dodge_menus: bool,
    pub snap_windows: bool,
    pub lock_screen: bool,
    pub app: AppView,
    pub dock: &'a DockView,
}

impl PanelData<'_> {
    pub fn tool(&self, slot: ToolSlot) -> Option<&ToolView> {
        self.tools.iter().find(|tool| tool.slot == slot)
    }
}
