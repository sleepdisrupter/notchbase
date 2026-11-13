//! What the notch can ask: prompts and their kinds, the questions in them, and the replies you can give.

use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Claude, Provider::Codex];

    pub fn title(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::Codex => "ChatGPT",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub id: u64,
    pub provider: Provider,
    pub session: String,
    pub project: String,
    pub agent: Option<String>,
    pub kind: PromptKind,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromptKind {
    Permission { tool: String, detail: String, always: bool },
    Questions { questions: Vec<Question>, answerable: bool },
    Attention { message: String },
    Finished { message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub header: String,
    pub text: String,
    pub offers: Vec<Offer>,
    pub multi: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Allow,
    AllowAlways,
    Deny,
    Answers(Vec<Vec<usize>>),
    InApp,
}

/// Whether the picks answer every question: one option for single choice, at least one for multi, none twice.
pub fn fits(questions: &[Question], picks: &[Vec<usize>]) -> bool {
    questions.len() == picks.len()
        && questions.iter().zip(picks).all(|(question, picked)| {
            let counted = if question.multi { !picked.is_empty() } else { picked.len() == 1 };
            let known = picked.iter().all(|index| *index < question.offers.len());
            let unique = picked.iter().enumerate().all(|(at, index)| !picked[..at].contains(index));
            counted && known && unique
        })
}
