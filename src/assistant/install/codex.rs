//! Codex's hooks, all shell commands: a blocking PermissionRequest, a short PreToolUse for its questions,
//! background ones for progress and a quick SessionEnd.

use serde_json::{Value, json};

use super::exe;
use super::hooks::Spec;
use crate::assistant::prompt::Provider;
use crate::assistant::timing;

const TAIL: &str = "hook codex";
const WAITING: &str = "Waiting for notchbase";
const QUESTION_MATCHER: &str = "^request_user_input$";
const QUESTION_TIMEOUT_SECS: u64 = 10;

pub(super) fn spec(exe: &str) -> Spec {
    spec_for(exe, cfg!(windows))
}

pub(super) fn spec_for(exe: &str, windows: bool) -> Spec {
    let command = exe::shell_command(exe, TAIL, windows);
    let blocking = json!({
        "type": "command",
        "command": command,
        "timeout": timing::hook_timeout_secs(Provider::Codex),
        "statusMessage": WAITING,
    });
    let question = json!({ "type": "command", "command": command, "timeout": QUESTION_TIMEOUT_SECS });
    let background = json!({ "type": "command", "command": command, "async": true });
    let quick = json!({ "type": "command", "command": command });
    let matching = |matcher: &str, handler: &Value| json!({ "matcher": matcher, "hooks": [handler] });
    let always = |handler: &Value| json!({ "hooks": [handler] });
    Spec {
        groups: vec![
            ("PermissionRequest", matching("*", &blocking)),
            ("PreToolUse", matching(QUESTION_MATCHER, &question)),
            ("PostToolUse", matching("*", &background)),
            ("UserPromptSubmit", always(&background)),
            ("Stop", always(&background)),
            ("SessionEnd", always(&quick)),
        ],
        ours,
    }
}

pub(super) fn ours(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .and_then(|command| command.trim_end().strip_suffix(TAIL))
        .is_some_and(|exe| exe.ends_with(char::is_whitespace) && exe::names_us(exe))
}
