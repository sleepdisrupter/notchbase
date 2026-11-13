use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use crate::find::shared::Order;

use super::super::lanes::{Lanes, Listed, Next, PENDING_MOST, Queued};
use super::super::tally::LIVE_DIRS;
use super::super::{Job, Pace};
use super::{HOME, deep, plan};

fn lanes() -> (Lanes, Receiver<Order>) {
    let (sink, inbox) = mpsc::channel();
    let lanes = Lanes::new(sink);
    lanes.follow(Arc::new(plan(true)), 0);
    (lanes, inbox)
}

fn live(dir: &str, ticket: u32) -> Job {
    Job { live: true, ..deep(dir, ticket) }
}

fn at_home(below: &str, ticket: u32) -> Job {
    deep(&format!("{HOME}/{below}"), ticket)
}

fn taken(lanes: &Lanes, worker: usize, epoch: u64) -> Option<Job> {
    match lanes.next(worker, epoch, false) {
        Next::Job(taken) => Some(taken.job),
        Next::Idle | Next::Done => None,
    }
}

fn finished(inbox: &Receiver<Order>) -> Vec<u32> {
    inbox.try_iter().filter_map(|order| if let Order::Finished(ticket) = order { Some(ticket) } else { None }).collect()
}

fn children(jobs: Vec<Job>) -> Vec<Queued> {
    jobs.into_iter().map(|job| Queued { job, rank: 0, ahead: false }).collect()
}

fn settled(lanes: &Lanes, job: Job, below: Vec<Job>) {
    lanes.settle(vec![Listed { job, children: children(below) }], 0);
}

#[test]
fn nothing_scanned_before_a_plan() {
    let (sink, inbox) = mpsc::channel();
    let lanes = Lanes::new(sink);
    assert_eq!(lanes.submit(vec![at_home("a", 7), at_home("b", 7), at_home("c", 2)]), None);
    assert_eq!(finished(&inbox), [7, 2], "each ticket once, when its last job is dropped");
    assert!(!lanes.busy());
}

#[test]
fn live_lane_runs_while_paused() {
    let (lanes, inbox) = lanes();
    lanes.pace(Pace::Paused);
    let epoch = lanes.submit(vec![at_home("Music", 1), live(&format!("{HOME}/Movies"), 2)]).expect("a pool to start");
    assert_eq!(taken(&lanes, 1, epoch), None, "a paused scan keeps one worker, and only for live work");
    let job = taken(&lanes, 0, epoch).expect("the live job");
    assert!(job.live && job.ticket == 2);
    assert_eq!(taken(&lanes, 0, epoch), None, "the tier job waits for power");
    settled(&lanes, job, Vec::new());
    assert_eq!(finished(&inbox), [2], "the paused job has not counted down");
    lanes.pace(Pace::Full);
    assert_eq!(taken(&lanes, 3, epoch).map(|job| job.ticket), Some(1));
}

#[test]
fn gentle_two_workers_full_four() {
    let (lanes, _inbox) = lanes();
    lanes.pace(Pace::Gentle);
    let epoch = lanes.submit((0..4).map(|ticket| at_home("Music", ticket)).collect()).expect("a pool to start");
    assert!(taken(&lanes, 2, epoch).is_none() && taken(&lanes, 3, epoch).is_none());
    assert!(taken(&lanes, 1, epoch).is_some() && taken(&lanes, 0, epoch).is_some());
    lanes.pace(Pace::Full);
    assert!(taken(&lanes, 3, epoch).is_some() && taken(&lanes, 2, epoch).is_some());
}

#[test]
fn tiers_wait_for_the_writer() {
    let (lanes, _inbox) = lanes();
    let epoch = lanes.submit(vec![at_home("Music", 1)]).expect("a pool to start");
    let first = taken(&lanes, 0, epoch).expect("the first job");
    lanes.settle(vec![Listed { job: first, children: children(vec![at_home("Music/a", 1)]) }], PENDING_MOST + 1);
    lanes.submit(vec![live(&format!("{HOME}/Movies"), 2)]);
    assert_eq!(taken(&lanes, 1, epoch).map(|job| job.ticket), Some(2), "live work ignores the backlog");
    assert_eq!(taken(&lanes, 0, epoch), None, "the writer has more than enough rows to apply");
    lanes.landed(1);
    assert_eq!(taken(&lanes, 0, epoch).map(|job| job.ticket), Some(1));
}

#[test]
fn lower_tiers_run_first() {
    let (sink, _inbox) = mpsc::channel();
    let lanes = Lanes::new(sink);
    let home = Path::new("/Users/me");
    let roots = [super::boot("/")];
    let fences = super::super::Fences { apps: vec!["/Applications".into()], ..Default::default() };
    let plan = super::super::Plan::fenced(&Default::default(), &roots, Some(home), None, true, fences);
    lanes.follow(Arc::new(plan), 0);
    let order = [
        "/Library/Fonts",
        "/Users/me/Library/Caches",
        "/Users/Shared",
        "/Applications",
        "/Users/me/.cargo",
        "/Users/me/Music",
    ];
    let jobs = order.iter().zip(0..).map(|(dir, ticket)| deep(dir, ticket)).collect();
    let epoch = lanes.submit(jobs).expect("a pool to start");
    let ran: Vec<(String, u8)> = std::iter::from_fn(|| taken(&lanes, 0, epoch))
        .map(|job| (job.dir.to_string_lossy().into_owned(), job.tier))
        .collect();
    let expected = [
        ("/Applications", 0),
        ("/Users/me/Music", 0),
        ("/Users/Shared", 0),
        ("/Library/Fonts", 1),
        ("/Users/me/.cargo", 2),
        ("/Users/me/Library/Caches", 3),
    ];
    assert_eq!(ran, expected.map(|(dir, tier)| (dir.to_owned(), tier)), "and the scanner works out each tier itself");
}

#[test]
fn spent_live_errand_moves_to_tiers() {
    let (lanes, inbox) = lanes();
    lanes.pace(Pace::Paused);
    let epoch = lanes.submit(vec![live(&format!("{HOME}/big"), 9)]).expect("a pool to start");
    for listed in 1..=LIVE_DIRS {
        let job = taken(&lanes, 0, epoch).unwrap_or_else(|| panic!("folder {listed} is still live"));
        let child = Job { dir: job.dir.join("x"), ..job.clone() };
        let waiting = Job { dir: job.dir.join("y"), ..job.clone() };
        let below = if listed == 1 { vec![child, waiting] } else { vec![child] };
        settled(&lanes, job, below);
    }
    assert_eq!(taken(&lanes, 0, epoch), None, "even a live job queued earlier is demoted once the share is spent");
    assert!(lanes.busy() && finished(&inbox).is_empty());
    lanes.pace(Pace::Full);
    let rest: Vec<Job> = std::iter::from_fn(|| taken(&lanes, 0, epoch)).collect();
    assert_eq!(rest.len(), 2);
    assert!(rest.iter().all(|job| !job.live && job.ticket == 9), "the rest runs in the tier lanes, same ticket");
}

#[test]
fn disallowed_job_dropped_when_taken() {
    let (lanes, inbox) = lanes();
    let jobs = vec![at_home("Library/Caches/app", 1), at_home("Projects/app/node_modules", 2), at_home("Documents", 3)];
    let epoch = lanes.submit(jobs).expect("a pool to start");
    lanes.follow(Arc::new(plan(false)), 1);
    assert_eq!(taken(&lanes, 0, epoch).map(|job| job.ticket), Some(2), "level one only holds caches back");
    assert_eq!(taken(&lanes, 0, epoch), None);
    let mut done = finished(&inbox);
    done.sort_unstable();
    assert_eq!(done, [1, 3], "a cache at level one and a guarded folder after access was lost");
}

#[test]
fn pool_ends_when_nothing_is_left() {
    let (lanes, inbox) = lanes();
    let first = lanes.submit(vec![at_home("Music", 1)]).expect("a pool to start");
    assert_eq!(lanes.submit(vec![at_home("Movies", 2)]), None, "the running pool takes it");
    let jobs: Vec<Job> = std::iter::from_fn(|| taken(&lanes, 0, first)).collect();
    assert!(lanes.busy(), "two folders are being listed");
    jobs.into_iter().for_each(|job| settled(&lanes, job, Vec::new()));
    assert!(matches!(lanes.next(0, first, true), Next::Done));
    assert!(!lanes.busy());
    assert_eq!(finished(&inbox), [1, 2]);
    let second = lanes.submit(vec![at_home("Pictures", 3)]).expect("a new pool");
    assert_ne!(first, second);
    assert!(matches!(lanes.next(1, first, true), Next::Done), "a worker of the old pool steps aside");
    assert_eq!(taken(&lanes, 0, second).map(|job| job.ticket), Some(3));
}
