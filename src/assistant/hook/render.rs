//! Prints the PermissionRequest decision the assistant expects. A release prints nothing, so it asks on its own.

use serde_json::{Map, Value, json};

use crate::assistant::wire::Answer;

const DENIED: &str = "The user denied this request from notchbase.";
const JOINED: &str = ", ";

pub(super) enum Render {
    Silent,
    Claude { input: Value, suggestions: Option<Value> },
    Codex,
}

impl Render {
    pub(super) fn output(&self, answer: &Answer) -> Option<String> {
        let decision = match (self, answer) {
            (Self::Silent, _) | (_, Answer::Release) | (Self::Codex, Answer::Answers { .. }) => return None,
            (Self::Claude { suggestions: Some(suggestions), .. }, Answer::Always) => {
                json!({ "behavior": "allow", "updatedPermissions": suggestions })
            }
            (Self::Claude { .. } | Self::Codex, Answer::Allow | Answer::Always) => json!({ "behavior": "allow" }),
            (Self::Claude { .. }, Answer::Deny) => json!({ "behavior": "deny", "message": DENIED, "interrupt": true }),
            (Self::Codex, Answer::Deny) => json!({ "behavior": "deny", "message": DENIED }),
            (Self::Claude { input, .. }, Answer::Answers { picks }) => {
                json!({ "behavior": "allow", "updatedInput": answered(input, picks)? })
            }
        };
        let output = json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": decision,
            }
        });
        Some(output.to_string())
    }
}

/// Claude's AskUserQuestion input with an answers map added: each question's text to the chosen labels.
fn answered(input: &Value, picks: &[Vec<usize>]) -> Option<Value> {
    let questions = input.get("questions")?.as_array()?;
    if questions.len() != picks.len() {
        return None;
    }
    let mut answers = Map::new();
    for (question, picked) in questions.iter().zip(picks) {
        let text = question.get("question")?.as_str()?;
        let options = question.get("options")?.as_array()?;
        let labels: Option<Vec<&str>> = picked.iter().map(|index| label(options.get(*index)?)).collect();
        let labels = labels.filter(|labels| !labels.is_empty())?;
        answers.insert(text.to_owned(), Value::from(labels.join(JOINED)));
    }
    let mut updated = input.as_object()?.clone();
    updated.insert("answers".to_owned(), Value::Object(answers));
    Some(Value::Object(updated))
}

fn label(option: &Value) -> Option<&str> {
    option.get("label").and_then(Value::as_str).or_else(|| option.as_str())
}
