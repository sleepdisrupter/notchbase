//! Runs an operation file by file and sums it up: what was saved and how much, what was already compact, what failed.

use std::path::PathBuf;

use super::{Outcome, Source};
use crate::shelf::names::file_name;
use crate::shelf::{Notice, Task};

pub(super) enum FileResult {
    Saved { path: PathBuf, before: u64, after: u64 },
    AlreadyCompact,
}

#[derive(Clone, Copy)]
pub(super) enum Summary {
    Converted,
    Compressed,
}

#[derive(Default)]
struct Tally {
    saved: Vec<PathBuf>,
    bytes_before: u64,
    bytes_after: u64,
    already_compact: usize,
    errors: Vec<String>,
    cancelled: bool,
}

pub(super) fn each_file(
    sources: &[Source],
    task: &mut Task,
    summary: Summary,
    mut one: impl FnMut(&Source, &mut Task) -> Result<FileResult, String>,
) -> Outcome {
    let mut tally = Tally::default();
    for (index, source) in sources.iter().enumerate() {
        let mut part = task.part(index, sources.len());
        let result = one(source, &mut part);
        if part.cancelled() {
            tally.cancelled = true;
            if let Ok(saved) = result {
                tally.record(Ok(saved));
            }
            break;
        }
        tally.record(result);
        part.report(1.0);
    }
    tally.into_outcome(summary)
}

pub(super) fn saving_percent(before: u64, after: u64) -> u64 {
    if before == 0 || after >= before {
        return 0;
    }
    let (before, after) = (u128::from(before), u128::from(after));
    (((before - after) * 100 + before / 2) / before) as u64
}

impl Tally {
    fn record(&mut self, result: Result<FileResult, String>) {
        match result {
            Ok(FileResult::Saved { path, before, after }) => {
                self.saved.push(path);
                self.bytes_before += before;
                self.bytes_after += after;
            }
            Ok(FileResult::AlreadyCompact) => self.already_compact += 1,
            Err(text) => self.errors.push(text),
        }
    }

    fn into_outcome(self, summary: Summary) -> Outcome {
        if self.cancelled {
            return Outcome { notice: Notice::failed(cancelled_text(self.saved.len())), outputs: self.saved };
        }
        if self.saved.is_empty() && self.already_compact == 0 {
            return Outcome::failed(self.failure_text());
        }
        let mut text = summary.text(&self);
        if !self.errors.is_empty() {
            text.push_str(&format!(" · {} failed", self.errors.len()));
        }
        Outcome { outputs: self.saved, notice: Notice::ok(text) }
    }

    fn failure_text(&self) -> String {
        match self.errors.as_slice() {
            [] => "Nothing to do".to_owned(),
            [only] => only.clone(),
            [first, rest @ ..] => format!("{first} (+{} more)", rest.len()),
        }
    }
}

fn cancelled_text(saved: usize) -> String {
    match saved {
        0 => "Cancelled".to_owned(),
        1 => "Cancelled · 1 file saved".to_owned(),
        saved => format!("Cancelled · {saved} files saved"),
    }
}

impl Summary {
    fn text(self, tally: &Tally) -> String {
        let saving = saving_percent(tally.bytes_before, tally.bytes_after);
        match (self, tally.saved.as_slice()) {
            (Summary::Compressed, []) => "Already compact".to_owned(),
            (Summary::Compressed, [one]) => format!("{} · {saving}% smaller", file_name(one)),
            (Summary::Compressed, many) => format!("Compressed {} files · {saving}% smaller", many.len()),
            (Summary::Converted, [one]) => format!("Saved {}", file_name(one)),
            (Summary::Converted, many) => format!("Saved {} files", many.len()),
        }
    }
}
