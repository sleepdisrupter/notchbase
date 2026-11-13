use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::poll::{Worker, lock};

use super::eventually;

const LONG: Duration = Duration::from_secs(60);
const SHORT: Duration = Duration::from_millis(15);
const NAP: Duration = Duration::from_millis(5);

#[test]
fn work_runs_on_the_named_thread() {
    let worker = Worker::start("notchbase-test-worker", NAP, Mutex::new(None), |shift| {
        *lock(shift.state()) = std::thread::current().name().map(str::to_owned);
    });
    eventually("the work", || lock(worker.state()).is_some());
    assert_eq!(lock(worker.state()).as_deref(), Some("notchbase-test-worker"));
}

#[test]
fn on_duty_until_worker_is_dropped() {
    let (done, finished) = mpsc::channel();
    let worker = Worker::start("notchbase-test-duty", NAP, AtomicBool::new(false), move |shift| {
        shift.state().store(shift.on_duty(), Ordering::SeqCst);
        while shift.on_duty() {
            std::thread::sleep(NAP);
        }
        let _ = done.send(());
    });
    eventually("the shift starting", || worker.state().load(Ordering::SeqCst));
    drop(worker);
    assert!(finished.recv_timeout(super::PATIENCE).is_ok(), "the shift ends once the worker is gone");
}

#[test]
fn nap_lasts_until_its_deadline() {
    let (done, slept) = mpsc::channel();
    let _worker = Worker::start("notchbase-test-nap", NAP, (), move |shift| {
        let began = Instant::now();
        shift.nap_until(began + SHORT, |()| false);
        let _ = done.send(began.elapsed());
    });
    let slept = slept.recv_timeout(super::PATIENCE).unwrap();
    assert!(slept >= SHORT, "woke after {slept:?}");
}

#[test]
fn nap_never_oversleeps_its_deadline() {
    let (done, slept) = mpsc::channel();
    let _worker = Worker::start("notchbase-test-step", LONG, (), move |shift| {
        let began = Instant::now();
        shift.nap_until(began + SHORT, |()| false);
        let _ = done.send(began.elapsed());
    });
    let slept = slept.recv_timeout(super::PATIENCE).unwrap();
    assert!(slept >= SHORT && slept < LONG, "woke after {slept:?}");
}

#[test]
fn passed_deadline_ends_nap_at_once() {
    let (done, slept) = mpsc::channel();
    let _worker = Worker::start("notchbase-test-past", LONG, (), move |shift| {
        let began = Instant::now();
        shift.nap_until(began, |()| false);
        let _ = done.send(began.elapsed());
    });
    assert!(slept.recv_timeout(super::PATIENCE).unwrap() < LONG);
}

#[test]
fn nap_ends_early_on_interruption() {
    let (done, woke) = mpsc::channel();
    let worker = Worker::start("notchbase-test-poke", NAP, AtomicBool::new(false), move |shift| {
        shift.nap_until(Instant::now() + LONG, |poked| poked.load(Ordering::SeqCst));
        let _ = done.send(shift.on_duty());
    });
    worker.state().store(true, Ordering::SeqCst);
    assert_eq!(woke.recv_timeout(super::PATIENCE), Ok(true), "woken by the poke while still on duty");
}

#[test]
fn nap_ends_early_when_dropped() {
    let (done, woke) = mpsc::channel();
    let worker = Worker::start("notchbase-test-drop", NAP, (), move |shift| {
        shift.nap_until(Instant::now() + LONG, |()| false);
        let _ = done.send(shift.on_duty());
    });
    drop(worker);
    assert_eq!(woke.recv_timeout(super::PATIENCE), Ok(false));
}

#[test]
fn shift_keeps_state_after_drop() {
    let kept = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&kept);
    let (done, finished) = mpsc::channel();
    let worker = Worker::start("notchbase-test-state", NAP, Mutex::new(vec![1]), move |shift| {
        while shift.on_duty() {
            std::thread::sleep(NAP);
        }
        lock(shift.state()).push(2);
        seen.store(lock(shift.state()).len() == 2, Ordering::SeqCst);
        let _ = done.send(());
    });
    drop(worker);
    finished.recv_timeout(super::PATIENCE).unwrap();
    assert!(kept.load(Ordering::SeqCst));
}
