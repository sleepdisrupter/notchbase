//! Claude Code and Codex: their hooks, the questions they ask in the notch and the answers sent back.

pub(super) mod agents;
mod view;

use crate::assistant::{self, Hooks, Prompt, PromptKind, Provider, Question, Reply};
use crate::bridge::{Action, Ask, Choices};

use super::App;
use super::config::{assistant_of, hooked, provider_key};
use super::preview::previewing;

const NOT_SAVED: &str = "Couldn't save the setting";
const NOT_CONNECTED: &str = "Couldn't connect questions";
const NOT_LISTENING: &str = "questions in the notch are unavailable: couldn't open a local port";
const TRUST_CODEX: &str = "Open Codex and run /hooks to trust notchbase";
const MCP_PREFIX: &str = "mcp__";

impl App {
    pub(super) fn start_assistant(&mut self) {
        if !self.inbox.running() {
            eprintln!("notchbase: {NOT_LISTENING}");
        }
        self.connect_assistant(None, false);
    }

    pub(super) fn set_assistant(&mut self, assistant: Option<Provider>) {
        if assistant_of(self.config.config()) == assistant {
            return;
        }
        let was = self.hooked_provider();
        let saved = self.config.update(|config| {
            if let Some(provider) = assistant {
                config.assistant.provider = provider_key(provider).to_owned();
            }
            config.assistant.enabled = assistant.is_some();
        });
        if let Err(err) = saved {
            self.report(NOT_SAVED, err);
        }
        self.tabs = super::tools::tabs(self.config.config(), self.tools.hosts());
        self.connect_assistant(was, true);
        self.redraw_notch();
    }

    pub(super) fn set_questions_in_notch(&mut self, on: bool) {
        self.set_hooked(|config| config.assistant.questions_in_notch = on);
    }

    pub(super) fn set_progress_in_notch(&mut self, on: bool) {
        self.set_hooked(|config| config.assistant.progress_in_notch = on);
    }

    fn set_hooked(&mut self, change: impl FnOnce(&mut crate::config::Config)) {
        let was = self.hooked_provider();
        if let Err(err) = self.config.update(change) {
            self.report(NOT_SAVED, err);
        }
        self.connect_assistant(was, true);
        self.redraw_notch();
    }

    pub(super) fn hooked_provider(&self) -> Option<Provider> {
        hooked(self.config.config()).map(|(provider, _)| provider)
    }

    pub(super) fn assistant(&self) -> Option<Provider> {
        assistant_of(self.config.config())
    }

    pub(super) fn answer(&mut self, id: u64, reply: Reply) -> Option<Action> {
        if self.pinned_prompt == Some(id) {
            self.pinned_prompt = None;
        }
        let next = follow_up(&reply, self.inbox.window_of(id));
        self.inbox.reply(id, reply);
        self.sync_prompts();
        next
    }

    pub(super) fn show_prompt(&mut self, id: u64) {
        self.pinned_prompt = Some(id);
        self.show_prompts();
    }

    pub(super) fn sync_prompts(&mut self) {
        self.refresh_prompts(false);
    }

    fn refresh_prompts(&mut self, force: bool) {
        if moved_on(self.inbox.version(), &mut self.prompts_seen, force) {
            self.show_prompts();
        }
    }

    fn show_prompts(&mut self) {
        let prompts = self.inbox.prompts();
        self.pinned_prompt = self.pinned_prompt.filter(|id| prompts.iter().any(|prompt| prompt.id == *id));
        let questions = self.config.config().assistant.questions_in_notch;
        let ask = shown_ask(&prompts, self.pinned_prompt, questions);
        self.update_notch(|notch| notch.show_ask(ask.clone()));
    }

    pub(super) fn connect_assistant(&mut self, disconnect: Option<Provider>, announce: bool) {
        let hooked = hooked(self.config.config());
        let chosen = hooked.map(|(provider, _)| provider);
        for provider in Provider::ALL {
            self.inbox.set_enabled(provider, chosen == Some(provider));
        }
        if manages_hooks() {
            if let Some(provider) = disconnect.filter(|&provider| chosen != Some(provider))
                && let Err(err) = assistant::uninstall(provider)
            {
                eprintln!("notchbase: couldn't remove {} hooks: {err}", provider.title());
            }
            if let Some((provider, hooks)) = hooked {
                self.install_hooks(provider, hooks, announce);
            }
        }
        self.refresh_prompts(true);
    }

    fn install_hooks(&mut self, provider: Provider, hooks: Hooks, announce: bool) {
        let fresh = !assistant::installed(provider, hooks);
        match assistant::install(provider, hooks) {
            Ok(()) if fresh && announce && provider == Provider::Codex => self.notify(TRUST_CODEX),
            Ok(()) => {}
            Err(err) if announce => self.report(NOT_CONNECTED, err),
            Err(err) => eprintln!("notchbase: {NOT_CONNECTED}: {err}"),
        }
    }
}

fn manages_hooks() -> bool {
    !previewing()
}

fn follow_up(reply: &Reply, window: Option<u32>) -> Option<Action> {
    window.filter(|_| *reply == Reply::InApp).map(Action::FocusWindow)
}

fn moved_on(version: u64, seen: &mut u64, force: bool) -> bool {
    let moved = force || version != *seen;
    *seen = version;
    moved
}

fn shown_ask(prompts: &[Prompt], pinned: Option<u64>, questions: bool) -> Option<Ask> {
    pinned_ask(prompts, pinned).or_else(|| questions.then(|| next_ask(prompts)).flatten())
}

fn next_ask(prompts: &[Prompt]) -> Option<Ask> {
    let shown = askable(prompts);
    let first = shown.first()?;
    Some(ask_for(first, shown.len() - 1))
}

fn pinned_ask(prompts: &[Prompt], pinned: Option<u64>) -> Option<Ask> {
    let shown = askable(prompts);
    let pinned = shown.iter().find(|prompt| Some(prompt.id) == pinned)?;
    Some(ask_for(pinned, shown.len() - 1))
}

fn askable(prompts: &[Prompt]) -> Vec<&Prompt> {
    prompts.iter().filter(|prompt| !matches!(prompt.kind, PromptKind::Finished { .. })).collect()
}

pub(super) fn title_of(prompt: &Prompt) -> String {
    ask_for(prompt, 0).title
}

fn ask_for(prompt: &Prompt, waiting: usize) -> Ask {
    let context = match &prompt.agent {
        Some(agent) => format!("{} \u{b7} {agent}", prompt.project),
        None => prompt.project.clone(),
    };
    let (title, detail, choices) = match &prompt.kind {
        PromptKind::Permission { tool, detail, always } => {
            let detail = Some(detail.clone()).filter(|d| !d.is_empty());
            (permission_title(tool), detail, Choices::Permission { always: *always })
        }
        PromptKind::Questions { questions, answerable: true } => {
            (first_text(questions), None, Choices::Questions(questions.clone()))
        }
        PromptKind::Questions { questions, answerable: false } => {
            let labels: Vec<&str> = questions
                .iter()
                .take(1)
                .flat_map(|question| &question.offers)
                .map(|offer| offer.label.as_str())
                .collect();
            (first_text(questions), (!labels.is_empty()).then(|| labels.join(" \u{b7} ")), Choices::Elsewhere)
        }
        PromptKind::Attention { message } | PromptKind::Finished { message } => {
            (message.clone(), None, Choices::Elsewhere)
        }
    };
    Ask { id: prompt.id, provider: prompt.provider, context, title, detail, choices, waiting }
}

fn first_text(questions: &[Question]) -> String {
    questions.first().map(|question| question.text.clone()).unwrap_or_default()
}

fn permission_title(tool: &str) -> String {
    let title = match tool {
        "Bash" | "shell" | "exec_command" | "local_shell" => "Run this command?",
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "apply_patch" => "Change this file?",
        "Read" => "Read this file?",
        "WebFetch" => "Open this page?",
        "WebSearch" => "Search the web?",
        tool => return format!("Allow {}?", readable_tool(tool)),
    };
    title.to_owned()
}

fn readable_tool(tool: &str) -> String {
    tool.strip_prefix(MCP_PREFIX).map_or_else(|| tool.to_owned(), |rest| rest.replace("__", " ").replace('_', " "))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::assistant::Offer;

    fn prompt(id: u64, kind: PromptKind) -> Prompt {
        Prompt {
            id,
            provider: Provider::Claude,
            session: "s".into(),
            project: "notchbase".into(),
            agent: None,
            kind,
            at: Instant::now(),
        }
    }

    fn question(options: &[&str], answerable: bool) -> PromptKind {
        let offers = options.iter().map(|label| Offer { label: (*label).into(), detail: String::new() }).collect();
        let question = Question { header: String::new(), text: "Ship it?".into(), offers, multi: false };
        PromptKind::Questions { questions: vec![question], answerable }
    }

    #[test]
    fn permissions_get_allow_or_deny() {
        let bash = PromptKind::Permission { tool: "Bash".into(), detail: "cargo test".into(), always: true };
        let ask = next_ask(&[prompt(1, bash)]).unwrap();
        assert_eq!((ask.title.as_str(), ask.detail.as_deref()), ("Run this command?", Some("cargo test")));
        assert_eq!(ask.choices, Choices::Permission { always: true });
        let mcp =
            PromptKind::Permission { tool: "mcp__github__create_issue".into(), detail: String::new(), always: false };
        let ask = next_ask(&[prompt(2, mcp)]).unwrap();
        assert_eq!((ask.title.as_str(), ask.detail), ("Allow github create issue?", None));
    }

    #[test]
    fn questions_answered_in_notch() {
        let four = next_ask(&[prompt(1, question(&["A", "B", "C", "D"], true))]).unwrap();
        let Choices::Questions(questions) = &four.choices else { panic!("questions") };
        assert_eq!((four.title.as_str(), questions[0].offers.len()), ("Ship it?", 4));
        let codex = next_ask(&[prompt(1, question(&["A", "B", "C"], false))]).unwrap();
        assert_eq!((codex.choices, codex.detail.as_deref()), (Choices::Elsewhere, Some("A \u{b7} B \u{b7} C")));
    }

    #[test]
    fn finished_turns_dont_pop() {
        let done = prompt(1, PromptKind::Finished { message: "Done".into() });
        assert_eq!(next_ask(std::slice::from_ref(&done)), None);
        let waiting = prompt(2, PromptKind::Attention { message: "Claude needs input".into() });
        let later = prompt(3, question(&["Yes", "No"], true));
        let ask = next_ask(&[done, waiting, later]).unwrap();
        assert_eq!((ask.id, ask.waiting), (2, 1));
    }

    #[test]
    fn pinned_prompt_jumps_queue() {
        let first = prompt(1, PromptKind::Attention { message: "Claude needs input".into() });
        let second = prompt(2, question(&["Yes", "No"], true));
        let finished = prompt(3, PromptKind::Finished { message: "Done".into() });
        let prompts = [first, second, finished];
        let ask = pinned_ask(&prompts, Some(2)).unwrap();
        assert_eq!((ask.id, ask.title.as_str(), ask.waiting), (2, "Ship it?", 1));
        assert_eq!(pinned_ask(&prompts, Some(3)), None, "a finished turn is never asked");
        assert_eq!(pinned_ask(&prompts, Some(9)), None);
        assert_eq!(pinned_ask(&prompts, None), None);
        assert_eq!(title_of(&prompts[0]), "Claude needs input");
    }

    #[test]
    fn setting_change_resyncs() {
        let mut seen = 4;
        assert!(!moved_on(4, &mut seen, false));
        assert!(moved_on(4, &mut seen, true), "turning questions off must take the open ask down");
        assert!(moved_on(5, &mut seen, false));
        assert_eq!(seen, 5);
        assert!(!moved_on(5, &mut seen, false));
    }

    #[test]
    fn answer_brings_window_forward() {
        assert_eq!(follow_up(&Reply::InApp, Some(42)), Some(Action::FocusWindow(42)));
        assert_eq!(follow_up(&Reply::InApp, None), None, "no known window, nothing more to do");
        assert_eq!(follow_up(&Reply::Allow, Some(42)), None, "answered in the notch, the window stays put");
        assert_eq!(follow_up(&Reply::Answers(vec![vec![0]]), Some(42)), None);
    }

    #[test]
    fn questions_off_shows_pinned_only() {
        let prompts = [prompt(1, question(&["Yes", "No"], true))];
        assert_eq!(shown_ask(&prompts, None, true).map(|ask| ask.id), Some(1));
        assert_eq!(shown_ask(&prompts, None, false), None);
        assert_eq!(shown_ask(&prompts, Some(1), false).map(|ask| ask.id), Some(1));
    }
}
