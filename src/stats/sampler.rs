//! The notchbase-stats thread. It publishes the fixed facts at once, then takes a sample per request, at most every
//! 400 ms, so a burst of requests makes one sample.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::hardware::{Probe, Readings};
use super::{Wake, lock};

const FIRST_SAMPLE_WITHIN: Duration = Duration::from_secs(1);
/// CPU usage is the change between two readings, so samples are kept this far apart.
const MIN_SAMPLE_GAP: Duration = Duration::from_millis(400);

pub(super) struct Sampler {
    latest: Arc<Mutex<Option<Readings>>>,
    requests: Option<Sender<bool>>,
}

impl Sampler {
    pub(super) fn spawn(wake: Wake) -> Self {
        let latest = Arc::default();
        let published = Arc::clone(&latest);
        let (requests, received) = mpsc::channel();
        let requests = thread::Builder::new()
            .name("notchbase-stats".into())
            .spawn(move || run(&received, &published, &wake))
            .ok()
            .map(|_| requests);
        Self { latest, requests }
    }

    pub(super) fn request(&mut self, lists_apps: bool) {
        if self.requests.as_ref().is_some_and(|requests| requests.send(lists_apps).is_err()) {
            self.requests = None;
        }
    }

    pub(super) fn take(&self) -> Option<Readings> {
        lock(&self.latest).take()
    }
}

fn run(requests: &Receiver<bool>, latest: &Mutex<Option<Readings>>, wake: &Wake) {
    let publish = |readings: Readings| {
        *lock(latest) = Some(readings);
        wake();
    };
    let mut probe = Probe::open();
    publish(probe.statics());
    let mut earliest = Instant::now() + MIN_SAMPLE_GAP;
    let mut requested = first_request(requests);
    while let Some(lists_apps) = requested.and_then(|lists_apps| wait_until(requests, earliest, lists_apps)) {
        publish(probe.sample(lists_apps));
        earliest = Instant::now() + MIN_SAMPLE_GAP;
        requested = requests.recv().ok();
    }
}

fn first_request(requests: &Receiver<bool>) -> Option<bool> {
    match requests.recv_timeout(FIRST_SAMPLE_WITHIN) {
        Ok(lists_apps) => Some(lists_apps),
        Err(RecvTimeoutError::Timeout) => Some(false),
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

fn wait_until(requests: &Receiver<bool>, deadline: Instant, mut lists_apps: bool) -> Option<bool> {
    loop {
        match requests.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(latest) => lists_apps = latest,
            Err(RecvTimeoutError::Timeout) => return Some(lists_apps),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_spaced_for_cpu_deltas() {
        assert!(MIN_SAMPLE_GAP >= sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    }

    #[test]
    fn queued_requests_fold_into_one() {
        let (requests, received) = mpsc::channel();
        for lists_apps in [false, false, true] {
            requests.send(lists_apps).unwrap();
        }
        assert_eq!(wait_until(&received, Instant::now(), false), Some(true), "the newest request decides");
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn shutdown_ends_every_wait_at_once() {
        let (requests, received) = mpsc::channel::<bool>();
        drop(requests);
        let started = Instant::now();
        assert_eq!(wait_until(&received, started + Duration::from_secs(60), true), None);
        assert_eq!(first_request(&received), None);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
