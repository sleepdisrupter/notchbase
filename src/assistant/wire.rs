//! The line protocol between hook and server: one JSON line each way on the loopback, 256 KiB in and 1 KiB back.

use std::io::{BufRead, Read};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::prompt::{PromptKind, Provider};

pub(super) const MAX_LINE: usize = 256 * 1024;
pub(super) const MAX_ANSWER: usize = 1024;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Envelope {
    pub(super) token: String,
    pub(super) event: Event,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Event {
    pub(super) provider: Provider,
    pub(super) session: String,
    pub(super) agent: Option<String>,
    pub(super) agent_type: Option<String>,
    pub(super) cwd: String,
    pub(super) started_ms: u64,
    /// The process id of the window the assistant runs in. Older hooks leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) window: Option<u32>,
    pub(super) signal: Signal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Signal {
    Ask { key: u64, prompt: PromptKind },
    Note { key: u64, prompt: PromptKind },
    Attention { message: String },
    ToolStarted { tool: String },
    ToolDone { key: u64 },
    TurnStarted,
    TurnEnded { message: String },
    Idle,
    SessionEnded,
}

impl Signal {
    pub(super) fn holds(&self) -> bool {
        matches!(self, Self::Ask { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub(super) enum Answer {
    Allow,
    Always,
    Deny,
    Answers { picks: Vec<Vec<usize>> },
    Release,
}

pub(super) fn encode<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let mut line = serde_json::to_vec(value).ok()?;
    line.push(b'\n');
    (line.len() <= MAX_LINE).then_some(line)
}

/// Reads one line of at most `cap` bytes. A longer, unfinished or malformed line reads as nothing.
pub(super) fn decode<T: DeserializeOwned>(reader: &mut impl BufRead, cap: usize) -> Option<T> {
    let mut line = Vec::new();
    reader.take(cap as u64 + 1).read_until(b'\n', &mut line).ok()?;
    if line.len() > cap || line.last() != Some(&b'\n') {
        return None;
    }
    serde_json::from_slice(&line).ok()
}
