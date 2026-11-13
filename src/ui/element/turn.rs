//! Slides one page out and the next in when a month or week changes, in the direction it moved.

use egui::{Id, Rect, vec2};

use super::Scene;
use crate::ui::canvas::{fade, scale_about};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};

const SWAP: SpringConfig = SpringConfig::new(0.5, 0.8);
const LONGEST_STEP: f32 = 1.0 / 30.0;
const SLIDE: f32 = 0.42;
const SHRUNK: f32 = 0.94;

#[derive(Clone)]
struct Turn<T> {
    shown: Option<(i64, T)>,
    leaving: Option<T>,
    direction: f32,
    progress: Spring,
}

pub(super) struct Swap<T> {
    pub(super) leaving: Option<T>,
    pub(super) progress: f32,
    pub(super) direction: f32,
}

struct Motion {
    shift: f32,
    scale: f32,
    alpha: f32,
}

pub(super) fn advance<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: Id, shown: &T, key: i64) -> Swap<T> {
    let dt = ctx.input(|input| input.stable_dt).min(LONGEST_STEP);
    let fresh =
        || Turn::<T> { shown: None, leaving: None, direction: 1.0, progress: Spring::new(1.0, SWAP, UNIT_PRECISION) };
    let mut turn = ctx.data_mut(|data| data.get_temp::<Turn<T>>(id)).unwrap_or_else(fresh);
    if let Some((before, previous)) = turn.shown.as_ref().filter(|(before, _)| *before != key) {
        turn.direction = (key - before).signum() as f32;
        turn.leaving = Some(previous.clone());
        turn.progress.snap_to(0.0);
        turn.progress.animate_to(1.0, SWAP);
    }
    turn.progress.step(dt);
    if turn.progress.is_settled() {
        turn.leaving = None;
    } else {
        ctx.request_repaint();
    }
    turn.shown = Some((key, shown.clone()));
    let swap = Swap { leaving: turn.leaving.clone(), progress: turn.progress.value, direction: turn.direction };
    ctx.data_mut(|data| data.insert_temp(id, turn));
    swap
}

pub(super) fn slide<T>(
    scene: &mut Scene,
    rect: Rect,
    current: &T,
    swap: &Swap<T>,
    mut paint: impl FnMut(&mut Scene, &T, bool),
) {
    let travel = rect.width() * SLIDE;
    let settled = swap.progress.clamp(0.0, 1.0);
    if let Some(leaving) = &swap.leaving {
        let motion = Motion {
            shift: -swap.direction * travel * settled,
            scale: lerp(1.0, SHRUNK, settled),
            alpha: 1.0 - smoothstep(0.0, 0.6, settled),
        };
        moving(scene, rect, &motion, |scene| paint(scene, leaving, false));
    }
    let motion = Motion {
        shift: swap.direction * travel * (1.0 - swap.progress),
        scale: lerp(SHRUNK, 1.0, settled),
        alpha: smoothstep(0.15, 0.85, settled),
    };
    moving(scene, rect, &motion, |scene| paint(scene, current, true));
}

fn moving(scene: &mut Scene, rect: Rect, motion: &Motion, paint: impl FnOnce(&mut Scene)) {
    let mark = scene.canvas.mark();
    paint(scene);
    if motion.shift.abs() < 0.01 && motion.scale >= 0.999 && motion.alpha >= 0.999 {
        return;
    }
    let zoom = scale_about(rect.center(), motion.scale);
    for shape in scene.canvas.since(mark) {
        shape.transform(zoom);
        shape.translate(vec2(motion.shift, 0.0));
        fade(shape, motion.alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, shown: &str, key: i64) -> Swap<String> {
        let mut swap = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            swap = Some(advance(ui.ctx(), Id::new("t"), &shown.to_owned(), key));
        });
        output.textures_delta.clear();
        swap.expect("a frame ran")
    }

    #[test]
    fn old_page_stays_until_new_lands() {
        let ctx = egui::Context::default();
        let first = frame(&ctx, "September", 9);
        assert!(first.leaving.is_none() && first.progress >= 0.999);
        let next = frame(&ctx, "October", 10);
        assert_eq!(next.leaving.as_deref(), Some("September"));
        assert_eq!(next.direction, 1.0);
        assert!(next.progress < 0.5, "it has only just started");
        let back = frame(&ctx, "September", 9);
        assert_eq!(back.direction, -1.0);
        let mut settled = frame(&ctx, "September", 9);
        for _ in 0..240 {
            settled = frame(&ctx, "September", 9);
        }
        assert!(settled.leaving.is_none());
    }
}
