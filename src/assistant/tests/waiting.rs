use std::time::Instant;

use crate::assistant::board::Board;
use crate::assistant::prompt::{PromptKind, Provider, Reply};
use crate::assistant::wire::{Answer, Event, Signal};

fn event(provider: Provider, session: &str, agent: Option<&str>, started_ms: u64, signal: Signal) -> Event {
    Event {
        provider,
        session: session.into(),
        agent: agent.map(Into::into),
        agent_type: agent.map(|_| "Explore".into()),
        cwd: format!("/work/{session}"),
        started_ms,
        window: None,
        signal,
    }
}

fn claude(session: &str, agent: Option<&str>, started_ms: u64, signal: Signal) -> Event {
    event(Provider::Claude, session, agent, started_ms, signal)
}

fn ask(key: u64) -> Signal {
    Signal::Ask { key, prompt: PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), always: false } }
}

fn waiting(board: &Board, session: &str) -> bool {
    board.runs().iter().any(|run| run.session == session && run.waiting)
}

#[test]
fn subagent_timeout_stops_waiting() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    let held = board.apply(claude("s", Some("a1"), 2_000, ask(1)), now).unwrap();
    assert!(waiting(&board, "s"));
    let version = board.version();
    board.forget(held.id);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Release));
    assert!(!waiting(&board, "s"));
    assert_eq!(board.runs().len(), 1, "the run keeps going, it just stops waiting");
    assert!(board.version() > version);
}

#[test]
fn other_session_end_keeps_waiting() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    let held = board.apply(claude("s", None, 2_000, ask(1)), now).unwrap();
    board.apply(claude("t", None, 1_000, Signal::TurnStarted), now);
    board.apply(claude("t", Some("a2"), 2_000, ask(2)), now);
    board.apply(claude("t", None, 3_000, Signal::SessionEnded), now);
    assert!(waiting(&board, "s"));
    assert!(held.answers.try_recv().is_err());
    assert_eq!(board.prompts().len(), 1);
}

#[test]
fn subagent_moving_on_keeps_waiting() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    board.apply(claude("s", None, 2_000, ask(1)), now);
    board.apply(claude("s", Some("a1"), 2_000, ask(2)), now);
    board.apply(claude("s", Some("a1"), 3_000, Signal::ToolDone { key: 2 }), now);
    assert_eq!(board.prompts().len(), 1);
    assert!(board.runs().iter().all(|run| !run.waiting), "a tool finishing means the session is busy again");
    board.apply(claude("s", None, 4_000, ask(3)), now);
    let subagent = board.apply(claude("s", Some("a1"), 4_000, ask(4)), now).unwrap();
    board.forget(subagent.id);
    assert!(waiting(&board, "s"), "the main agent's prompt is still held");
}

#[test]
fn expired_prompts_stop_waiting() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    board.apply(claude("s", None, 2_000, Signal::Attention { message: "Claude needs input".into() }), now);
    assert!(waiting(&board, "s"));
    board.sweep(now + crate::assistant::timing::ATTENTION_TTL);
    assert!(!waiting(&board, "s"));
}

#[test]
fn answering_one_of_two_keeps_waiting() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    let first = board.apply(claude("s", None, 2_000, ask(1)), now).unwrap();
    let second = board.apply(claude("s", Some("a1"), 2_000, ask(2)), now).unwrap();
    board.reply(first.id, Reply::Allow, 2_500);
    assert!(waiting(&board, "s"));
    board.reply(second.id, Reply::Allow, 2_600);
    assert!(!waiting(&board, "s"));
}

#[test]
fn denying_claude_ends_run_quietly() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    let held = board.apply(claude("s", None, 2_000, ask(1)), now).unwrap();
    let version = board.version();
    board.reply(held.id, Reply::Deny, 5_000);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Deny));
    assert!(board.runs().is_empty());
    assert!(board.finished().is_empty(), "an interrupted turn is not done");
    assert!(board.version() > version);
    board.apply(claude("s", None, 4_000, Signal::ToolDone { key: 9 }), now);
    assert!(board.runs().is_empty(), "a hook from before the deny doesn't bring it back");
    board.apply(claude("s", None, 6_000, Signal::TurnStarted), now);
    assert_eq!(board.runs().len(), 1, "the next turn starts a run again");
}

#[test]
fn denying_anything_else_keeps_the_run() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event(Provider::Codex, "c", None, 1_000, Signal::TurnStarted), now);
    let held = board.apply(event(Provider::Codex, "c", None, 2_000, ask(1)), now).unwrap();
    board.reply(held.id, Reply::Deny, 5_000);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Deny));
    let runs = board.runs();
    assert_eq!(runs.len(), 1);
    assert!(!runs[0].waiting);

    board.apply(claude("s", None, 1_000, Signal::TurnStarted), now);
    let note = board.apply(claude("s", None, 2_000, Signal::Attention { message: "Look".into() }), now);
    assert!(note.is_none());
    let id = board.prompts().iter().find(|prompt| prompt.session == "s").unwrap().id;
    board.reply(id, Reply::Deny, 5_000);
    assert!(board.runs().iter().any(|run| run.session == "s" && !run.waiting));
}
