//! Tiles rising off the shelf as their files are sent, then settling back in place.

use std::collections::HashMap;

use egui::emath::TSTransform;
use egui::{Pos2, Rect, pos2};

use crate::shelf::ItemId;
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};

const RISE: SpringConfig = SpringConfig::new(0.45, 1.0);
const SETTLE: SpringConfig = SpringConfig::new(0.3, 1.0);
const HELD_FOR: f32 = 1.2;
const ABOVE_TOP: f32 = 24.0;
const SMALLEST: f32 = 0.3;
const FADE_FROM: f32 = 0.45;
const SETTLE_FROM: f32 = 0.9;

enum Leg {
    Rising(Spring),
    Away(f32),
    Returning(Spring),
}

#[derive(Default)]
pub(super) struct Liftoff {
    flights: HashMap<ItemId, Leg>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pose {
    pub(super) center: Pos2,
    pub(super) scale: f32,
    pub(super) alpha: f32,
}

impl Pose {
    pub(super) fn transform(self, tile: Rect) -> TSTransform {
        let from = tile.center();
        TSTransform::new(self.center.to_vec2() - from.to_vec2() * self.scale, self.scale)
    }
}

impl Liftoff {
    pub(super) fn launch(&mut self, ids: &[ItemId]) {
        for &id in ids {
            let mut spring = Spring::new(0.0, RISE, UNIT_PRECISION);
            spring.animate_to(1.0, RISE);
            self.flights.insert(id, Leg::Rising(spring));
        }
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.flights.retain(|_, leg| {
            match leg {
                Leg::Rising(spring) => {
                    spring.step(dt);
                    if spring.is_settled() {
                        *leg = Leg::Away(0.0);
                    }
                }
                Leg::Away(held) => {
                    *held += dt;
                    if *held >= HELD_FOR {
                        *leg = returning();
                    }
                }
                Leg::Returning(spring) => {
                    spring.step(dt);
                    return !spring.is_settled();
                }
            }
            true
        });
    }

    pub(super) fn rest(&mut self) {
        for leg in self.flights.values_mut() {
            if !matches!(leg, Leg::Returning(_)) {
                *leg = returning();
            }
        }
    }

    pub(super) fn forget_gone(&mut self, alive: impl Fn(&ItemId) -> bool) {
        self.flights.retain(|id, _| alive(id));
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.flights.is_empty()
    }

    pub(super) fn pose(&self, id: ItemId, tile: Rect, page: Rect) -> Option<Pose> {
        let leg = self.flights.get(&id)?;
        let at = tile.center();
        Some(match leg {
            Leg::Rising(spring) => {
                rising(spring.value.clamp(0.0, 1.0), at, pos2(page.center().x, page.top() - ABOVE_TOP))
            }
            Leg::Away(_) => Pose { center: at, scale: SMALLEST, alpha: 0.0 },
            Leg::Returning(spring) => {
                let t = spring.value.clamp(0.0, 1.0);
                Pose { center: at, scale: lerp(SETTLE_FROM, 1.0, t), alpha: t }
            }
        })
    }
}

fn returning() -> Leg {
    let mut spring = Spring::new(0.0, SETTLE, UNIT_PRECISION);
    spring.animate_to(1.0, SETTLE);
    Leg::Returning(spring)
}

fn rising(t: f32, from: Pos2, to: Pos2) -> Pose {
    let up = 1.0 - (1.0 - t) * (1.0 - t);
    let across = t * t;
    let center = pos2(lerp(from.x, to.x, across), lerp(from.y, to.y, up));
    Pose { center, scale: lerp(1.0, SMALLEST, t), alpha: 1.0 - smoothstep(FADE_FROM, 1.0, t) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: Rect = Rect::from_min_max(pos2(200.0, 100.0), pos2(260.0, 172.0));
    const PAGE: Rect = Rect::from_min_max(pos2(16.0, 100.0), pos2(356.0, 260.0));

    fn run(liftoff: &mut Liftoff, seconds: f32) {
        for _ in 0..(seconds * 120.0) as usize {
            liftoff.step(1.0 / 120.0);
        }
    }

    #[test]
    fn sent_tile_rises_on_arc() {
        let mut liftoff = Liftoff::default();
        liftoff.launch(&[ItemId(1)]);
        let start = liftoff.pose(ItemId(1), TILE, PAGE).unwrap();
        assert_eq!((start.center, start.scale, start.alpha), (TILE.center(), 1.0, 1.0));
        run(&mut liftoff, 0.12);
        let early = liftoff.pose(ItemId(1), TILE, PAGE).unwrap();
        let rose = TILE.center().y - early.center.y;
        let drifted = TILE.center().x - early.center.x;
        assert!(rose > 0.0 && drifted > 0.0 && rose > drifted, "rises before it drifts: {rose} {drifted}");
        run(&mut liftoff, 0.6);
        let gone = liftoff.pose(ItemId(1), TILE, PAGE).unwrap();
        assert_eq!(gone.alpha, 0.0);
        assert!(liftoff.is_animating());
        assert_eq!(liftoff.pose(ItemId(2), TILE, PAGE), None);
    }

    #[test]
    fn tile_settles_back() {
        let mut liftoff = Liftoff::default();
        liftoff.launch(&[ItemId(1)]);
        run(&mut liftoff, 0.2);
        liftoff.rest();
        run(&mut liftoff, 0.05);
        let returning = liftoff.pose(ItemId(1), TILE, PAGE).unwrap();
        assert_eq!(returning.center, TILE.center());
        assert!(returning.alpha < 1.0 && returning.scale < 1.0);
        run(&mut liftoff, 2.0);
        assert!(!liftoff.is_animating());
        assert_eq!(liftoff.pose(ItemId(1), TILE, PAGE), None);
    }

    #[test]
    fn tile_returns_on_its_own() {
        let mut liftoff = Liftoff::default();
        liftoff.launch(&[ItemId(3)]);
        run(&mut liftoff, 4.0);
        assert!(!liftoff.is_animating());
    }

    #[test]
    fn pose_follows_flight() {
        let pose = Pose { center: pos2(50.0, 20.0), scale: 0.5, alpha: 1.0 };
        let moved = pose.transform(TILE) * TILE.center();
        assert!((moved - pose.center).length() < 1e-4);
        let corner = pose.transform(TILE) * TILE.min;
        assert!((corner - (pose.center - TILE.size() * 0.25)).length() < 1e-4);
    }
}
