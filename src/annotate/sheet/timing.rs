use std::time::{Duration, Instant};

use egui::{Pos2, pos2, vec2};

use super::*;
use crate::annotate::Stroke;
use crate::annotate::testing::solid;
use crate::ui::color;

const THICKEST_BRUSH: f32 = 32.0;
const PASSES: usize = 10;
const PASS_LENGTH: f32 = 300.0;
const PASS_DROP: f32 = 4.0;
const QUICK_HAND: f32 = 10.0;
const SLOW_HAND: f32 = 1.5;
const SECOND_SCRIBBLE_DROP: f32 = 12.0;
const TRIES: usize = 5;
pub(in crate::annotate) const PAPER: [u8; 3] = [246, 245, 242];
pub(in crate::annotate) const LAPTOP: [usize; 2] = [3024, 1964];
pub(in crate::annotate) const FIVE_K: [usize; 2] = [5120, 2880];

struct Case {
    name: &'static str,
    image: [usize; 2],
    scale: f32,
    passes: usize,
}

const CASES: [Case; 5] = [
    Case { name: "a laptop screenshot on its own screen", image: LAPTOP, scale: 0.344, passes: PASSES },
    Case { name: "a 5K screenshot on its own display", image: FIVE_K, scale: 0.36, passes: PASSES },
    Case { name: "a 5K screenshot on a laptop screen", image: FIVE_K, scale: 0.227, passes: PASSES },
    Case { name: "a 5K screenshot in the smallest window", image: FIVE_K, scale: 0.114, passes: PASSES },
    Case { name: "one straight pass, 5K on a laptop screen", image: FIVE_K, scale: 0.227, passes: 1 },
];

fn scribble(case: &Case, hand: f32, drop: f32) -> Stroke {
    let steps = (PASS_LENGTH / hand).ceil() as usize;
    let centre = pos2(case.image[0] as f32, case.image[1] as f32) / 2.0;
    let corner = centre - vec2(PASS_LENGTH, PASS_DROP * case.passes as f32) / (2.0 * case.scale);
    let samples: Vec<Pos2> = (0..case.passes)
        .flat_map(|pass| (0..=steps).map(move |step| (pass, step)))
        .map(|(pass, step)| {
            let along = step as f32 / steps as f32;
            let across = if pass % 2 == 0 { along } else { 1.0 - along };
            let down = (pass as f32 + along) * PASS_DROP + drop;
            corner + vec2(across * PASS_LENGTH, down) / case.scale
        })
        .collect();
    Stroke::from_samples(color::INK, THICKEST_BRUSH / case.scale, &samples).expect("a stroke")
}

pub(in crate::annotate) fn quickest(mut work: impl FnMut() -> Duration) -> f32 {
    (0..TRIES).map(|_| work()).min().unwrap_or_default().as_secs_f32() * 1000.0
}

pub(in crate::annotate) fn timed<Done>(work: impl FnOnce() -> Done) -> Duration {
    let start = Instant::now();
    let _done = work();
    start.elapsed()
}

#[test]
#[ignore = "prints how long the thickest strokes take to commit and undo; run it by name with --release"]
fn thick_strokes_are_timed() {
    for case in &CASES {
        let base = Arc::new(solid(case.image[0], case.image[1], PAPER));
        for (hand, pace) in [(QUICK_HAND, "quick"), (SLOW_HAND, "slow")] {
            let (under, over) = (scribble(case, hand, 0.0), scribble(case, hand, SECOND_SCRIBBLE_DROP));
            let commit = quickest(|| {
                let mut sheet = Sheet::new(base.clone()).expect("a sheet");
                timed(|| sheet.commit(under.clone()))
            });
            let undo = quickest(|| {
                let mut sheet = Sheet::new(base.clone()).expect("a sheet");
                sheet.commit(under.clone());
                sheet.commit(over.clone());
                timed(|| sheet.undo())
            });
            println!(
                "{}, {pace} hand: {:.0} px ink in {} segments: commit {commit:.1} ms, overlapping undo {undo:.1} ms",
                case.name,
                under.width(),
                under.points().len().saturating_sub(1),
            );
        }
    }
}
