//! What a tool can ask the host to do.

use serde::{Deserialize, Serialize};

use super::node::{ImageId, NoticeNode};
use super::values::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingOptions {
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub desktop_audio: bool,
    #[serde(default)]
    pub microphone_id: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub max_fps: Option<u32>,
    #[serde(default = "yes")]
    pub dim_outside: bool,
}

fn yes() -> bool {
    true
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            microphone: false,
            desktop_audio: false,
            microphone_id: None,
            folder: None,
            max_fps: None,
            dim_outside: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaControl {
    PlayPause,
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub calendar_id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl HttpRequest {
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self { method: method.into(), url: url.into(), ..Self::default() }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new("GET", url)
    }

    pub fn post(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new("POST", url).body(body)
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn json(self, body: &impl Serialize) -> Self {
        let text = serde_json::to_string(body).unwrap_or_default();
        self.header("Content-Type", "application/json").body(text)
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout_ms = Some(timeout.as_millis().min(u128::from(u64::MAX)) as u64);
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_str(&self.body).map_err(|err| err.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Notify { text: String },
    Notice { notice: NoticeNode },
    DismissNotice,
    CopyText { text: String },
    CopyImage { image: ImageId },
    Annotate { image: ImageId },
    SaveImage { image: ImageId, folder: Option<String>, prefix: String, format: ImageFormat, reply: Option<u64> },
    PickColor { reply: u64 },
    CaptureScreen { reply: u64 },
    StartRecording { options: RecordingOptions },
    StopRecording,
    CancelCapture,
    SaveSetting { key: String, value: Value },
    Shelve { paths: Vec<String> },
    Reveal { path: String },
    Open { target: String },
    OpenPage,
    OpenSettings,
    Collapse,
    Hold { held: bool },
    Media { control: MediaControl },
    Log { text: String },
    Chime,
    SaveEvent { event: EventDraft },
    DeleteEvent { id: String },
    KeepAwake { awake: bool },
    MuteMicrophone { muted: bool },
    Fetch { request: HttpRequest, reply: u64 },
    ReadFile { path: String, reply: u64 },
    WriteFile { path: String, text: String, reply: Option<u64> },
}
