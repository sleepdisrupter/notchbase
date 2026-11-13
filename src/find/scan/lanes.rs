//! The queue of folders waiting to be listed. The live lane serves errands someone is waiting on and keeps going
//! while the scan is paused. The tier lanes hold the background scan, user files first and caches last.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::find::shared::{Order, lock};

use super::plan::RANKS;
use super::tally::Tally;
use super::{Job, Pace, Plan};

pub(super) const TIERS: usize = 4;
// Rows the writer has yet to apply. Past this the tier lanes wait, so a fast disk cannot outrun it.
pub(super) const PENDING_MOST: usize = 262_144;
const FIRST_LANE: usize = 0;

pub(super) struct Lanes {
    state: Mutex<State>,
    changed: Condvar,
}

struct State {
    live: VecDeque<Queued>,
    tiers: [[VecDeque<Job>; RANKS]; TIERS],
    plan: Option<Arc<Plan>>,
    level: u8,
    pace: Pace,
    running: usize,
    pending: usize,
    epoch: u64,
    alive: bool,
    tally: Tally,
}

pub(super) struct Taken {
    pub job: Job,
    pub plan: Arc<Plan>,
    pub level: u8,
    pub pace: Pace,
}

pub(super) enum Next {
    Job(Box<Taken>),
    Idle,
    Done,
}

pub(super) struct Queued {
    pub job: Job,
    pub rank: usize,
    pub ahead: bool,
}

pub(super) struct Listed {
    pub job: Job,
    pub children: Vec<Queued>,
}

impl Lanes {
    pub(super) fn new(sink: Sender<Order>) -> Self {
        let state = State {
            live: VecDeque::new(),
            tiers: Default::default(),
            plan: None,
            level: 0,
            pace: Pace::Full,
            running: 0,
            pending: 0,
            epoch: 0,
            alive: false,
            tally: Tally::new(sink),
        };
        Self { state: Mutex::new(state), changed: Condvar::new() }
    }

    pub(super) fn submit(&self, jobs: Vec<Job>) -> Option<u64> {
        let plan = {
            let mut state = lock(&self.state);
            jobs.iter().for_each(|job| state.tally.opened(job.ticket, 1));
            state.plan.clone()
        };
        let admitted: Vec<Result<Queued, u32>> = jobs.into_iter().map(|job| admit(plan.as_deref(), job)).collect();
        let mut state = lock(&self.state);
        for job in admitted {
            match job {
                Ok(queued) => state.push(queued),
                Err(ticket) => state.tally.closed(ticket),
            }
        }
        self.changed.notify_all();
        let start = !state.alive && state.queued() > 0;
        if start {
            state.alive = true;
            state.epoch += 1;
        }
        start.then_some(state.epoch)
    }

    pub(super) fn next(&self, worker: usize, epoch: u64, wait: bool) -> Next {
        loop {
            let taken = match self.take(worker, epoch, wait) {
                Next::Job(taken) => taken,
                rest => return rest,
            };
            let Taken { job, plan, level, .. } = &*taken;
            if plan.wanted(&job.dir) && !plan.held_back(*level, &job.dir, job.tier) {
                return Next::Job(taken);
            }
            let mut state = lock(&self.state);
            state.running = state.running.saturating_sub(1);
            state.tally.closed(job.ticket);
            self.changed.notify_all();
        }
    }

    pub(super) fn settle(&self, listed: Vec<Listed>, rows: usize) {
        let mut state = lock(&self.state);
        state.pending += rows;
        for Listed { job, children } in listed {
            if job.live {
                state.tally.listed_live(job.ticket);
            }
            let live = job.live && !state.tally.spent(job.ticket);
            state.tally.opened(job.ticket, children.len());
            children.into_iter().for_each(|child| state.push(Queued { job: Job { live, ..child.job }, ..child }));
            state.tally.closed(job.ticket);
            state.running = state.running.saturating_sub(1);
        }
        self.changed.notify_all();
    }

    pub(super) fn landed(&self, rows: usize) {
        let mut state = lock(&self.state);
        state.pending = state.pending.saturating_sub(rows);
        self.changed.notify_all();
    }

    pub(super) fn follow(&self, plan: Arc<Plan>, level: u8) {
        let mut state = lock(&self.state);
        (state.plan, state.level) = (Some(plan), level);
        self.changed.notify_all();
    }

    pub(super) fn pace(&self, pace: Pace) {
        lock(&self.state).pace = pace;
        self.changed.notify_all();
    }

    pub(super) fn ended(&self, epoch: u64) {
        let mut state = lock(&self.state);
        if state.epoch == epoch {
            state.alive = false;
        }
    }

    #[cfg(test)]
    pub(super) fn busy(&self) -> bool {
        let state = lock(&self.state);
        state.queued() > 0 || state.running > 0
    }

    pub(super) fn backlog(&self) -> usize {
        lock(&self.state).queued()
    }

    fn take(&self, worker: usize, epoch: u64, wait: bool) -> Next {
        let mut state = lock(&self.state);
        loop {
            if state.epoch != epoch {
                return Next::Done;
            }
            if let Some(taken) = state.pop(worker) {
                return Next::Job(Box::new(taken));
            }
            if !wait {
                return Next::Idle;
            }
            if state.queued() == 0 && state.running == 0 {
                state.alive = false;
                self.changed.notify_all();
                return Next::Done;
            }
            state = self.wait(state);
        }
    }

    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn admit(plan: Option<&Plan>, job: Job) -> Result<Queued, u32> {
    match plan {
        Some(plan) if plan.wanted(&job.dir) => {
            let (rank, ahead) = (plan.rank(&job.dir), plan.ahead(&job.dir));
            Ok(Queued { rank, ahead, job: Job { tier: plan.inside(&job.dir), ..job } })
        }
        _ => Err(job.ticket),
    }
}

impl State {
    fn push(&mut self, queued: Queued) {
        if queued.job.live {
            self.live.push_back(queued);
        } else {
            let lane = if queued.ahead { FIRST_LANE } else { usize::from(queued.job.tier).min(TIERS - 1) };
            self.tiers[lane][queued.rank.min(RANKS - 1)].push_back(queued.job);
        }
    }

    fn pop(&mut self, worker: usize) -> Option<Taken> {
        let plan = self.plan.clone()?;
        let job = self.pop_live(worker).or_else(|| self.pop_tier(worker))?;
        self.running += 1;
        Some(Taken { job, plan, level: self.level, pace: self.pace })
    }

    fn pop_live(&mut self, worker: usize) -> Option<Job> {
        if worker >= self.pace.workers().max(1) {
            return None;
        }
        while let Some(queued) = self.live.pop_front() {
            if !self.tally.spent(queued.job.ticket) {
                return Some(queued.job);
            }
            self.push(Queued { job: Job { live: false, ..queued.job }, ..queued });
        }
        None
    }

    fn pop_tier(&mut self, worker: usize) -> Option<Job> {
        if worker >= self.pace.workers() || self.pending > PENDING_MOST {
            return None;
        }
        self.tiers.iter_mut().flatten().find_map(VecDeque::pop_front)
    }

    fn queued(&self) -> usize {
        self.live.len() + self.tiers.iter().flatten().map(VecDeque::len).sum::<usize>()
    }
}
