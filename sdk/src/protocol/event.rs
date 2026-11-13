//! What the host sends a tool: events, replies, and the feeds behind subscriptions.

use serde::{Deserialize, Serialize};

use super::canvas::Pointer;
use super::node::{HandlerId, ImageId};
use super::request::HttpResponse;
use super::style::Color;
use super::values::Values;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Tile,
    Page,
    Live,
    Notice,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiValue {
    Press,
    Bool(bool),
    Index(usize),
    Number(f32),
    Text(String),
    Picked { column: usize, index: usize },
    Pointer(Pointer),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Ui { surface: Surface, handler: HandlerId, value: UiValue },
    Shortcut { id: String },
    Tick { every_ms: u64 },
    Settings { values: Values },
    Reply { id: u64, reply: Reply },
    Recording { update: RecordingUpdate },
    AudioInputs { inputs: Vec<AudioInput> },
    Media { now_playing: Option<NowPlaying> },
    Bluetooth { change: DeviceChange },
    Volume { level: VolumeLevel },
    Notch { state: NotchState },
    Calendar { feed: CalendarFeed },
    Power { power: Power },
    ScreenLock { locked: bool },
    Sensors { sensors: Sensors },
    Download { change: DownloadChange },
    Focus { focus: Focus },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Color { color: Option<Color> },
    Capture { capture: Option<Capture> },
    Saved { result: Result<String, String> },
    Fetched { result: Result<HttpResponse, String> },
    Read { result: Result<String, String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub image: ImageId,
    pub width: u32,
    pub height: u32,
    pub on_clipboard: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RecordingState {
    #[default]
    Idle,
    Choosing,
    Starting,
    Recording {
        since: u64,
    },
    Saving,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordingUpdate {
    State { state: RecordingState },
    Saved { path: String, seconds: f64, note: Option<String> },
    Failed { error: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInput {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub battery: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceChange {
    Connected { device: Device },
    Disconnected { device: Device },
    Updated { device: Device },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CalendarFeed {
    Unavailable,
    Waiting,
    Denied,
    Events {
        events: Vec<CalendarEvent>,
        #[serde(default)]
        calendars: Vec<CalendarInfo>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub writable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub calendar: String,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub declined: bool,
    #[serde(default)]
    pub calendar_id: String,
    #[serde(default)]
    pub editable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Power {
    pub level: Option<u8>,
    #[serde(default)]
    pub plugged: bool,
    #[serde(default)]
    pub charging: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sensors {
    #[serde(default)]
    pub camera: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    pub active: bool,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub id: String,
    pub name: String,
    pub received: u64,
    #[serde(default)]
    pub total: Option<u64>,
}

impl Download {
    pub fn share(&self) -> Option<f32> {
        let total = self.total.filter(|total| *total > 0)?;
        Some((self.received as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadChange {
    Progress { download: Download },
    Finished { download: Download, path: String },
    Cancelled { download: Download },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolumeLevel {
    pub volume: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotchState {
    pub open: bool,
    pub page: Option<String>,
    #[serde(default)]
    pub phase: NotchPhase,
    #[serde(default)]
    pub style: NotchStyle,
    #[serde(default)]
    pub expansion: f32,
    #[serde(default)]
    pub hug: f32,
    #[serde(default)]
    pub float: f32,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub notch: Option<[f32; 2]>,
    #[serde(default)]
    pub held: bool,
    #[serde(default)]
    pub settled: bool,
    #[serde(default)]
    pub activities: Vec<String>,
    #[serde(default)]
    pub music: bool,
}

impl NotchState {
    pub fn hugged(&self) -> bool {
        self.hug >= 0.99
    }

    pub fn floating(&self) -> bool {
        self.hug <= 0.01 && self.float >= 0.99
    }

    pub fn opening(&self) -> bool {
        self.expansion > 0.0 && self.expansion < 1.0
    }

    pub fn fully_open(&self) -> bool {
        self.expansion >= 1.0
    }

    pub fn resting(&self) -> bool {
        self.phase == NotchPhase::Resting && self.settled
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotchPhase {
    #[default]
    Resting,
    Peeking,
    Open,
    Notice,
    Dropping,
    Asking,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotchStyle {
    #[default]
    Notch,
    Island,
}
