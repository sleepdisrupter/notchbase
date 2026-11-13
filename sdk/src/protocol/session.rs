//! The calls and answers that make up a session with a tool.

use serde::{Deserialize, Serialize};

use super::event::Event;
use super::manifest::Manifest;
use super::node::{LiveNode, Node, TileNode};
use super::request::Request;
use super::values::Values;

// Bump this when the wire format changes. New fields need `#[serde(default)]` so older hosts
// and tools can still read each other.
pub const PROTOCOL_VERSION: u32 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    #[serde(rename = "macos")]
    MacOs,
    #[default]
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dirs {
    pub data: String,
    pub home: Option<String>,
    pub pictures: Option<String>,
    pub videos: Option<String>,
    pub desktop: Option<String>,
    pub downloads: Option<String>,
    #[serde(default)]
    pub documents: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Start {
    pub settings: Values,
    pub dirs: Dirs,
    pub platform: Platform,
    pub host_version: String,
    #[serde(default)]
    pub clock_24h: bool,
    #[serde(default)]
    pub time_zone: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wants {
    #[serde(default)]
    pub every_ms: Vec<u64>,
    #[serde(default)]
    pub settings: bool,
    #[serde(default)]
    pub recording: bool,
    #[serde(default)]
    pub audio_inputs: bool,
    #[serde(default)]
    pub media: bool,
    #[serde(default)]
    pub bluetooth: bool,
    #[serde(default)]
    pub volume: bool,
    #[serde(default)]
    pub notch: bool,
    #[serde(default)]
    pub calendar: bool,
    #[serde(default)]
    pub calendar_range: Option<[u64; 2]>,
    #[serde(default)]
    pub power: bool,
    #[serde(default)]
    pub screen_lock: bool,
    #[serde(default)]
    pub sensors: bool,
    #[serde(default)]
    pub downloads: bool,
    #[serde(default)]
    pub focus: bool,
}

/// What a tool hands back after each call. `revision` goes up with every update, so the host knows
/// when to ask for the views again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub requests: Vec<Request>,
    pub wants: Wants,
    pub revision: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Views {
    pub tile: Option<TileNode>,
    pub page: Option<Node>,
    pub live: Option<LiveNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum Call {
    Manifest,
    Start { start: Box<Start> },
    Handle { events: Vec<Event> },
    Views,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    Manifest { manifest: Manifest },
    Effects { effects: Effects },
    Views { views: Box<Views> },
    Failed { error: String },
}
