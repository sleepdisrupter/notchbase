//! Coding assistants at work, and the questions they ask.

use crate::assistant::{Finished, Provider, Question, Run};

#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub id: u64,
    pub provider: Provider,
    pub context: String,
    pub title: String,
    pub detail: Option<String>,
    pub choices: Choices,
    pub waiting: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Choices {
    Permission { always: bool },
    Questions(Vec<Question>),
    Elsewhere,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentsView {
    pub running: Vec<RunRow>,
    pub recent: Vec<Finished>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunRow {
    pub run: Run,
    pub prompt: Option<(u64, String)>,
}
