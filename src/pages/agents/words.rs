//! What each row says: what a run is doing, how long it took and when it ended.

use crate::assistant::{Finished, Provider, Run};
use crate::ui::agent_mark::clock;

const THINKING: &str = "Thinking";
const NEEDS_YOU: &str = "Needs you";
const NOTHING: &str = "Nothing running";
const MAX_CHARS: usize = 600;
const DOT: &str = " \u{b7} ";
const MINUTE_MS: u64 = 60_000;
const HOUR_MS: u64 = 60 * MINUTE_MS;
const DAY_MS: u64 = 24 * HOUR_MS;

pub(super) fn doing(run: &Run, prompt: Option<&str>, now_ms: u64) -> String {
    if run.waiting {
        return prompt.map(first_line).filter(|title| !title.is_empty()).unwrap_or(NEEDS_YOU).to_owned();
    }
    let verb = run.doing.unwrap_or(THINKING);
    format!("{verb}{DOT}{}", clock(now_ms.saturating_sub(run.since_ms)))
}

pub(super) fn finished(done: &Finished, now_ms: u64) -> String {
    let took = clock(done.took.as_millis() as u64);
    format!("{took}{DOT}{}", ago(now_ms.saturating_sub(done.ended_ms)))
}

pub(super) fn ago(elapsed_ms: u64) -> String {
    match elapsed_ms {
        ms if ms < MINUTE_MS => "just now".to_owned(),
        ms if ms < HOUR_MS => format!("{}m ago", ms / MINUTE_MS),
        ms if ms < DAY_MS => format!("{}h ago", ms / HOUR_MS),
        ms => format!("{}d ago", ms / DAY_MS),
    }
}

pub(super) fn ago_changes_in(elapsed_ms: u64) -> u64 {
    let step = match elapsed_ms {
        ms if ms < HOUR_MS => MINUTE_MS,
        ms if ms < DAY_MS => HOUR_MS,
        _ => DAY_MS,
    };
    step - elapsed_ms % step
}

pub(super) fn first_line(message: &str) -> &str {
    message.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default()
}

pub(super) fn whole(message: &str) -> String {
    let lines: Vec<&str> = message.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    lines.join("\n").chars().take(MAX_CHARS).collect()
}

pub(super) fn nothing() -> &'static str {
    NOTHING
}

pub(super) fn who_shows_up(assistant: Option<Provider>) -> &'static str {
    match assistant {
        Some(Provider::Claude) => "Claude Code shows up here while it works",
        Some(Provider::Codex) => "Codex shows up here while it works",
        None => "Claude Code and Codex show up here while they work",
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn run(waiting: bool, doing: Option<&'static str>) -> Run {
        Run {
            provider: Provider::Claude,
            session: "s".into(),
            project: "p".into(),
            since_ms: 1_000,
            waiting,
            doing,
            window: None,
        }
    }

    #[test]
    fn running_turn_says_what_and_how_long() {
        assert_eq!(doing(&run(false, Some("Editing")), None, 193_000), "Editing \u{b7} 3:12");
        assert_eq!(doing(&run(false, None), None, 43_000), "Thinking \u{b7} 0:42");
        assert_eq!(doing(&run(true, Some("Editing")), Some("Run this command?"), 9_000), "Run this command?");
        assert_eq!(doing(&run(true, None), None, 9_000), NEEDS_YOU);
        assert_eq!(doing(&run(true, None), Some("  "), 9_000), NEEDS_YOU);
    }

    #[test]
    fn finished_turn_says_took_and_ended() {
        let done = Finished {
            id: 1,
            provider: Provider::Claude,
            project: "rocket".into(),
            message: String::new(),
            took: Duration::from_secs(151),
            ended_ms: 1_000_000,
        };
        assert_eq!(finished(&done, 1_000_000 + 4 * MINUTE_MS + 10_000), "2:31 \u{b7} 4m ago");
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(3 * HOUR_MS + 5), "3h ago");
        assert_eq!(ago(2 * DAY_MS), "2d ago");
    }

    #[test]
    fn ago_changes_at_whole_unit() {
        for elapsed in [0, 1, 30_000, 59_999, 4 * MINUTE_MS + 10_000, HOUR_MS - 1, HOUR_MS + 5, 2 * DAY_MS + 7] {
            let wait = ago_changes_in(elapsed);
            assert!(wait > 0);
            assert_eq!(ago(elapsed + wait - 1), ago(elapsed), "no change before {elapsed} + {wait}");
            assert_ne!(ago(elapsed + wait), ago(elapsed), "a change at {elapsed} + {wait}");
        }
        assert_eq!(ago_changes_in(4 * MINUTE_MS + 10_000), 50_000);
        assert_eq!(ago_changes_in(HOUR_MS + 5), HOUR_MS - 5);
    }

    #[test]
    fn first_line_is_summary() {
        let message = "Shipped the fix, tests green\nThe parser now keeps blank lines out\nRun cargo test to check";
        assert_eq!(first_line(message), "Shipped the fix, tests green");
        assert_eq!(first_line("All done."), "All done.");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn whole_message_keeps_lines() {
        let message = "Summary\nFixed the parser\nAdded snake_case tests";
        assert_eq!(whole(message), message);
        assert_eq!(whole(&"x".repeat(MAX_CHARS + 10)).chars().count(), MAX_CHARS);
    }

    #[test]
    fn waiting_row_shows_prompt() {
        let prompt = "Claude needs your input\nIt is stuck on the migration";
        assert_eq!(doing(&run(true, None), Some(prompt), 9_000), "Claude needs your input");
    }

    #[test]
    fn empty_page_names_assistant() {
        assert!(who_shows_up(Some(Provider::Claude)).starts_with("Claude Code shows"));
        assert!(who_shows_up(Some(Provider::Codex)).starts_with("Codex shows"));
        assert!(who_shows_up(None).contains("and Codex show"));
    }
}
