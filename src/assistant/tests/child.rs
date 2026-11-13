use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{TempDir, eventually, running};
use crate::assistant::prompt::Reply;
use crate::assistant::timing;

const BINARY: &str = if cfg!(windows) { "notchbase.exe" } else { "notchbase" };

const BINARY_OVERRIDE: &str = "NOTCHBASE_TEST_BINARY";

fn built_binary() -> PathBuf {
    let binary = std::env::var_os(BINARY_OVERRIDE).map(PathBuf::from).unwrap_or_else(|| {
        let test_exe = std::env::current_exe().unwrap();
        test_exe.parent().and_then(Path::parent).unwrap().join(BINARY)
    });
    assert!(
        binary.exists(),
        "{} is missing: run `cargo build` before the ignored child-process tests",
        binary.display()
    );
    binary
}

fn spawn(data_dir: &Path, payload: &Value) -> Child {
    let mut child = Command::new(built_binary())
        .args(["hook", "claude"])
        .env("NOTCHBASE_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload.to_string().as_bytes()).unwrap();
    child
}

fn finish(child: Child) -> Output {
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "exit status {:?}", output.status);
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    output
}

fn permission_request() -> Value {
    json!({
        "session_id": "child", "cwd": "/Users/me/rocket", "hook_event_name": "PermissionRequest",
        "tool_name": "Bash", "tool_input": { "command": "rm -rf target" },
    })
}

#[test]
#[ignore = "needs the notchbase binary from `cargo build`"]
fn real_binary_answers_permission() {
    let app = running(timing::hold);
    let child = spawn(app.dir.root(), &permission_request());
    let prompt = eventually("the prompt from the child", || app.assistant.prompts().into_iter().next());
    app.assistant.reply(prompt.id, Reply::Deny);
    let output = finish(child);
    let decision: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(decision["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
    assert_eq!(decision["hookSpecificOutput"]["decision"]["behavior"], "deny");
}

#[test]
#[ignore = "needs the notchbase binary from `cargo build`"]
fn real_binary_stays_silent() {
    let empty = TempDir::new();
    let started = Instant::now();
    let output = finish(spawn(empty.root(), &permission_request()));
    assert!(output.stdout.is_empty());
    assert!(started.elapsed() < Duration::from_secs(3));

    let app = running(timing::hold);
    let stop = json!({ "session_id": "child", "cwd": "/r", "hook_event_name": "Stop", "last_assistant_message": "ok" });
    let output = finish(spawn(app.dir.root(), &stop));
    assert!(output.stdout.is_empty());
    eventually("the finished note", || app.assistant.prompts().into_iter().next());
}
