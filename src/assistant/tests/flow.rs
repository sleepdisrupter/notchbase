use std::io::{BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::assistant::endpoint::{self, Endpoint};
use crate::assistant::prompt::{Prompt, PromptKind, Provider, Reply};
use crate::assistant::wire::{self, Answer, Envelope, Event, Signal};
use crate::assistant::{hook, timing};

use super::{Running, TempDir, eventually, running};

fn permission_request(session: &str) -> Value {
    json!({
        "session_id": session,
        "transcript_path": "/tmp/t.jsonl",
        "cwd": "/Users/me/projects/rocket",
        "permission_mode": "default",
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": { "command": "cargo publish", "description": "Publish the crate" },
    })
}

fn spawn_hook(provider: &str, payload: &Value, dir: &Path) -> JoinHandle<Option<String>> {
    let args = vec![provider.to_owned()];
    let input = payload.to_string().into_bytes();
    let dir: PathBuf = dir.to_path_buf();
    thread::spawn(move || hook::respond(&args, input.as_slice(), Some(&dir)))
}

fn send(provider: &str, payload: &Value, dir: &Path) -> Option<String> {
    hook::respond(&[provider.to_owned()], payload.to_string().as_bytes(), Some(dir))
}

fn only_prompt(prompts: Vec<Prompt>) -> Option<Prompt> {
    (prompts.len() == 1).then(|| prompts.into_iter().next()).flatten()
}

fn raw_connect(dir: &Path) -> (TcpStream, Endpoint) {
    let endpoint = endpoint::read(dir).expect("an endpoint file");
    let stream = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port))).unwrap();
    (stream, endpoint)
}

fn raw_ask(token: &str) -> Vec<u8> {
    let event = Event {
        provider: Provider::Claude,
        session: "raw".into(),
        agent: None,
        agent_type: None,
        cwd: "/x".into(),
        started_ms: 1,
        window: None,
        signal: Signal::Ask {
            key: 1,
            prompt: PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), always: false },
        },
    };
    wire::encode(&Envelope { token: token.into(), event }).unwrap()
}

#[test]
fn permission_answered_in_the_notch() {
    let app = running(timing::hold);
    let client = spawn_hook("claude", &permission_request("s1"), app.dir.root());
    let prompt = eventually("the permission prompt", || only_prompt(app.assistant.prompts()));
    assert_eq!(prompt.project, "rocket");
    assert_eq!(
        prompt.kind,
        PromptKind::Permission { tool: "Bash".into(), detail: "cargo publish".into(), always: false }
    );
    assert!(app.wakes.load(Ordering::SeqCst) >= 1);

    app.assistant.reply(prompt.id, Reply::Allow);
    let output: Value = serde_json::from_str(&client.join().unwrap().expect("a decision")).unwrap();
    assert_eq!(
        output,
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "allow" } } })
    );
    assert!(app.assistant.prompts().is_empty());
}

#[test]
fn question_picked_in_notch_answers() {
    let app = running(timing::hold);
    let payload = json!({
        "session_id": "s1", "cwd": "/r", "hook_event_name": "PermissionRequest", "tool_name": "AskUserQuestion",
        "tool_input": { "questions": [{ "question": "Deploy now?", "header": "Deploy", "multiSelect": false,
            "options": [{ "label": "Yes", "description": "" }, { "label": "Not yet", "description": "" }] }] },
    });
    let client = spawn_hook("claude", &payload, app.dir.root());
    let prompt = eventually("the question", || only_prompt(app.assistant.prompts()));
    assert!(matches!(prompt.kind, PromptKind::Questions { answerable: true, .. }));
    app.assistant.reply(prompt.id, Reply::Answers(vec![vec![1]]));
    let output: Value = serde_json::from_str(&client.join().unwrap().unwrap()).unwrap();
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["updatedInput"]["answers"],
        json!({ "Deploy now?": "Not yet" })
    );
}

#[test]
fn answer_in_app_prints_nothing() {
    let app = running(timing::hold);
    let client = spawn_hook("codex", &permission_request("thr"), app.dir.root());
    let prompt = eventually("the codex prompt", || only_prompt(app.assistant.prompts()));
    assert_eq!(prompt.provider, Provider::Codex);
    app.assistant.reply(prompt.id, Reply::InApp);
    assert_eq!(client.join().unwrap(), None);
}

#[test]
fn later_stop_leaves_finished_note() {
    let app = running(timing::hold);
    let client = spawn_hook("claude", &permission_request("s1"), app.dir.root());
    eventually("the permission prompt", || only_prompt(app.assistant.prompts()));
    thread::sleep(Duration::from_millis(5));
    let stop =
        json!({ "session_id": "s1", "cwd": "/r", "hook_event_name": "Stop", "last_assistant_message": "Shipped." });
    assert_eq!(send("claude", &stop, app.dir.root()), None);
    assert_eq!(client.join().unwrap(), None);
    let prompt = eventually("the finished note", || only_prompt(app.assistant.prompts()));
    assert_eq!(prompt.kind, PromptKind::Finished { message: "Shipped.".into() });
}

#[test]
fn terminal_answer_clears_permission() {
    let app = running(timing::hold);
    let client = spawn_hook("claude", &permission_request("s1"), app.dir.root());
    eventually("the permission prompt", || only_prompt(app.assistant.prompts()));
    thread::sleep(Duration::from_millis(5));
    let mut done = permission_request("s1");
    done["hook_event_name"] = json!("PostToolUse");
    done["tool_response"] = json!({ "stdout": "ok" });
    send("claude", &done, app.dir.root());
    assert_eq!(client.join().unwrap(), None);
    assert!(app.assistant.prompts().is_empty());
}

#[test]
fn prompt_vanishes_when_hook_exits() {
    let app = running(timing::hold);
    let (mut stream, endpoint) = raw_connect(app.dir.root());
    stream.write_all(&raw_ask(&endpoint.token)).unwrap();
    eventually("the raw prompt", || only_prompt(app.assistant.prompts()));
    drop(stream);
    eventually("the prompt to vanish", || app.assistant.prompts().is_empty().then_some(()));
}

#[test]
fn hold_expiry_releases_the_hook() {
    let app = running(|_, _| Duration::from_millis(300));
    let started = Instant::now();
    let client = spawn_hook("claude", &permission_request("s1"), app.dir.root());
    assert_eq!(client.join().unwrap(), None);
    assert!(started.elapsed() >= Duration::from_millis(250));
    assert!(app.assistant.prompts().is_empty());
}

#[test]
fn bad_tokens_and_long_lines_rejected() {
    let app = running(timing::hold);
    let (mut stream, _) = raw_connect(app.dir.root());
    stream.write_all(&raw_ask("not-the-token")).unwrap();
    let mut rest = Vec::new();
    assert_eq!(stream.read_to_end(&mut rest).unwrap_or(0), 0);

    let (mut stream, endpoint) = raw_connect(app.dir.root());
    let mut line = raw_ask(&endpoint.token);
    line.pop();
    line.extend(std::iter::repeat_n(b' ', wire::MAX_LINE));
    line.push(b'\n');
    let _ = stream.write_all(&line);
    let _ = stream.read_to_end(&mut rest);

    let (mut stream, _) = raw_connect(app.dir.root());
    stream.write_all(b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
    let _ = stream.read_to_end(&mut rest);
    assert!(app.assistant.prompts().is_empty());
}

#[test]
fn disabled_provider_releases_at_once() {
    let app = running(timing::hold);
    app.assistant.set_enabled(Provider::Codex, false);
    assert_eq!(spawn_hook("codex", &permission_request("thr"), app.dir.root()).join().unwrap(), None);
    assert!(app.assistant.prompts().is_empty());
}

#[test]
fn stopping_releases_hooks_and_endpoint() {
    let Running { assistant, dir, .. } = running(timing::hold);
    let client = spawn_hook("claude", &permission_request("s1"), dir.root());
    eventually("the permission prompt", || only_prompt(assistant.prompts()));
    drop(assistant);
    assert_eq!(client.join().unwrap(), None);
    assert!(endpoint::read(dir.root()).is_none());
}

#[test]
fn no_app_means_a_quick_silent_exit() {
    let dir = TempDir::new();
    let started = Instant::now();
    assert_eq!(send("claude", &permission_request("s"), dir.root()), None);
    endpoint::publish(dir.root(), &Endpoint { port: 9, token: "stale".into() }).unwrap();
    assert_eq!(send("claude", &permission_request("s"), dir.root()), None);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn answers_decode_on_the_client_side() {
    let app = running(timing::hold);
    let (mut stream, endpoint) = raw_connect(app.dir.root());
    stream.write_all(&raw_ask(&endpoint.token)).unwrap();
    let prompt = eventually("the raw prompt", || only_prompt(app.assistant.prompts()));
    app.assistant.reply(prompt.id, Reply::Deny);
    let answer: Answer = wire::decode(&mut BufReader::new(&stream), wire::MAX_ANSWER).unwrap();
    assert_eq!(answer, Answer::Deny);
}
