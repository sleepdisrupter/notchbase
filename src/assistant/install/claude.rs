//! Claude Code's hooks: a blocking PermissionRequest, background ones for progress and a quick SessionEnd.

use serde_json::{Value, json};

use super::exe;
use super::hooks::Spec;
use crate::assistant::prompt::Provider;
use crate::assistant::timing;

const ARGS: [&str; 2] = ["hook", "claude"];
const SHELL_TAIL: &str = " hook claude";

pub(super) fn spec(exe: &str) -> Spec {
    let blocking = json!({
        "type": "command",
        "command": exe,
        "args": ARGS,
        "timeout": timing::hook_timeout_secs(Provider::Claude),
    });
    let background = json!({ "type": "command", "command": exe, "args": ARGS, "async": true });
    let quick = json!({ "type": "command", "command": exe, "args": ARGS });
    let every_tool = |handler: &Value| json!({ "matcher": "*", "hooks": [handler] });
    let always = |handler: &Value| json!({ "hooks": [handler] });
    Spec {
        groups: vec![
            ("PermissionRequest", every_tool(&blocking)),
            ("PreToolUse", every_tool(&background)),
            ("PostToolUse", every_tool(&background)),
            ("PostToolUseFailure", every_tool(&background)),
            ("UserPromptSubmit", always(&background)),
            ("Stop", always(&background)),
            ("Notification", always(&background)),
            ("SessionEnd", always(&quick)),
        ],
        ours,
    }
}

/// Our handlers run the notchbase executable with `hook claude`, as arguments or at the end of a shell line.
pub(super) fn ours(handler: &Value) -> bool {
    let Some(command) = handler.get("command").and_then(Value::as_str) else {
        return false;
    };
    let args: Vec<&str> = handler
        .get("args")
        .and_then(Value::as_array)
        .map(|args| args.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if args.starts_with(&ARGS) {
        return exe::names_us(command);
    }
    command.trim_end().strip_suffix(SHELL_TAIL).is_some_and(exe::names_us)
}
