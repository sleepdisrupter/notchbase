//! Turns Claude Code's hook payloads into events: permission requests and questions become prompts, the rest progress.

use serde_json::Value;

use super::render::Render;
use super::{Plan, describe, fingerprint, text};
use crate::assistant::prompt::{PromptKind, Provider};
use crate::assistant::wire::Signal;

/// Notifications that mean Claude is waiting on you.
const NOTICES: [&str; 5] = [
    "permission_prompt",
    "elicitation_dialog",
    "elicitation_url_dialog",
    "agent_needs_input",
    "quota_auto_resume_stale",
];
/// Sent once Claude has sat waiting for input. It ends the run, since Claude skips Stop when a turn is interrupted.
const IDLE: &str = "idle_prompt";
const PLAN_READY: &str = "Plan ready for review";

pub(super) fn plan(payload: &Value, started_ms: u64) -> Option<Plan> {
    let (signal, render) = match text(payload, "hook_event_name")? {
        "PermissionRequest" => permission(payload)?,
        "PreToolUse" => (Signal::ToolStarted { tool: text(payload, "tool_name")?.to_owned() }, Render::Silent),
        "PostToolUse" | "PostToolUseFailure" => (Signal::ToolDone { key: tool_key(payload)? }, Render::Silent),
        "UserPromptSubmit" => (Signal::TurnStarted, Render::Silent),
        "Stop" => {
            let message = describe::message(text(payload, "last_assistant_message"));
            (Signal::TurnEnded { message }, Render::Silent)
        }
        "Notification" => (notice(payload)?, Render::Silent),
        "SessionEnd" => (Signal::SessionEnded, Render::Silent),
        _ => return None,
    };
    Some(Plan::new(Provider::Claude, payload, started_ms, signal, render))
}

fn permission(payload: &Value) -> Option<(Signal, Render)> {
    let tool = text(payload, "tool_name")?;
    let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
    let suggestions = payload
        .get("permission_suggestions")
        .filter(|suggestions| suggestions.as_array().is_some_and(|list| !list.is_empty()))
        .cloned();
    let prompt = match tool {
        "AskUserQuestion" => describe::questions(&input, true)?,
        "ExitPlanMode" => PromptKind::Attention { message: PLAN_READY.to_owned() },
        _ => describe::permission(tool, &input, suggestions.is_some()),
    };
    let signal = Signal::Ask { key: fingerprint::key(tool, &input), prompt };
    Some((signal, Render::Claude { input, suggestions }))
}

fn tool_key(payload: &Value) -> Option<u64> {
    let tool = text(payload, "tool_name")?;
    Some(fingerprint::key(tool, payload.get("tool_input").unwrap_or(&Value::Null)))
}

fn notice(payload: &Value) -> Option<Signal> {
    let kind = text(payload, "notification_type")?;
    if kind == IDLE {
        return Some(Signal::Idle);
    }
    if !NOTICES.contains(&kind) {
        return None;
    }
    let message = describe::message(text(payload, "message"));
    (!message.is_empty()).then_some(Signal::Attention { message })
}
