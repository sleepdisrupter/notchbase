//! Turns each listing into outcomes: progress, a finished file, a stall after 20 seconds of no growth, or a lost file.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Download;

use super::scan::{Listing, Partial, finished_name};

const STALL: Duration = Duration::from_secs(20);
const UNNAMED: &str = "Download";
const UNCONFIRMED: &str = "Unconfirmed ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Progress(Download),
    Finished(Download, String),
    Stalled(Download),
    Lost(Download),
}

struct Seen {
    size: u64,
    total: Option<u64>,
    lands_as: Option<String>,
    moved: Instant,
    showing: bool,
}

#[derive(Default)]
pub(super) struct Tracker {
    primed: bool,
    seen: HashMap<String, Seen>,
}

impl Tracker {
    pub(super) fn observe(&mut self, listing: &Listing, now: Instant) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        for partial in &listing.partials {
            match self.seen.get_mut(&partial.name) {
                Some(seen) if seen.size != partial.size => {
                    *seen = Seen::of(partial, now, true);
                    outcomes.push(Outcome::Progress(download(partial)));
                }
                Some(seen) if seen.showing && now.duration_since(seen.moved) >= STALL => {
                    seen.showing = false;
                    outcomes.push(Outcome::Stalled(download(partial)));
                }
                Some(_) => {}
                None => {
                    let showing = self.primed;
                    self.seen.insert(partial.name.clone(), Seen::of(partial, now, showing));
                    if showing {
                        outcomes.push(Outcome::Progress(download(partial)));
                    }
                }
            }
        }
        self.primed = true;
        let listed = |name: &String| listing.partials.iter().any(|partial| &partial.name == name);
        let gone: Vec<String> = self.seen.keys().filter(|name| !listed(name)).cloned().collect();
        for name in gone {
            let Some(seen) = self.seen.remove(&name).filter(|seen| seen.showing) else { continue };
            let partial = Partial { name, size: seen.size, total: seen.total, lands_as: seen.lands_as };
            let landed = finished_name(&partial.name).filter(|landed| listing.settled.contains(*landed));
            outcomes.push(match landed {
                Some(landed) => Outcome::Finished(download(&partial), landed.to_owned()),
                None => Outcome::Lost(download(&partial)),
            });
        }
        outcomes
    }
}

impl Seen {
    fn of(partial: &Partial, moved: Instant, showing: bool) -> Self {
        Self { size: partial.size, total: partial.total, lands_as: partial.lands_as.clone(), moved, showing }
    }
}

fn download(partial: &Partial) -> Download {
    let name = partial.lands_as.clone().unwrap_or_else(|| title(&partial.name));
    Download { id: partial.name.clone(), name, received: partial.size, total: partial.total }
}

fn title(name: &str) -> String {
    match finished_name(name) {
        Some(stem) if !stem.starts_with(UNCONFIRMED) => stem.to_owned(),
        _ => UNNAMED.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn listing(partials: &[(&str, u64)], settled: &[&str]) -> Listing {
        Listing {
            partials: partials
                .iter()
                .map(|(name, size)| Partial { name: (*name).to_owned(), size: *size, total: None, lands_as: None })
                .collect(),
            settled: settled.iter().map(|name| (*name).to_owned()).collect::<HashSet<String>>(),
        }
    }

    fn movie(received: u64) -> Download {
        Download { id: "movie.mp4.crdownload".into(), name: "movie.mp4".into(), received, total: None }
    }

    #[test]
    fn followed_to_the_finished_file() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        assert!(tracker.observe(&listing(&[], &["old.pdf"]), t0).is_empty());
        let started = tracker.observe(&listing(&[("movie.mp4.crdownload", 10)], &["old.pdf"]), t0);
        assert_eq!(started, [Outcome::Progress(movie(10))]);
        let grew = tracker.observe(&listing(&[("movie.mp4.crdownload", 40)], &["old.pdf"]), t0);
        assert_eq!(grew, [Outcome::Progress(movie(40))]);
        assert!(tracker.observe(&listing(&[("movie.mp4.crdownload", 40)], &["old.pdf"]), t0).is_empty());
        let landed = tracker.observe(&listing(&[], &["old.pdf", "movie.mp4"]), t0);
        assert_eq!(landed, [Outcome::Finished(movie(40), "movie.mp4".into())]);
        assert!(tracker.observe(&listing(&[], &["old.pdf", "movie.mp4"]), t0).is_empty());
    }

    #[test]
    fn leftovers_quiet_until_they_grow() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        let stale = listing(&[("movie.mp4.crdownload", 10)], &[]);
        assert!(tracker.observe(&stale, t0).is_empty());
        assert!(tracker.observe(&stale, t0 + STALL * 3).is_empty());
        let resumed = tracker.observe(&listing(&[("movie.mp4.crdownload", 11)], &[]), t0 + STALL * 3);
        assert_eq!(resumed, [Outcome::Progress(movie(11))]);
    }

    #[test]
    fn deleted_leftover_was_nothing() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(&listing(&[("movie.mp4.crdownload", 10)], &[]), t0);
        assert!(tracker.observe(&listing(&[], &["movie.mp4"]), t0).is_empty());
    }

    #[test]
    fn stalled_download_comes_back() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(&listing(&[], &[]), t0);
        let paused = listing(&[("movie.mp4.crdownload", 10)], &[]);
        tracker.observe(&paused, t0);
        assert!(tracker.observe(&paused, t0 + STALL / 2).is_empty());
        assert_eq!(tracker.observe(&paused, t0 + STALL), [Outcome::Stalled(movie(10))]);
        assert!(tracker.observe(&paused, t0 + STALL * 2).is_empty());
        let resumed = tracker.observe(&listing(&[("movie.mp4.crdownload", 30)], &[]), t0 + STALL * 2);
        assert_eq!(resumed, [Outcome::Progress(movie(30))]);
    }

    #[test]
    fn vanished_download_is_lost() {
        let mut tracker = Tracker::default();
        let t0 = Instant::now();
        tracker.observe(&listing(&[], &[]), t0);
        tracker.observe(&listing(&[("movie.mp4.crdownload", 10)], &[]), t0);
        assert_eq!(tracker.observe(&listing(&[], &["other.txt"]), t0), [Outcome::Lost(movie(10))]);
    }

    #[test]
    fn unnamed_download_gets_plain_title() {
        assert_eq!(title("Unconfirmed 481516.crdownload"), UNNAMED);
        assert_eq!(title("song.mp3.download"), "song.mp3");
    }
}
