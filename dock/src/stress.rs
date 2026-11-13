//! A stress run of minimizing and restoring, on a TextEdit window only, for measuring how reliable it is on a real
//! screen: `echo 100 > "<data folder>/try-stress"` then `notchbase-dock --try-minimize` with TextEdit in front.
//! Cycles mix plain ones, quick repeats, asks over each other, a big window, one in a corner and one on another
//! screen; after each the end state is checked and written to `stress.log`, with every flight's steps.

use std::time::{Duration, Instant};

use egui::{Rect, pos2, vec2};

/// One thing a cycle does, this long after it starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// Moves and sizes the window (before minimizing).
    Place(Rect),
    Minimize,
    Restore,
    /// Checks that it all ended cleanly: the window up where it was placed, nothing left over.
    Check,
}

/// The kinds of cycle, in turn.
pub const KINDS: [&str; 6] = ["plain", "quick", "overlap", "big", "corner", "other screen"];

/// The acts of cycle `n` on a screen (global top-left points), and the other screen if there is one.
pub fn cycle(n: u32, screen: Rect, other: Option<Rect>) -> (&'static str, Vec<(Duration, Act)>) {
    let ms = Duration::from_millis;
    let normal = Rect::from_center_size(screen.center() - vec2(0.0, 40.0), vec2(720.0, 480.0));
    let kind = KINDS[n as usize % KINDS.len()];
    let place = match kind {
        "big" => Rect::from_min_max(screen.min + vec2(20.0, 60.0), screen.max - vec2(20.0, 110.0)),
        "corner" => Rect::from_min_size(pos2(screen.right() - 520.0, screen.bottom() - 420.0), vec2(500.0, 400.0)),
        "other screen" => other.map_or(normal, |o| Rect::from_center_size(o.center(), vec2(720.0, 480.0))),
        _ => normal,
    };
    let acts = match kind {
        // A click on the tile before the window has finished going down: it waits its turn.
        "quick" => vec![(ms(0), Act::Minimize), (ms(150), Act::Restore), (ms(4000), Act::Check)],
        // Asks on top of each other: one minimize and one restore come of them, never two.
        "overlap" => vec![
            (ms(0), Act::Minimize),
            (ms(40), Act::Minimize),
            (ms(90), Act::Restore),
            (ms(130), Act::Restore),
            (ms(4500), Act::Check),
        ],
        _ => vec![(ms(0), Act::Minimize), (ms(1500), Act::Restore), (ms(3500), Act::Check)],
    };
    let mut all = vec![(ms(0), Act::Place(place))];
    all.extend(acts.into_iter().map(|(at, act)| (at + ms(500), act)));
    (kind, all)
}

/// Whether a window ended where it should: within a few points of where it was placed (the system may nudge it
/// under the menu bar).
pub fn in_place(frame: Rect, placed: Rect) -> bool {
    (frame.min - placed.min).length() < 6.0 && (frame.size() - placed.size()).length() < 6.0
}

/// A run in progress.
pub struct Run {
    pub pid: i32,
    pub id: u32,
    pub left: u32,
    pub n: u32,
    pub started: Instant,
    pub acts: Vec<(Duration, Act)>,
    pub kind: &'static str,
    pub placed: Rect,
    pub passed: u32,
    pub failed: u32,
}

impl Run {
    pub fn new(pid: i32, id: u32, cycles: u32) -> Self {
        Self {
            pid,
            id,
            left: cycles,
            n: 0,
            started: Instant::now(),
            acts: Vec::new(),
            kind: "",
            placed: Rect::NOTHING,
            passed: 0,
            failed: 0,
        }
    }

    /// The acts due now.
    pub fn due(&mut self) -> Vec<Act> {
        let elapsed = self.started.elapsed();
        let (now, later): (Vec<_>, Vec<_>) = self.acts.drain(..).partition(|(at, _)| *at <= elapsed);
        self.acts = later;
        now.into_iter().map(|(_, act)| act).collect()
    }
}

pub fn log(line: &str) {
    use std::io::Write as _;
    let path = crate::lifecycle::data_dir().join("stress.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { min: pos2(0.0, 33.0), max: pos2(1470.0, 956.0) };

    #[test]
    fn every_cycle_places_the_window_first_and_checks_last() {
        for n in 0..12 {
            let (kind, acts) = cycle(n, SCREEN, None);
            assert!(matches!(acts[0].1, Act::Place(_)), "{kind}");
            assert_eq!(acts.last().unwrap().1, Act::Check, "{kind}");
            assert!(acts.windows(2).all(|w| w[0].0 <= w[1].0), "{kind}: in time order");
            assert!(acts.iter().any(|a| a.1 == Act::Minimize) && acts.iter().any(|a| a.1 == Act::Restore));
        }
    }

    #[test]
    fn windows_are_placed_on_screen_and_on_the_other_one_when_asked() {
        let other = Rect::from_min_size(pos2(1470.0, 0.0), vec2(1920.0, 1080.0));
        for n in 0..6 {
            let (kind, acts) = cycle(n, SCREEN, Some(other));
            let Act::Place(p) = acts[0].1 else { panic!() };
            let on = if kind == "other screen" { other } else { SCREEN };
            assert!(on.contains_rect(p), "{kind}: {p:?}");
        }
    }

    #[test]
    fn a_window_a_point_or_two_off_is_still_in_place() {
        let placed = Rect::from_min_size(pos2(100.0, 100.0), vec2(700.0, 500.0));
        assert!(in_place(placed.translate(vec2(1.0, 2.0)), placed));
        assert!(!in_place(placed.translate(vec2(0.0, 40.0)), placed));
    }
}
