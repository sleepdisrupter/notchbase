//! The `Tool` trait and the `Context` a tool starts with.

use std::path::PathBuf;

use crate::command::Command;
use crate::element::Element;
use crate::protocol::{Dirs, Manifest, Platform, Start};
use crate::settings::Settings;
use crate::subscription::Subscription;
use crate::surface::{Live, Tile};

/// A notchbase tool. The host calls `init` once and `update` for every message, and asks for the
/// views again whenever the model changes.
pub trait Tool: Sized + 'static {
    type Msg: Clone + Send + 'static;

    /// Who the tool is: its id, name, page, settings and shortcuts.
    fn manifest() -> Manifest;

    /// Builds the starting model. The returned command runs straight away.
    fn init(context: &Context) -> (Self, Command<Self::Msg>);

    /// Handles one message and says what should happen next.
    fn update(&mut self, msg: Self::Msg) -> Command<Self::Msg>;

    /// What the tool listens to. The host asks again after every round of updates.
    fn subscriptions(&self) -> Subscription<Self::Msg> {
        Subscription::none()
    }

    /// The tool's tile on the Tools page.
    fn tile(&self) -> Option<Tile<Self::Msg>> {
        None
    }

    /// The tool's own page, if it has one.
    fn page(&self) -> Option<Element<Self::Msg>> {
        None
    }

    /// A live activity for the closed notch, like a running timer.
    fn live(&self) -> Option<Live<Self::Msg>> {
        None
    }
}

/// What a tool is given at the start: its settings, folders, platform and the user's clock settings.
#[derive(Clone, Debug)]
pub struct Context {
    settings: Settings,
    dirs: Dirs,
    platform: Platform,
    host_version: String,
    clock_24h: bool,
    time_zone: Option<String>,
}

impl Context {
    pub(crate) fn new(start: Start, settings: Settings) -> Self {
        Self {
            settings,
            dirs: start.dirs,
            platform: start.platform,
            host_version: start.host_version,
            clock_24h: start.clock_24h,
            time_zone: start.time_zone,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn clock_24h(&self) -> bool {
        self.clock_24h
    }

    pub fn time_zone(&self) -> Option<&str> {
        self.time_zone.as_deref()
    }

    /// The tool's own folder for files it keeps.
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.dirs.data)
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn host_version(&self) -> &str {
        &self.host_version
    }
}
