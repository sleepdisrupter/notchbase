//! Live activities: what shows in the closed notch while something is going on.

use notchbase_sdk::protocol::LiveNode;

use crate::assistant::Provider;

use super::ToolSlot;

#[derive(Clone, Debug, PartialEq)]
pub enum Activity {
    Tool(ToolSlot, LiveNode),
    Agent(AgentLive),
    Video(VideoLive),
    Lock(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoLive {
    pub id: u64,
    pub site: crate::webvideo::Site,
    pub audio: bool,
    pub stage: VideoStage,
    pub share: f32,
    pub title: String,
    pub detail: String,
    pub cover: Option<std::sync::Arc<egui::ColorImage>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoStage {
    Preparing,
    Finding,
    Downloading,
    Joining,
    Saved,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentLive {
    pub provider: Provider,
    pub mood: Mood,
    pub since_ms: u64,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Thinking,
    Doing(&'static str),
    Waiting,
}
