//! The hook client: `notchbase hook claude` or `notchbase hook codex`, run by the assistant with the payload on stdin.
//! It always exits 0 and prints nothing or a valid decision, so a problem here never blocks the assistant.

mod claude;
mod client;
mod codex;
mod describe;
mod fingerprint;
mod render;
mod window;

#[cfg(test)]
mod tests;

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::endpoint;
use super::prompt::Provider;
use super::wire::{Event, Signal};
use render::Render;

const HOOK: &str = "hook";
const MAX_PAYLOAD: u64 = 64 * 1024 * 1024;

struct Plan {
    event: Event,
    render: Render,
}

impl Plan {
    fn new(provider: Provider, payload: &Value, started_ms: u64, signal: Signal, render: Render) -> Self {
        let owned = |field| text(payload, field).map(str::to_owned);
        let event = Event {
            provider,
            session: owned("session_id").unwrap_or_default(),
            agent: owned("agent_id"),
            agent_type: owned("agent_type"),
            cwd: owned("cwd").unwrap_or_default(),
            started_ms,
            window: None,
            signal,
        };
        Self { event, render }
    }
}

pub(super) fn is_launch(first: Option<&OsStr>) -> bool {
    first.is_some_and(|first| first == HOOK)
}

pub(super) fn run_from_args() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if !is_launch(args.first().map(OsString::as_os_str)) {
        return None;
    }
    // A panic must stay quiet, since whatever we print goes to the assistant.
    std::panic::set_hook(Box::new(|_| {}));
    let rest: Vec<String> = args[1..].iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let dir = crate::platform::data_dir();
    let output = std::panic::catch_unwind(|| respond(&rest, std::io::stdin().lock(), dir.as_deref())).ok().flatten();
    if let Some(output) = output {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(output.as_bytes()).and_then(|()| stdout.flush());
    }
    Some(0)
}

pub(super) fn respond(args: &[String], input: impl Read, dir: Option<&Path>) -> Option<String> {
    let provider = match args.first()?.as_str() {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return None,
    };
    let started_ms = now_ms();
    // Codex's legacy notify program passes its JSON as an argument instead of stdin.
    let notice = args.get(1).filter(|_| provider == Provider::Codex);
    let payload = if notice.is_none() { read_payload(input) } else { None };
    // No assistant.json means notchbase isn't running, so the assistant asks on its own.
    let endpoint = endpoint::read(dir?)?;
    let mut plan = match (provider, notice) {
        (Provider::Codex, Some(notice)) => codex::notify(notice, started_ms),
        (Provider::Claude, _) => claude::plan(&payload?, started_ms),
        (Provider::Codex, None) => codex::plan(&payload?, started_ms),
    }?;
    plan.event.window = window::owner();
    client::deliver(&endpoint, plan)
}

fn read_payload(input: impl Read) -> Option<Value> {
    let mut bytes = Vec::new();
    input.take(MAX_PAYLOAD + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_PAYLOAD {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn text<'a>(payload: &'a Value, field: &str) -> Option<&'a str> {
    payload.get(field)?.as_str()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}
