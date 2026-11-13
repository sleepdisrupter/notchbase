use std::time::{Duration, Instant};

use crate::assistant::board::Board;
use crate::assistant::prompt::{Offer, PromptKind, Provider, Question, Reply};
use crate::assistant::timing::{ATTENTION_TTL, FINISHED_TTL};
use crate::assistant::wire::{Answer, Event, Signal};

fn event(session: &str, agent: Option<&str>, started_ms: u64, signal: Signal) -> Event {
    Event {
        provider: Provider::Claude,
        session: session.into(),
        agent: agent.map(Into::into),
        agent_type: agent.map(|_| "Explore".into()),
        cwd: "C:\\work\\notchbase\\".into(),
        started_ms,
        window: None,
        signal,
    }
}

fn ask(key: u64) -> Signal {
    Signal::Ask { key, prompt: PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), always: true } }
}

fn question(answerable: bool) -> Signal {
    let offer = |label: &str| Offer { label: label.into(), detail: String::new() };
    let question =
        Question { header: String::new(), text: "Ship?".into(), offers: vec![offer("Yes"), offer("No")], multi: false };
    Signal::Ask { key: 9, prompt: PromptKind::Questions { questions: vec![question], answerable } }
}

fn picked(picks: &[&[usize]]) -> Reply {
    Reply::Answers(picks.iter().map(|picked| picked.to_vec()).collect())
}

#[test]
fn replies_map_to_answers() {
    let mut board = Board::default();
    let now = Instant::now();
    let held = board.apply(event("s", None, 1, ask(1)), now).unwrap();
    let prompts = board.prompts();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].id, held.id);
    assert_eq!(prompts[0].project, "notchbase");
    assert_eq!(prompts[0].provider, Provider::Claude);
    board.reply(held.id, Reply::Deny, 0);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Deny));
    assert!(board.prompts().is_empty());

    let held = board.apply(event("s", None, 2, question(true)), now).unwrap();
    board.reply(held.id, picked(&[&[1]]), 0);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Answers { picks: vec![vec![1]] }));

    let held = board.apply(event("s", None, 2, ask(2)), now).unwrap();
    board.reply(held.id, Reply::AllowAlways, 0);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Always));

    let unanswerable = [
        (question(false), picked(&[&[0]])),
        (question(true), Reply::Allow),
        (ask(3), Reply::InApp),
        (question(true), picked(&[&[7]])),
        (question(true), picked(&[&[0, 1]])),
        (question(true), picked(&[])),
    ];
    for (signal, reply) in unanswerable {
        let held = board.apply(event("s", None, 3, signal), now).unwrap();
        board.reply(held.id, reply, 0);
        assert_eq!(held.answers.try_recv(), Ok(Answer::Release));
    }
}

#[test]
fn later_turn_supersedes_old_prompts() {
    let mut board = Board::default();
    let now = Instant::now();
    let main = board.apply(event("s", None, 10, ask(1)), now).unwrap();
    let helper = board.apply(event("s", Some("a1"), 10, ask(2)), now).unwrap();
    let other = board.apply(event("t", None, 10, ask(3)), now).unwrap();
    let later = board.apply(event("s", None, 50, ask(4)), now).unwrap();

    assert!(board.apply(event("s", None, 20, Signal::TurnStarted), now).is_none());
    assert_eq!(main.answers.try_recv(), Ok(Answer::Release));
    assert!(helper.answers.try_recv().is_err());
    assert!(other.answers.try_recv().is_err());
    assert!(later.answers.try_recv().is_err());
    assert_eq!(board.prompts().len(), 3);

    board.apply(event("s", None, 60, Signal::TurnEnded { message: "Done".into() }), now);
    assert_eq!(later.answers.try_recv(), Ok(Answer::Release));
    let kinds: Vec<PromptKind> = board.prompts().into_iter().map(|prompt| prompt.kind).collect();
    assert_eq!(kinds.last(), Some(&PromptKind::Finished { message: "Done".into() }));
    assert_eq!(kinds.len(), 3);

    board.apply(event("s", None, 70, Signal::SessionEnded), now);
    assert_eq!(helper.answers.try_recv(), Ok(Answer::Release));
    assert_eq!(board.prompts().len(), 1);
}

#[test]
fn tool_done_settles_oldest_match() {
    let mut board = Board::default();
    let now = Instant::now();
    let first = board.apply(event("s", None, 1, ask(7)), now).unwrap();
    let second = board.apply(event("s", None, 2, ask(7)), now).unwrap();
    let different = board.apply(event("s", None, 3, ask(8)), now).unwrap();
    board.apply(event("s", Some("a1"), 4, Signal::ToolDone { key: 7 }), now);
    assert!(first.answers.try_recv().is_err());
    board.apply(event("s", None, 4, Signal::ToolDone { key: 7 }), now);
    assert_eq!(first.answers.try_recv(), Ok(Answer::Release));
    assert!(second.answers.try_recv().is_err());
    assert!(different.answers.try_recv().is_err());
    assert_eq!(board.prompts().len(), 2);
}

#[test]
fn attention_waits_behind_held_prompts() {
    let mut board = Board::default();
    let now = Instant::now();
    let held = board.apply(event("s", None, 1, ask(1)), now).unwrap();
    board.apply(event("s", None, 2, Signal::Attention { message: "Needs permission".into() }), now);
    assert_eq!(board.prompts().len(), 1);
    board.reply(held.id, Reply::InApp, 0);

    board.apply(event("s", None, 3, Signal::TurnEnded { message: String::new() }), now);
    board.apply(event("s", None, 4, Signal::Attention { message: "Needs input".into() }), now);
    let prompts = board.prompts();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].kind, PromptKind::Attention { message: "Needs input".into() });
}

#[test]
fn notes_expire_and_can_be_dismissed() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("s", None, 1, Signal::TurnEnded { message: "Done".into() }), now);
    board.apply(event("t", None, 1, Signal::Attention { message: "Look".into() }), now);
    board.sweep(now + FINISHED_TTL - Duration::from_secs(1));
    assert_eq!(board.prompts().len(), 2);
    board.sweep(now + FINISHED_TTL);
    assert_eq!(board.prompts().len(), 1);
    let id = board.prompts()[0].id;
    board.reply(id, Reply::Allow, 0);
    assert!(board.prompts().is_empty());
    board.apply(event("t", None, 2, Signal::Attention { message: "Again".into() }), now);
    board.sweep(now + ATTENTION_TTL);
    assert!(board.prompts().is_empty());
}

#[test]
fn muted_providers_release_and_ignore() {
    let mut board = Board::default();
    let now = Instant::now();
    let held = board.apply(event("s", None, 1, ask(1)), now).unwrap();
    let version = board.version();
    board.set_enabled(Provider::Codex, false);
    assert_eq!(board.version(), version);
    board.set_enabled(Provider::Claude, false);
    assert_eq!(held.answers.try_recv(), Ok(Answer::Release));
    assert!(board.prompts().is_empty());
    assert!(board.apply(event("s", None, 2, ask(2)), now).is_none());
    assert!(board.prompts().is_empty());
    board.set_enabled(Provider::Claude, true);
    assert!(board.apply(event("s", None, 3, ask(3)), now).is_some());
}

#[test]
fn versions_only_move_on_real_changes() {
    let mut board = Board::default();
    let now = Instant::now();
    board.apply(event("s", None, 1, Signal::TurnStarted), now);
    let start = board.version();
    board.apply(event("s", None, 1, Signal::ToolDone { key: 1 }), now);
    board.apply(event("s", None, 1, Signal::TurnStarted), now);
    board.forget(42);
    board.reply(42, Reply::Allow, 0);
    board.sweep(now);
    assert_eq!(board.version(), start);
    let held = board.apply(event("s", None, 1, ask(1)), now).unwrap();
    assert!(board.version() > start);
    let after_ask = board.version();
    board.forget(held.id);
    assert!(board.version() > after_ask);
    assert!(board.prompts().is_empty());
}
