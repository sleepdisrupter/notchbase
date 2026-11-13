//! Short, plain text for prompts: a tool call's name and first meaningful field, questions and their options, and
//! messages with markdown marks stripped, each clipped to fit.

use serde_json::Value;

use crate::assistant::prompt::{Offer, PromptKind, Question};

const DETAIL_CHARS: usize = 600;
const TEXT_CHARS: usize = 400;
const OPTION_CHARS: usize = 120;
const HEADER_CHARS: usize = 40;
const MAX_OPTIONS: usize = 8;
const MAX_QUESTIONS: usize = 4;
const DETAIL_FIELDS: [&str; 9] =
    ["command", "file_path", "notebook_path", "url", "query", "pattern", "path", "prompt", "description"];
const PATCH_MARKERS: [&str; 3] = ["*** Add File: ", "*** Update File: ", "*** Delete File: "];
const ELLIPSIS: char = '\u{2026}';
const MARKDOWN_MARKS: [char; 5] = ['#', '-', '*', '>', '`'];

pub(super) fn permission(tool: &str, input: &Value, always: bool) -> PromptKind {
    PromptKind::Permission { tool: tool_label(tool), detail: detail(tool, input), always }
}

/// AskUserQuestion's questions, answerable in the notch only when none were dropped and each has options.
pub(super) fn questions(input: &Value, may_answer: bool) -> Option<PromptKind> {
    let asked = input.get("questions")?.as_array()?;
    let questions: Vec<Question> = asked.iter().take(MAX_QUESTIONS).filter_map(question).collect();
    let complete = questions.len() == asked.len()
        && asked.iter().zip(&questions).all(|(raw, parsed)| {
            raw.get("question").and_then(Value::as_str).is_some() && offers_of(raw).len() == parsed.offers.len()
        });
    let answerable = may_answer && complete && questions.iter().all(|question| !question.offers.is_empty());
    (!questions.is_empty()).then_some(PromptKind::Questions { questions, answerable })
}

fn question(raw: &Value) -> Option<Question> {
    let header = raw.get("header").and_then(Value::as_str).unwrap_or_default();
    let text = raw.get("question").and_then(Value::as_str).filter(|text| !text.trim().is_empty()).unwrap_or(header);
    if text.trim().is_empty() {
        return None;
    }
    let offers = offers_of(raw).iter().take(MAX_OPTIONS).filter_map(offer).collect();
    let multi = raw.get("multiSelect").and_then(Value::as_bool).unwrap_or(false);
    Some(Question { header: summary(header, HEADER_CHARS), text: summary(text, TEXT_CHARS), offers, multi })
}

fn offers_of(raw: &Value) -> &[Value] {
    raw.get("options").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
}

fn offer(raw: &Value) -> Option<Offer> {
    let label = raw.get("label").and_then(Value::as_str).or_else(|| raw.as_str())?;
    let detail = raw.get("description").and_then(Value::as_str).unwrap_or_default();
    Some(Offer { label: clip(label, OPTION_CHARS), detail: summary(detail, OPTION_CHARS) })
}

/// Keeps a message's line breaks but tidies each line, so the Agents page can show the first as a summary.
pub(super) fn message(text: Option<&str>) -> String {
    let lines: Vec<String> =
        text.unwrap_or_default().lines().map(message_line).filter(|line| !line.is_empty()).collect();
    clip(&lines.join("\n"), TEXT_CHARS)
}

fn message_line(line: &str) -> String {
    let words: Vec<String> = line
        .split_whitespace()
        .filter(|word| !word.chars().all(|mark| MARKDOWN_MARKS.contains(&mark)))
        .map(|word| word.replace(['*', '`'], ""))
        .filter(|word| !word.is_empty())
        .collect();
    words.join(" ")
}

fn summary(text: &str, max_chars: usize) -> String {
    clip(&text.split_whitespace().collect::<Vec<_>>().join(" "), max_chars)
}

/// MCP tools read as server/tool.
fn tool_label(tool: &str) -> String {
    match tool.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")) {
        Some((server, name)) => format!("{server}/{name}"),
        None => tool.to_owned(),
    }
}

fn detail(tool: &str, input: &Value) -> String {
    let text = patch_files(tool, input)
        .or_else(|| DETAIL_FIELDS.iter().find_map(|field| words(input.get(field)?)))
        .unwrap_or_else(|| compact(input));
    clip(&text, DETAIL_CHARS)
}

fn words(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    }
}

fn patch_files(tool: &str, input: &Value) -> Option<String> {
    if tool != "apply_patch" {
        return None;
    }
    let patch = input.get("command").and_then(Value::as_str).or_else(|| input.get("patch")?.as_str())?;
    let files: Vec<&str> = patch
        .lines()
        .filter_map(|line| PATCH_MARKERS.iter().find_map(|marker| line.strip_prefix(marker)))
        .map(str::trim)
        .collect();
    (!files.is_empty()).then(|| files.join(", "))
}

fn compact(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::Object(fields) if fields.is_empty() => String::new(),
        other => other.to_string(),
    }
}

fn clip(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().nth(max_chars).is_none() {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.truncate(clipped.trim_end().len());
    clipped.push(ELLIPSIS);
    clipped
}
