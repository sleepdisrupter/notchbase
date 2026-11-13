//! Turns Codex's hook payloads into events. Its own questions are shown but not answered, and the legacy notify
//! program's turn-complete counts as a turn ending.

use serde_json::Value;

use super::render::Render;
use super::{Plan, describe, fingerprint, text};
use crate::assistant::prompt::Provider;
use crate::assistant::wire::{Event, Signal};

const QUESTION_TOOL: &str = "request_user_input";
const TURN_COMPLETE: &str = "agent-turn-complete";

pub(super) fn plan(payload: &Value, started_ms: u64) -> Option<Plan> {
    let (signal, render) = match text(payload, "hook_event_name")? {
        "PermissionRequest" => {
            let (tool, input) = tool(payload)?;
            let signal =
                Signal::Ask { key: fingerprint::key(tool, input), prompt: describe::permission(tool, input, false) };
            (signal, Render::Codex)
        }
        "PreToolUse" => {
            let (tool, input) = tool(payload).filter(|(tool, _)| *tool == QUESTION_TOOL)?;
            let signal =
                Signal::Note { key: fingerprint::key(tool, input), prompt: describe::questions(input, false)? };
            (signal, Render::Silent)
        }
        "PostToolUse" => {
            let (tool, input) = tool(payload)?;
            (Signal::ToolDone { key: fingerprint::key(tool, input) }, Render::Silent)
        }
        "UserPromptSubmit" => (Signal::TurnStarted, Render::Silent),
        "Stop" => {
            let message = describe::message(text(payload, "last_assistant_message"));
            (Signal::TurnEnded { message }, Render::Silent)
        }
        "SessionEnd" => (Signal::SessionEnded, Render::Silent),
        _ => return None,
    };
    Some(Plan::new(Provider::Codex, payload, started_ms, signal, render))
}

pub(super) fn notify(argument: &str, started_ms: u64) -> Option<Plan> {
    let payload: Value = serde_json::from_str(argument).ok()?;
    if text(&payload, "type")? != TURN_COMPLETE {
        return None;
    }
    let event = Event {
        provider: Provider::Codex,
        session: text(&payload, "thread-id").unwrap_or_default().to_owned(),
        agent: None,
        agent_type: None,
        cwd: text(&payload, "cwd").unwrap_or_default().to_owned(),
        started_ms,
        window: None,
        signal: Signal::TurnEnded { message: describe::message(text(&payload, "last-assistant-message")) },
    };
    Some(Plan { event, render: Render::Silent })
}

fn tool(payload: &Value) -> Option<(&str, &Value)> {
    Some((text(payload, "tool_name")?, payload.get("tool_input").unwrap_or(&Value::Null)))
}
