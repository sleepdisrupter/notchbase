use std::time::{Duration, Instant};

use crate::assistant::board::Board;
use crate::assistant::prompt::{PromptKind, Provider, Reply};
use crate::assistant::wire::{Event, Signal};

const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;

fn event(session: &str, started_ms: u64, signal: Signal) -> Event {
    Event {
        provider: Provider::Claude,
        session: session.into(),
        agent: None,
        agent_type: None,
        cwd: format!("/work/{session}"),
        started_ms,
        window: None,
        signal,
    }
}

fn ask() -> Signal {
    Signal::Ask { key: 1, prompt: PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), always: false } }
}

fn ended() -> Signal {
    Signal::TurnEnded { message: "Done".into() }
}

#[test]
fn turn_runs_from_prompt_to_stop() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("app", 1_000, Signal::TurnStarted), now);
    let runs = board.runs();
    assert_eq!((runs.len(), runs[0].project.as_str(), runs[0].since_ms), (1, "app", 1_000));
    assert_eq!(runs[0].session, "app");
    board.apply(event("app", 2_000, Signal::ToolDone { key: 4 }), now);
    board.apply(event("app", 61_000, ended()), now);
    assert!(board.runs().is_empty());
    let finished = board.finished();
    assert_eq!((finished.len(), finished[0].took), (1, Duration::from_secs(60)));
    assert_eq!((finished[0].message.as_str(), finished[0].ended_ms), ("Done", 61_000));
}

#[test]
fn only_last_eight_turns_are_kept() {
    let mut board = Board::default();
    let now = Instant::now();
    for turn in 0..10u64 {
        let at = turn * MINUTE;
        board.apply(event("app", at + 1, Signal::TurnStarted), now);
        board.apply(event("app", at + 2, Signal::TurnEnded { message: format!("turn {turn}") }), now);
    }
    let finished = board.finished();
    assert_eq!(finished.len(), 8);
    assert_eq!((finished[0].message.as_str(), finished[7].message.as_str()), ("turn 2", "turn 9"));
}

#[test]
fn waiting_prompt_names_its_session() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("app", 1_000, Signal::TurnStarted), now);
    board.apply(event("app", 2_000, ask()), now);
    let prompts = board.prompts();
    assert_eq!((prompts.len(), prompts[0].session.as_str()), (1, "app"));
    assert_eq!(board.runs()[0].session, prompts[0].session);
}

#[test]
fn late_hooks_never_revive_a_turn() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("app", 1_000, Signal::TurnStarted), now);
    board.apply(event("app", 9_000, ended()), now);
    board.apply(event("app", 8_000, Signal::ToolDone { key: 4 }), now);
    assert!(board.runs().is_empty());
    board.apply(event("app", 10_000, Signal::TurnStarted), now);
    assert_eq!(board.runs()[0].since_ms, 10_000);
}

#[test]
fn mid_turn_work_starts_a_run() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("a", 1_000, Signal::ToolDone { key: 1 }), now);
    board.apply(event("b", 1_000, Signal::TurnStarted), now);
    assert_eq!(board.runs().len(), 2);
    board.apply(event("a", 2_000, Signal::Idle), now);
    board.apply(event("b", 2_000, Signal::SessionEnded), now);
    assert!(board.runs().is_empty());
    assert!(board.finished().is_empty(), "only a turn that stops on its own is done");
}

#[test]
fn question_waits_until_answered() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("app", 1_000, Signal::TurnStarted), now);
    let held = board.apply(event("app", 2_000, ask()), now).unwrap();
    assert!(board.runs()[0].waiting);
    board.reply(held.id, Reply::Allow, 0);
    assert!(!board.runs()[0].waiting);

    board.apply(event("app", 3_000, Signal::Attention { message: "Claude needs your permission".into() }), now);
    assert!(board.runs()[0].waiting);
    board.apply(event("app", 4_000, Signal::ToolDone { key: 1 }), now);
    assert!(!board.runs()[0].waiting);
}

#[test]
fn quiet_run_drops_after_15_minutes() {
    let mut board = Board::default();
    board.apply(event("app", 0, Signal::TurnStarted), Instant::now());
    let version = board.version();
    board.expire_runs(14 * MINUTE);
    assert_eq!((board.runs().len(), board.version()), (1, version));
    board.expire_runs(15 * MINUTE);
    assert!(board.runs().is_empty());
    assert!(board.version() > version);
}

#[test]
fn finished_turn_leaves_after_3_hours() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("app", 0, Signal::TurnStarted), now);
    board.apply(event("app", MINUTE, ended()), now);
    let version = board.version();
    board.expire_runs(3 * HOUR);
    assert_eq!((board.finished().len(), board.version()), (1, version));
    board.expire_runs(3 * HOUR + MINUTE + 1);
    assert!(board.finished().is_empty());
    assert!(board.version() > version);
}

#[test]
fn run_keeps_latest_window() {
    let mut board = Board::default();
    let now = Instant::now();
    let from = |window, started_ms, signal| Event { window, ..event("app", started_ms, signal) };
    board.apply(from(None, 1_000, Signal::TurnStarted), now);
    assert_eq!(board.runs()[0].window, None);
    board.apply(from(Some(42), 2_000, Signal::ToolStarted { tool: "Bash".into() }), now);
    assert_eq!(board.runs()[0].window, Some(42));
    let version = board.version();
    board.apply(from(Some(42), 2_500, Signal::ToolStarted { tool: "Bash".into() }), now);
    assert_eq!(board.version(), version, "hearing the same window again changes nothing");
    board.apply(from(None, 3_000, Signal::ToolDone { key: 1 }), now);
    assert_eq!(board.runs()[0].window, Some(42), "a hook without a window keeps the one we know");
    board.apply(from(Some(7), 4_000, Signal::TurnStarted), now);
    assert_eq!(board.runs()[0].window, Some(7));
    assert!(board.version() > version);
}

#[test]
fn turning_off_forgets_runs() {
    let mut board = Board::default();
    board.apply(event("app", 0, Signal::TurnStarted), Instant::now());
    board.set_enabled(Provider::Claude, false);
    assert!(board.runs().is_empty());
}

fn in_window(session: &str, window: Option<u32>, started_ms: u64, signal: Signal) -> Event {
    Event { window, ..event(session, started_ms, signal) }
}

#[test]
fn prompt_knows_its_run_window() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(in_window("app", Some(42), 1_000, Signal::TurnStarted), now);
    let held = board.apply(event("app", 2_000, ask()), now).unwrap();
    board.apply(event("other", 1_000, Signal::TurnStarted), now);
    let blind = board.apply(event("other", 2_000, ask()), now).unwrap();
    assert_eq!(board.window_of(held.id), Some(42));
    assert_eq!(board.window_of(blind.id), None, "a run without a known window");
    assert_eq!(board.window_of(999), None, "no such prompt");
}

#[test]
fn closed_window_ends_run_quietly() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(in_window("closed", Some(7), 1_000, Signal::TurnStarted), now);
    board.apply(event("closed", 2_000, ask()), now);
    board.apply(in_window("open", Some(8), 1_000, Signal::TurnStarted), now);
    board.apply(event("blind", 1_000, Signal::TurnStarted), now);
    let version = board.version();
    board.expire_gone(3_000, |pid| pid != 7);
    let left: Vec<String> = board.runs().into_iter().map(|run| run.session).collect();
    assert_eq!(left, ["open", "blind"]);
    assert!(board.finished().is_empty(), "a closed window is not a finished turn");
    assert!(board.prompts().is_empty(), "its question went with it");
    assert!(board.version() > version);
    board.apply(event("closed", 2_500, Signal::ToolDone { key: 1 }), now);
    assert_eq!(board.runs().len(), 2, "a late hook from before it closed never brings it back");
    let settled = board.version();
    board.expire_gone(4_000, |pid| pid != 7);
    assert_eq!(board.version(), settled, "nothing more to end");
}
