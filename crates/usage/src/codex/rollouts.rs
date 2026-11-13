//! Recent session logs: the 64 newest from the latest 7 day folders and the 16 newest archived, newest first.

use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const DAY_DIRS: usize = 7;
const SESSION_FILES: usize = 64;
const ARCHIVED_FILES: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Rollout {
    pub(super) path: PathBuf,
    pub(super) len: u64,
    pub(super) modified: SystemTime,
}

pub(super) fn recent(home: &Path) -> io::Result<Vec<Rollout>> {
    let mut sessions = Vec::new();
    for day in recent_days(&home.join("sessions"))? {
        sessions.extend(rollout_files(&day)?);
    }
    let mut paths = newest_names(sessions, SESSION_FILES);
    paths.extend(newest_names(rollout_files(&home.join("archived_sessions"))?, ARCHIVED_FILES));
    let mut rollouts: Vec<Rollout> = paths.into_iter().filter_map(stat).collect();
    rollouts.sort_by_key(|rollout| Reverse(rollout.modified));
    Ok(rollouts)
}

fn recent_days(sessions: &Path) -> io::Result<Vec<PathBuf>> {
    let mut days = Vec::new();
    for year in numbered(sessions)? {
        for month in numbered(&year)? {
            for day in numbered(&month)? {
                if days.len() == DAY_DIRS {
                    return Ok(days);
                }
                days.push(day);
            }
        }
    }
    Ok(days)
}

fn numbered(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs: Vec<(String, PathBuf)> = entries(dir)?
        .into_iter()
        .filter(|(name, path)| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) && path.is_dir())
        .collect();
    // By length, then name, so the numbers sort right without parsing them. Newest first.
    dirs.sort_by(|(a, _), (b, _)| (b.len(), b).cmp(&(a.len(), a)));
    Ok(dirs.into_iter().map(|(_, path)| path).collect())
}

fn rollout_files(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let is_rollout = |name: &str| name.starts_with("rollout-") && name.ends_with(".jsonl");
    Ok(entries(dir)?.into_iter().filter(|(name, _)| is_rollout(name)).collect())
}

fn newest_names(mut files: Vec<(String, PathBuf)>, keep: usize) -> Vec<PathBuf> {
    files.sort_by(|(a, _), (b, _)| b.cmp(a));
    files.into_iter().take(keep).map(|(_, path)| path).collect()
}

fn entries(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    Ok(listing
        .filter_map(Result::ok)
        .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
        .collect())
}

fn stat(path: PathBuf) -> Option<Rollout> {
    let meta = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
    Some(Rollout { len: meta.len(), modified: meta.modified().ok()?, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::fixture::{Scratch, T0, at};

    fn names(rollouts: &[Rollout]) -> Vec<String> {
        rollouts.iter().map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn empty_homes_have_no_rollouts() {
        let dir = Scratch::new();
        assert!(recent(&dir.path().join("nowhere")).unwrap().is_empty());
        assert!(recent(dir.path()).unwrap().is_empty());
        std::fs::create_dir_all(dir.path().join("sessions").join("2026")).unwrap();
        assert!(recent(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn newest_first_including_archived() {
        let dir = Scratch::new();
        dir.write("sessions/2026/09/20/rollout-2026-09-20T08-00-00-a.jsonl", &["{}".into()], at(T0 + 30));
        dir.write("sessions/2026/09/21/rollout-2026-09-21T08-00-00-b.jsonl", &["{}".into()], at(T0 + 10));
        dir.write("sessions/2026/09/21/notes.txt", &["{}".into()], at(T0 + 90));
        dir.write("sessions/2026/09/21/rollout-2026-09-21T09-00-00-c.json", &["{}".into()], at(T0 + 90));
        dir.write("sessions/2026/drafts/rollout-2026-09-21T09-00-00-d.jsonl", &["{}".into()], at(T0 + 90));
        dir.write("archived_sessions/rollout-2026-09-19T08-00-00-e.jsonl", &["{}".into(), "{}".into()], at(T0 + 20));
        let found = recent(dir.path()).unwrap();
        assert_eq!(
            names(&found),
            [
                "rollout-2026-09-20T08-00-00-a.jsonl",
                "rollout-2026-09-19T08-00-00-e.jsonl",
                "rollout-2026-09-21T08-00-00-b.jsonl"
            ]
        );
        assert_eq!((found[0].len, found[0].modified), (3, at(T0 + 30)));
        assert_eq!(found[1].len, 6);
    }

    #[test]
    fn only_the_newest_days_are_listed() {
        let dir = Scratch::new();
        for day in 1..=9 {
            let month = if day <= 4 { "08" } else { "09" };
            let path = format!("sessions/2026/{month}/{day:02}/rollout-2026-{month}-{day:02}T00-00-00-x.jsonl");
            dir.write(&path, &["{}".into()], at(T0 + day));
        }
        dir.write("sessions/2025/12/31/rollout-2025-12-31T00-00-00-x.jsonl", &["{}".into()], at(T0 + 100));
        let found = recent(dir.path()).unwrap();
        assert_eq!(found.len(), DAY_DIRS);
        assert_eq!(found.first().unwrap().modified, at(T0 + 9));
        assert_eq!(found.last().unwrap().modified, at(T0 + 3));
    }

    #[test]
    fn busy_days_keep_the_newest_names() {
        let dir = Scratch::new();
        for n in 0..SESSION_FILES + 5 {
            let path = format!("sessions/2026/09/21/rollout-2026-09-21T00-00-{n:03}.jsonl");
            dir.write(&path, &[], at(T0));
        }
        let found = recent(dir.path()).unwrap();
        assert_eq!(found.len(), SESSION_FILES);
        assert!(!names(&found).contains(&"rollout-2026-09-21T00-00-004.jsonl".to_owned()));
        assert!(names(&found).contains(&"rollout-2026-09-21T00-00-005.jsonl".to_owned()));
    }
}
