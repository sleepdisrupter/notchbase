//! Covers one row of a capsule, filling its solid middle at once and working out from there to the soft edges.

use std::ops::Range;

use egui::Pos2;

use super::capsule::Capsule;
use super::{FULL, PIXEL_CENTRE};

pub(super) fn cover(line: &mut [u8], across: &Range<usize>, capsule: &Capsule, centre_y: f32) -> Range<usize> {
    let level = |x: usize| capsule.coverage(Pos2::new(x as f32 + PIXEL_CENTRE, centre_y));
    let mut covered = wholly_inked(capsule.core(centre_y, across), level);
    if covered.is_empty() {
        let Some(seed) = inked_near(capsule.nearest(centre_y), across, level) else { return covered };
        covered = seed..seed;
    }
    line[covered.start - across.start..covered.end - across.start].fill(FULL);
    while covered.start > across.start && raise(line, across, covered.start - 1, level) {
        covered.start -= 1;
    }
    while covered.end < across.end && raise(line, across, covered.end, level) {
        covered.end += 1;
    }
    covered
}

fn wholly_inked(mut core: Range<usize>, level: impl Fn(usize) -> u8) -> Range<usize> {
    while !core.is_empty() && level(core.start) != FULL {
        core.start += 1;
    }
    while !core.is_empty() && level(core.end - 1) != FULL {
        core.end -= 1;
    }
    core
}

fn inked_near(nearest: f32, across: &Range<usize>, level: impl Fn(usize) -> u8) -> Option<usize> {
    let last = across.end.checked_sub(1).filter(|last| *last >= across.start)?;
    let middle = (nearest.floor().max(0.0) as usize).clamp(across.start, last);
    let beside = [Some(middle), middle.checked_sub(1), middle.checked_add(1)];
    beside.into_iter().flatten().find(|x| across.contains(x) && level(*x) > 0)
}

fn raise(line: &mut [u8], across: &Range<usize>, x: usize, level: impl Fn(usize) -> u8) -> bool {
    let inked = level(x);
    let held = &mut line[x - across.start];
    *held = (*held).max(inked);
    inked > 0
}
