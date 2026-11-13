//! What the self-test saw, and the checks it prints.

use std::path::PathBuf;

#[derive(Default)]
pub struct Outcome {
    pub drag_result: Option<Result<(), String>>,
    pub hovers_seen: usize,
    pub dropped: Vec<PathBuf>,
    pub release_seen_after_drag: bool,
}

impl Outcome {
    pub fn report(&self, dragged: &[PathBuf]) -> bool {
        let checks = [
            ("start_drag returned Ok", matches!(self.drag_result, Some(Ok(())))),
            ("the window saw the drag hover", self.hovers_seen > 0),
            ("the window received the dropped files", same_files(&self.dropped, dragged)),
            ("no mouse release reached the window", !self.release_seen_after_drag),
        ];
        let printed: Vec<bool> = checks.into_iter().map(|(check, passed)| print_check(check, passed)).collect();
        printed.into_iter().all(|passed| passed)
    }
}

pub fn print_check(check: &str, passed: bool) -> bool {
    println!("[{}] {check}", if passed { "ok" } else { "FAIL" });
    passed
}

fn same_files(dropped: &[PathBuf], dragged: &[PathBuf]) -> bool {
    dropped.len() == dragged.len() && canonical_sorted(dropped) == canonical_sorted(dragged)
}

fn canonical_sorted(paths: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let mut canonical: Vec<_> = paths.iter().map(|path| std::fs::canonicalize(path).ok()).collect();
    canonical.sort();
    canonical
}
