//! The user's own words. Terms come from four places: ones they add, ones they correct after
//! dictating, ones they keep saying, and ones on screen while they talk. They steer Whisper
//! (through its prompt), the tidy-up model, and a fixer that runs on every transcript.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::words::{Word, common, likeness, possessive, split, squash, term_like};

/// Whisper reads about 224 tokens of prompt. Terms past this many characters are left out.
const PROMPT_CHARS: usize = 640;
const POLISH_TERMS: usize = 150;
const KEPT_TERMS: usize = 400;
const KEPT_SEEN: usize = 800;
/// A word said this often becomes a term. Said while on screen, it counts double.
const SEEN_TO_LEARN: u32 = 4;
/// A correction made this often is applied on its own, without waiting for the models.
const RULE_AFTER: u32 = 2;
/// How alike a heard word must be to a term to be spelled like it.
const SNAP_LIKENESS: f32 = 0.8;
const SNAP_SHORTEST: usize = 5;
const SCREEN_TERMS: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Added by the user.
    Added,
    /// Learned from a correction the user made.
    Corrected,
    /// Learned from being said often.
    Heard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    pub text: String,
    pub source: Source,
    #[serde(default)]
    pub uses: u32,
    #[serde(default)]
    pub last: u64,
}

/// "what was heard" is written "what was meant".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub from: String,
    pub to: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Seen {
    count: u32,
    last: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Glossary {
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    seen: HashMap<String, Seen>,
    #[serde(skip)]
    file: Option<PathBuf>,
}

impl Glossary {
    pub fn load(file: PathBuf) -> Self {
        let mut glossary: Glossary =
            std::fs::read(&file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        glossary.file = Some(file);
        glossary
    }

    pub fn save(&self) {
        let Some(file) = &self.file else { return };
        let Ok(bytes) = serde_json::to_vec_pretty(self) else { return };
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
        let partial = file.with_extension("json.part");
        if std::fs::write(&partial, bytes).is_ok() {
            let _ = std::fs::rename(&partial, file);
        }
    }

    /// Adds a term the user typed, or a rule written "from => to".
    pub fn add(&mut self, entry: &str, now: u64) -> bool {
        if let Some((from, to)) = entry.split_once("=>").or_else(|| entry.split_once('→')) {
            let (from, to) = (from.trim(), to.trim());
            if from.is_empty() || to.is_empty() || squash(from).is_empty() {
                return false;
            }
            self.rules.retain(|rule| !rule.from.eq_ignore_ascii_case(from));
            self.rules.push(Rule { from: from.to_lowercase(), to: to.to_owned(), count: RULE_AFTER });
            self.learn(to, Source::Added, now);
            return true;
        }
        let text = entry.trim().trim_matches(|c: char| c == ',' || c == ';');
        if squash(text).is_empty() {
            return false;
        }
        self.learn(text, Source::Added, now);
        true
    }

    pub fn remove(&mut self, text: &str) {
        self.terms.retain(|term| term.text != text);
        self.rules.retain(|rule| rule.to != text && rule.from != text.to_lowercase());
        self.seen.remove(&squash(text));
    }

    fn learn(&mut self, text: &str, source: Source, now: u64) -> bool {
        if let Some(term) = self.terms.iter_mut().find(|term| term.text == text) {
            term.uses += 1;
            term.last = now;
            if source == Source::Added || (source == Source::Corrected && term.source == Source::Heard) {
                term.source = source;
            }
            return false;
        }
        // The same word, spelled differently, was learned before: the newer spelling wins.
        self.terms.retain(|term| term.source == Source::Added || squash(&term.text) != squash(text));
        self.terms.push(Term { text: text.to_owned(), source, uses: 1, last: now });
        self.trim();
        true
    }

    fn trim(&mut self) {
        if self.terms.len() <= KEPT_TERMS {
            return;
        }
        self.terms.sort_by_key(|term| (term.source != Source::Added, std::cmp::Reverse(term.last)));
        self.terms.truncate(KEPT_TERMS);
    }

    /// The terms in the order they matter: the user's own, then corrections, then the rest by use.
    fn ranked(&self) -> Vec<&Term> {
        let mut terms: Vec<&Term> = self.terms.iter().collect();
        let rank = |source: Source| match source {
            Source::Added => 0,
            Source::Corrected => 1,
            Source::Heard => 2,
        };
        terms.sort_by_key(|term| (rank(term.source), std::cmp::Reverse((term.uses, term.last))));
        terms
    }

    /// Whisper's prompt: the terms on screen first, since they're what this is likely about.
    pub fn prompt(&self, screen: &[String]) -> String {
        let mut picked: Vec<&str> = Vec::new();
        let mut length = "Glossary: .".len();
        let ranked = self.ranked();
        let all = ranked.iter().take(8).map(|term| term.text.as_str()).chain(screen.iter().map(String::as_str));
        for term in all.chain(ranked.iter().map(|term| term.text.as_str())) {
            if picked.iter().any(|seen| squash(seen) == squash(term)) {
                continue;
            }
            if length + term.len() + 2 > PROMPT_CHARS {
                break;
            }
            length += term.len() + 2;
            picked.push(term);
        }
        if picked.is_empty() { String::new() } else { format!("Glossary: {}.", picked.join(", ")) }
    }

    /// The terms the tidy-up model is told about.
    pub fn for_polish(&self, screen: &[String]) -> Vec<String> {
        let mut terms: Vec<String> = self.ranked().into_iter().map(|term| term.text.clone()).collect();
        for term in screen {
            if !terms.iter().any(|known| squash(known) == squash(term)) {
                terms.push(term.clone());
            }
        }
        terms.truncate(POLISH_TERMS);
        terms
    }

    /// Spells what was heard the way the user writes it: their corrections first, then terms
    /// that were heard as several words or with the wrong capitals, or a letter or so off.
    pub fn fix(&self, text: &str, screen: &[String]) -> String {
        let ruled = self.apply_rules(text);
        let mut targets: Vec<&str> = self.terms.iter().map(|term| term.text.as_str()).collect();
        targets.extend(screen.iter().map(String::as_str));
        snap(&ruled, &targets)
    }

    fn apply_rules(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for rule in self.rules.iter().filter(|rule| rule.count >= RULE_AFTER) {
            out = replace_phrase(&out, &rule.from, &rule.to);
        }
        out
    }

    /// Learns from what the user changed in text that was just typed for them.
    pub fn corrected(&mut self, typed: &str, edited: &str, now: u64) -> Vec<String> {
        let mut learned = Vec::new();
        for (from, to) in crate::diff::corrections(typed, edited) {
            if !worth_learning(&from, &to) {
                continue;
            }
            match self.rules.iter_mut().find(|rule| rule.from == from.to_lowercase()) {
                Some(rule) if rule.to == to => rule.count += 1,
                Some(rule) => {
                    rule.to.clone_from(&to);
                    rule.count = 1;
                }
                None => self.rules.push(Rule { from: from.to_lowercase(), to: to.clone(), count: 1 }),
            }
            if self.learn(&to, Source::Corrected, now) {
                learned.push(to);
            }
        }
        learned
    }

    /// Counts the terms in a finished transcript. Ones said often, or said while on screen,
    /// are learned.
    pub fn heard(&mut self, text: &str, screen: &[String], now: u64) -> Vec<String> {
        let mut learned = Vec::new();
        for word in split(text) {
            let (stem, _) = possessive(&word.text);
            if let Some(term) = self.terms.iter_mut().find(|term| term.text == stem) {
                term.uses += 1;
                term.last = now;
                continue;
            }
            let on_screen = screen.iter().any(|term| term == stem);
            if !on_screen && !term_like(&word) && !self.seen.contains_key(stem) {
                continue;
            }
            let key = stem.to_owned();
            let seen = self.seen.entry(key.clone()).or_default();
            seen.count += if on_screen { 2 } else { 1 };
            seen.last = now;
            if seen.count >= SEEN_TO_LEARN {
                self.seen.remove(&key);
                if self.learn(stem, Source::Heard, now) {
                    learned.push(stem.to_owned());
                }
            }
        }
        if self.seen.len() > KEPT_SEEN {
            let mut oldest: Vec<(String, u64)> = self.seen.iter().map(|(k, v)| (k.clone(), v.last)).collect();
            oldest.sort_by_key(|(_, last)| *last);
            for (key, _) in oldest.into_iter().take(self.seen.len() - KEPT_SEEN) {
                self.seen.remove(&key);
            }
        }
        learned
    }
}

/// A correction teaches a term when what it wrote isn't everyday English, and is either shaped
/// like a name or code, or a respelling of what was heard ("notch base" to "notchbase").
fn worth_learning(from: &str, to: &str) -> bool {
    let words = split(to);
    if words.is_empty() || words.len() > 4 || words.iter().all(|word| common(&word.text)) {
        return false;
    }
    let shaped = to.chars().any(|c| c.is_uppercase() || c.is_numeric() || matches!(c, '.' | '_' | '/' | '#'));
    shaped || likeness(&squash(from), &squash(to)) >= 0.6
}

/// Terms worth knowing from what's on screen: names, products and code near the cursor.
pub fn from_screen(text: &str) -> Vec<String> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for (order, word) in split(text).into_iter().enumerate() {
        if term_like(&word) {
            let stem = possessive(&word.text).0.trim_end_matches(['.', '-']).to_owned();
            let entry = counts.entry(stem).or_insert((0, order));
            entry.0 += 1;
            entry.1 = order;
        }
    }
    let mut terms: Vec<(String, (usize, usize))> = counts.into_iter().collect();
    // Said more, then nearer the end, which is where the cursor usually is.
    terms.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(b.1.1.cmp(&a.1.1)));
    terms.into_iter().take(SCREEN_TERMS).map(|(term, _)| term).collect()
}

fn replace_phrase(text: &str, from: &str, to: &str) -> String {
    let from_words: Vec<String> = split(from).into_iter().map(|w| w.text.to_lowercase()).collect();
    if from_words.is_empty() {
        return text.to_owned();
    }
    let words = split(text);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i + from_words.len() <= words.len() {
        let span = &words[i..i + from_words.len()];
        let last = possessive(&span[span.len() - 1].text);
        let matches = span.iter().zip(&from_words).enumerate().all(|(k, (word, wanted))| {
            let text = if k == span.len() - 1 { last.0 } else { word.text.as_str() };
            text.to_lowercase() == *wanted
        });
        if matches {
            out.push_str(&text[copied..span[0].start]);
            out.push_str(to);
            out.push_str(last.1);
            copied = span[span.len() - 1].end;
            i += from_words.len();
        } else {
            i += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A run of heard words that should be `term`.
fn snaps_to(span: &[Word], term: &str) -> bool {
    let joined: String = span.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" ");
    let (heard, _) = possessive(&joined);
    if heard == term {
        return false;
    }
    let (heard_squashed, term_squashed) = (squash(heard), squash(term));
    if heard_squashed == term_squashed {
        // Same letters. Only a capital or a space apart, which only matters for terms that have
        // capitals inside ("GitHub") or words heard apart ("notch base").
        let shaped = term.chars().skip(1).any(char::is_uppercase) || term.chars().any(|c| c.is_numeric());
        return span.len() > 1 || shaped;
    }
    let one_word = span.len() == 1 && !term.contains(' ');
    let first = |text: &str| text.chars().next().map(|c| c.to_lowercase().collect::<String>());
    one_word
        && term_squashed.chars().count() >= SNAP_SHORTEST
        && first(heard) == first(term)
        && !common(heard)
        && !common(term)
        && likeness(&heard_squashed, &term_squashed) >= SNAP_LIKENESS
}

fn snap(text: &str, terms: &[&str]) -> String {
    let words = split(text);
    if words.is_empty() || terms.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    'words: while i < words.len() {
        for length in (1..=3).rev() {
            let Some(span) = words.get(i..i + length) else { continue };
            // A span with a full stop inside is two sentences, not one term.
            if span.windows(2).any(|pair| text[pair[0].end..pair[1].start].trim() != "") {
                continue;
            }
            let Some(term) = terms.iter().find(|term| snaps_to(span, term)) else { continue };
            let ending = possessive(&span[length - 1].text).1;
            out.push_str(&text[copied..span[0].start]);
            out.push_str(term);
            out.push_str(ending);
            copied = span[length - 1].end;
            i += length;
            continue 'words;
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knowing(terms: &[&str]) -> Glossary {
        let mut glossary = Glossary::default();
        for term in terms {
            glossary.add(term, 1);
        }
        glossary
    }

    #[test]
    fn heard_apart_or_misspelled_terms_are_respelled() {
        let glossary = knowing(&["notchbase", "GitHub", "Terraform", "Priya", "Roam"]);
        let fixed = glossary.fix("I pushed the notch base release to Github. Pria moved it to Teraform.", &[]);
        assert_eq!(fixed, "I pushed the notchbase release to GitHub. Priya moved it to Terraform.");
        assert_eq!(glossary.fix("notch base's update", &[]), "notchbase's update");
    }

    #[test]
    fn everyday_words_are_left_alone() {
        let glossary = knowing(&["Roam", "Mark", "Notes"]);
        assert_eq!(glossary.fix("we roam around the room. Mark it", &[]), "we roam around the room. Mark it");
        let names = knowing(&["Karen"]);
        assert_eq!(names.fix("the care and the caring", &[]), "the care and the caring");
    }

    #[test]
    fn sentences_are_not_joined_into_a_term() {
        let glossary = knowing(&["notchbase"]);
        assert_eq!(glossary.fix("It's a notch. Base camp next.", &[]), "It's a notch. Base camp next.");
    }

    #[test]
    fn rules_apply_once_learned() {
        let mut glossary = Glossary::default();
        assert!(glossary.add("brock => Groq", 1));
        assert_eq!(glossary.fix("ask Brock about Brock's limits", &[]), "ask Groq about Groq's limits");
        assert!(!glossary.add(" => x", 1));
        assert!(glossary.terms.iter().any(|term| term.text == "Groq" && term.source == Source::Added));
    }

    #[test]
    fn corrections_are_learned() {
        let mut glossary = Glossary::default();
        let learned = glossary.corrected("ask Brock about the limit", "ask Groq about the limit", 5);
        assert_eq!(learned, ["Groq"]);
        assert_eq!(glossary.fix("ask Brock", &[]), "ask Brock", "one correction steers the models only");
        glossary.corrected("Brock is fast", "Groq is fast", 6);
        assert_eq!(glossary.fix("ask Brock", &[]), "ask Groq", "twice, and it's fixed every time");
        assert!(glossary.prompt(&[]).contains("Groq"));
    }

    #[test]
    fn everyday_corrections_teach_nothing() {
        let mut glossary = Glossary::default();
        assert!(glossary.corrected("their going home", "they're going home", 1).is_empty());
        assert!(glossary.terms.is_empty());
    }

    #[test]
    fn terms_said_often_are_learned() {
        let mut glossary = Glossary::default();
        for _ in 0..3 {
            assert!(glossary.heard("we ship Kubernetes today", &[], 1).is_empty());
        }
        assert_eq!(glossary.heard("Kubernetes again", &[], 2), ["Kubernetes"]);
        assert!(glossary.heard("then Kubernetes", &[], 3).is_empty(), "known terms only count up");
        let mut screen = Glossary::default();
        let shown = ["dotenv".to_owned()];
        screen.heard("update the dotenv file", &shown, 1);
        assert_eq!(screen.heard("and dotenv again", &shown, 2), ["dotenv"], "said while shown counts double");
    }

    #[test]
    fn prompt_fits_and_leads_with_screen_terms() {
        let mut glossary = Glossary::default();
        for i in 0..200 {
            glossary.add(&format!("Term{i}"), i);
        }
        let prompt = glossary.prompt(&["Visible".into()]);
        assert!(prompt.starts_with("Glossary: ") && prompt.ends_with('.'));
        assert!(prompt.len() <= PROMPT_CHARS, "{}", prompt.len());
        assert!(prompt.contains("Visible"));
        assert_eq!(Glossary::default().prompt(&[]), "");
    }

    #[test]
    fn screen_terms_are_picked_out() {
        let text = "fn main() { let groq = Groq::new(); }\nAsk Priya about the PR for notchbase's GitHub repo.";
        let terms = from_screen(text);
        for wanted in ["Groq", "Priya", "PR", "GitHub"] {
            assert!(terms.iter().any(|term| term == wanted), "{wanted} in {terms:?}");
        }
        assert!(!terms.iter().any(|term| term == "Ask" || term == "about"));
    }

    #[test]
    fn removing_forgets() {
        let mut glossary = knowing(&["brock => Groq"]);
        glossary.remove("Groq");
        assert!(glossary.terms.is_empty() && glossary.rules.is_empty());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("notchbase-dictation-{}", std::process::id()));
        let file = dir.join("glossary.json");
        let mut glossary = Glossary::load(file.clone());
        glossary.add("notchbase", 1);
        glossary.save();
        assert_eq!(Glossary::load(file).terms, glossary.terms);
        let _ = std::fs::remove_dir_all(dir);
    }
}
