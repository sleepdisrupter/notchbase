use std::path::Path;
use std::time::Instant;

use super::golden::{MONDAY_NOON, at, open};
use super::personal::personal;
use super::suggest::suggest;
use crate::find::context::MINUTE;
use crate::find::kind::Kind;
use crate::find::model::Live;
use crate::find::rank::{Candidate, keys};
use crate::find::tests::corpus::from_paths;
use crate::find::usage::Usage;
use crate::find::view::SHOWN;
use crate::find::weights::{FILES, USAGE_SHORTLIST};

const CANDIDATES: usize = 2 * USAGE_SHORTLIST;

fn candidate(live: &Live, path: &str) -> Candidate {
    let id = live.resolve(Path::new(path)).unwrap();
    let (flags, kind) = (live.flags(id), Kind::of(live.folded(id), live.flags(id)));
    Candidate { id, keys: keys(live, id), kind, flags, modified: live.modified(id), created: live.created(id) }
}

#[test]
#[ignore = "prints how long the usage lock is held over a full model; run with --release for the real numbers"]
fn usage_lock_held_briefly() {
    let paths: Vec<String> = (0..FILES)
        .map(|serial| format!("/Users/john/GitHub/project-{}/src/file-{serial:04}.rs", serial % 97))
        .collect();
    let usage = Usage::load(None, MONDAY_NOON);
    for (serial, path) in paths.iter().enumerate() {
        let typed = format!("file-{:02}", serial % 100);
        open(&usage, path, &typed, &at(MONDAY_NOON + serial as u32 * MINUTE, Some("Xcode")));
    }
    let now = at(MONDAY_NOON + FILES as u32 * MINUTE, Some("Xcode"));
    let shortlist: Vec<&str> = paths.iter().rev().take(CANDIDATES).map(String::as_str).collect();
    let live = from_paths(&shortlist);
    let candidates: Vec<Candidate> = shortlist.iter().map(|path| candidate(&live, path)).collect();
    let started = Instant::now();
    let folded_and_matched = usage.matching(&[b"file-40", b"src"]).len();
    let first_match = started.elapsed();
    let started = Instant::now();
    let matched = usage.matching(&[b"file-39", b"src"]).len();
    let later_match = started.elapsed();
    let started = Instant::now();
    let scored = usage.read(|model| candidates.iter().map(|who| personal(model, who, b"fi", &now).prefix).sum::<f32>());
    let personal_stage = started.elapsed();
    let started = Instant::now();
    let suggested = usage.read(|model| suggest(&live, model, &now, SHOWN)).len();
    let suggestions = started.elapsed();
    println!("matching, first (folds {FILES} paths): {first_match:?} for {folded_and_matched} paths");
    println!("matching, later: {later_match:?} for {matched} paths");
    println!("personal terms of {CANDIDATES} candidates: {personal_stage:?} (prefix sum {scored})");
    println!("the empty-query list: {suggestions:?} for {suggested} rows");
    assert!(folded_and_matched > 0 && matched > 0 && scored > 0.0 && suggested == SHOWN);
}
