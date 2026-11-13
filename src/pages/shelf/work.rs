//! Progress for each tile while the shelf works on its files, one after another or all together.

use std::collections::HashMap;

use crate::shelf::{Busy, ItemId, Op};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION};

const PROGRESS: SpringConfig = SpringConfig::new(0.35, 1.0);
const DONE_FOR: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    Waiting,
    Active,
    Complete,
    Done { age: f32, ok: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TileWork {
    pub(super) progress: f32,
    pub(super) phase: Phase,
}

struct Finished {
    items: Vec<ItemId>,
    age: f32,
    ok: bool,
}

#[derive(Default)]
pub(super) struct Work {
    busy: Option<Busy>,
    progress: HashMap<ItemId, Spring>,
    finished: Option<Finished>,
}

impl Work {
    pub(super) fn update(&mut self, busy: Option<Busy>, succeeded: bool, dt: f32) {
        if let (Some(was), None) = (&self.busy, &busy) {
            let items = was.items.clone();
            if succeeded {
                for spring in self.progress.values_mut() {
                    spring.animate_to(1.0, PROGRESS);
                }
            }
            self.finished = Some(Finished { items, age: 0.0, ok: succeeded });
        }
        if let Some(busy) = &busy {
            if self.busy.as_ref().is_none_or(|was| was.items != busy.items) {
                self.progress.clear();
                self.finished = None;
            }
            for (index, id) in busy.items.iter().enumerate() {
                let target = local_progress(busy, index);
                let spring = self.progress.entry(*id).or_insert_with(|| Spring::new(0.0, PROGRESS, UNIT_PRECISION));
                if spring.target != target {
                    spring.animate_to(target, PROGRESS);
                }
            }
        }
        self.busy = busy;
        for spring in self.progress.values_mut() {
            spring.step(dt);
        }
        if let Some(finished) = &mut self.finished {
            finished.age += dt;
            if finished.age >= DONE_FOR {
                self.finished = None;
                self.progress.clear();
            }
        }
    }

    pub(super) fn tile(&self, id: ItemId) -> Option<TileWork> {
        let progress = self.progress.get(&id).map_or(0.0, |spring| spring.value.clamp(0.0, 1.0));
        if let Some(busy) = &self.busy {
            let index = busy.items.iter().position(|item| *item == id)?;
            let local = local_progress(busy, index);
            let phase = if per_file(busy.op) && local >= 1.0 {
                Phase::Complete
            } else if local > 0.0 || index == current(busy) {
                Phase::Active
            } else {
                Phase::Waiting
            };
            return Some(TileWork { progress, phase });
        }
        let finished = self.finished.as_ref().filter(|finished| finished.items.contains(&id))?;
        Some(TileWork { progress, phase: Phase::Done { age: finished.age, ok: finished.ok } })
    }

    pub(super) fn is_animating(&self) -> bool {
        self.busy.is_some() || self.finished.is_some() || !self.progress.values().all(Spring::is_settled)
    }
}

fn per_file(op: Op) -> bool {
    !matches!(op, Op::Combine | Op::Split)
}

fn current(busy: &Busy) -> usize {
    let count = busy.items.len().max(1);
    if per_file(busy.op) { ((busy.progress * count as f32) as usize).min(count - 1) } else { 0 }
}

fn local_progress(busy: &Busy, index: usize) -> f32 {
    if per_file(busy.op) {
        (busy.progress * busy.items.len() as f32 - index as f32).clamp(0.0, 1.0)
    } else {
        busy.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shelf::Format;

    fn busy(op: Op, progress: f32, count: u64) -> Busy {
        Busy { op, progress, items: (1..=count).map(ItemId).collect() }
    }

    fn settle(work: &mut Work, busy: Option<Busy>, succeeded: bool) {
        for _ in 0..240 {
            work.update(busy.clone(), succeeded, 1.0 / 120.0);
        }
    }

    #[test]
    fn files_worked_in_turn() {
        let mut work = Work::default();
        settle(&mut work, Some(busy(Op::Convert(Format::Png), 0.5, 4)), true);
        let phases: Vec<Phase> = (1..=4).map(|id| work.tile(ItemId(id)).unwrap().phase).collect();
        assert_eq!(phases, [Phase::Complete, Phase::Complete, Phase::Active, Phase::Waiting]);
        assert!((work.tile(ItemId(2)).unwrap().progress - 1.0).abs() < 0.01);
        assert!(work.tile(ItemId(3)).unwrap().progress < 0.01);
        assert_eq!(work.tile(ItemId(9)), None);
    }

    #[test]
    fn combining_moves_all_together() {
        let mut work = Work::default();
        settle(&mut work, Some(busy(Op::Combine, 0.4, 3)), true);
        for id in 1..=3 {
            let tile = work.tile(ItemId(id)).unwrap();
            assert_eq!(tile.phase, Phase::Active);
            assert!((tile.progress - 0.4).abs() < 0.01);
        }
    }

    #[test]
    fn finishing_flashes_then_clears() {
        let mut work = Work::default();
        settle(&mut work, Some(busy(Op::Compress, 0.2, 1)), true);
        work.update(None, true, 0.0);
        assert!(matches!(work.tile(ItemId(1)).unwrap().phase, Phase::Done { ok: true, .. }));
        settle(&mut work, None, true);
        assert_eq!(work.tile(ItemId(1)), None);
        assert!(!work.is_animating());
    }
}
