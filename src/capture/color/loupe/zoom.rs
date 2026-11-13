//! Loupe zoom: a cell size from 6 to 20 points, stepped by the wheel on a log scale, always whole pixels.

pub(super) const DEFAULT_CELL: f32 = 10.0;
pub(super) const MIN_CELL: f32 = 6.0;
pub(super) const MAX_CELL: f32 = 20.0;
const ZOOM_STEP: f32 = 0.15;

pub(super) fn cell_target(zoom_log: f32, ppp: f32) -> f32 {
    let px = (zoom_log.exp2().clamp(MIN_CELL, MAX_CELL) * ppp).round();
    px.clamp((MIN_CELL * ppp).ceil(), (MAX_CELL * ppp).floor()) / ppp
}

pub(super) fn next_zoom(zoom_log: f32, notches: f32, current: f32, ppp: f32) -> (f32, f32) {
    let (lo, hi) = (MIN_CELL.log2(), MAX_CELL.log2());
    let log = (zoom_log + notches * ZOOM_STEP).clamp(lo, hi);
    let target = cell_target(log, ppp);
    if notches.abs() < 1.0 || (target - current).abs() * ppp >= 0.5 {
        return (log, target);
    }
    // A whole notch that rounds back to the same size still moves a pixel, so every notch shows.
    let px = ((current * ppp).round() + notches.signum()).clamp((MIN_CELL * ppp).ceil(), (MAX_CELL * ppp).floor());
    let target = px / ppp;
    (target.log2().clamp(lo, hi), target)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [f32; 8] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];

    #[test]
    fn zoom_targets_are_whole_pixels() {
        for ppp in SCALES {
            let mut log = MIN_CELL.log2() - 1.0;
            while log < MAX_CELL.log2() + 1.0 {
                let cell = cell_target(log, ppp);
                let px = cell * ppp;
                assert!((px - px.round()).abs() < 1e-3, "ppp {ppp}: {px}");
                assert!((MIN_CELL - 1e-4..=MAX_CELL + 1e-4).contains(&cell));
                log += 0.05;
            }
        }
    }

    #[test]
    fn every_notch_moves_zoom_to_limit() {
        for ppp in SCALES {
            let max = cell_target(MAX_CELL.log2(), ppp);
            let min = cell_target(MIN_CELL.log2(), ppp);
            let (mut log, mut cell) = (MIN_CELL.log2(), min);
            let mut notches = 0;
            while cell < max {
                let (next_log, next) = next_zoom(log, 1.0, cell, ppp);
                assert!(next > cell, "ppp {ppp}: stuck at {cell}");
                (log, cell) = (next_log, next);
                notches += 1;
            }
            assert!(notches >= 8, "zooming across the range should take a few notches");
            assert_eq!(next_zoom(log, 1.0, cell, ppp).1, max);
            while cell > min {
                let (next_log, next) = next_zoom(log, -1.0, cell, ppp);
                assert!(next < cell, "ppp {ppp}: stuck at {cell}");
                (log, cell) = (next_log, next);
            }
            assert_eq!(next_zoom(log, -1.0, cell, ppp).1, min);
        }
    }
}
