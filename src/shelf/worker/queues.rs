//! The worker's queues. Operations go first, then file info, then thumbnails.

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::shelf::ops::Source;
use crate::shelf::{ItemId, Kind, Op};

pub(super) struct Lookup {
    pub(super) id: ItemId,
    pub(super) path: PathBuf,
    pub(super) kind: Kind,
}

pub(super) enum Job {
    Probe(Lookup),
    Op(Op, Vec<Source>),
}

pub(super) enum Work {
    Op(Op, Vec<Source>),
    Info(Lookup),
    Thumb(Lookup),
}

#[derive(Default)]
pub(super) struct Queues {
    ops: VecDeque<(Op, Vec<Source>)>,
    infos: VecDeque<Lookup>,
    thumbs: VecDeque<Lookup>,
}

impl Queues {
    pub(super) fn push(&mut self, job: Job) {
        match job {
            Job::Probe(task) => self.infos.push_back(task),
            Job::Op(op, sources) => self.ops.push_back((op, sources)),
        }
    }

    pub(super) fn push_thumb(&mut self, task: Lookup) {
        self.thumbs.push_back(task);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.infos.is_empty() && self.thumbs.is_empty()
    }

    pub(super) fn next_by_priority(&mut self) -> Option<Work> {
        if let Some((op, sources)) = self.ops.pop_front() {
            return Some(Work::Op(op, sources));
        }
        if let Some(task) = self.infos.pop_front() {
            return Some(Work::Info(task));
        }
        self.thumbs.pop_front().map(Work::Thumb)
    }
}
