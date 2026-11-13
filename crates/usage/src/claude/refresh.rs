//! One refresh: the desktop history if it's fresh, otherwise the API at most every 5 minutes (30 seconds when asked by
//! hand), falling back to older history when the API fails.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use super::credentials::Credentials;
use super::desktop::{self, History};
use super::state::{Hold, Outcome, Shared};
use super::{USAGE_STALE_AFTER, api, lock};

const API_EVERY: Duration = Duration::from_secs(5 * 60);
const API_MANUAL_GAP: Duration = Duration::from_secs(30);
const APP_FILE_UNREADABLE: &str = "Couldn't read usage from the Claude app";

pub(super) fn refresh(shared: &Mutex<Shared>, manual: bool) {
    let mut job = Job::take(&mut lock(shared), manual, Instant::now());
    let outcome = job.run(SystemTime::now());
    job.finish(&mut lock(shared), outcome);
}

struct Job {
    manual: bool,
    started: Instant,
    hold: Hold,
    last_api: Option<Instant>,
    history: Option<desktop::Cache>,
    credentials: Option<Credentials>,
    api_called: bool,
    api_setback: Option<Outcome>,
}

impl Job {
    fn take(shared: &mut Shared, manual: bool, started: Instant) -> Self {
        Self {
            manual,
            started,
            hold: shared.hold,
            last_api: shared.last_api,
            history: shared.history.take(),
            credentials: shared.cached.take(),
            api_called: false,
            api_setback: None,
        }
    }

    fn run(&mut self, now: SystemTime) -> Outcome {
        match desktop::load(&mut self.history) {
            History::Found(file) if is_fresh(file.at, now) => Outcome::Ready(file.into()),
            history if self.api_allowed() && self.api_due() => self.ask_api(history),
            History::Found(file) => Outcome::Stale(file.into()),
            History::Unreadable | History::Missing => Outcome::Unchanged,
        }
    }

    fn api_allowed(&self) -> bool {
        self.manual
            || match self.hold {
                Hold::None => true,
                Hold::Until(at) => self.started >= at,
                Hold::UntilAsked => false,
            }
    }

    fn api_due(&self) -> bool {
        let gap = if self.manual { API_MANUAL_GAP } else { API_EVERY };
        self.last_api.is_none_or(|at| self.started.duration_since(at) >= gap)
    }

    fn ask_api(&mut self, history: History) -> Outcome {
        self.api_called = true;
        let (api, credentials) = api::fetch(self.credentials.take());
        self.credentials = credentials;
        settle(api, history, &mut self.api_setback)
    }

    fn finish(self, shared: &mut Shared, outcome: Outcome) {
        shared.history = self.history;
        shared.cached = self.credentials;
        if self.api_called {
            shared.last_api = Some(self.started);
        }
        if let Some(api) = &self.api_setback {
            shared.note_hold(api, self.started);
        }
        shared.apply(outcome, self.started);
    }
}

fn settle(api: Outcome, history: History, setback: &mut Option<Outcome>) -> Outcome {
    match (api, history) {
        (Outcome::Ready(usage), _) => Outcome::Ready(usage),
        (api, History::Found(file)) => {
            *setback = Some(api);
            Outcome::Stale(file.into())
        }
        // The app's file is there but unreadable, so this isn't a login problem.
        (Outcome::Expired | Outcome::SignedOut, History::Unreadable) => Outcome::failed(APP_FILE_UNREADABLE),
        (api, History::Unreadable | History::Missing) => api,
    }
}

fn is_fresh(at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(at).map_or(true, |age| age <= USAGE_STALE_AFTER)
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn job(manual: bool, hold: Hold, api_ago: Option<Duration>, started: Instant) -> Job {
        let shared = &mut Shared { hold, last_api: api_ago.map(|ago| started - ago), ..Shared::default() };
        Job::take(shared, manual, started)
    }

    #[test]
    fn freshness() {
        let now = at(10_000);
        assert!(is_fresh(at(10_000), now));
        assert!(is_fresh(at(10_000 - 19 * 60), now));
        assert!(!is_fresh(at(10_000 - 21 * 60), now));
        assert!(is_fresh(at(10_060), now), "clock skew counts as fresh");
    }

    #[test]
    fn api_is_asked_every_five_minutes() {
        let t = Instant::now() + Duration::from_secs(3600);
        assert!(job(false, Hold::None, None, t).api_due());
        assert!(!job(false, Hold::None, Some(API_EVERY - Duration::from_secs(1)), t).api_due());
        assert!(job(false, Hold::None, Some(API_EVERY), t).api_due());
    }

    #[test]
    fn manual_asks_sooner_not_in_bursts() {
        let t = Instant::now() + Duration::from_secs(3600);
        assert!(job(true, Hold::None, Some(API_MANUAL_GAP), t).api_due());
        assert!(!job(true, Hold::None, Some(API_MANUAL_GAP - Duration::from_secs(1)), t).api_due());
    }

    #[test]
    fn holds_pause_automatic_calls_only() {
        let t = Instant::now() + Duration::from_secs(3600);
        let later = Hold::Until(t + Duration::from_secs(1));
        assert!(!job(false, later, None, t).api_allowed(), "a rate limit is respected");
        assert!(job(false, Hold::Until(t), None, t).api_allowed(), "and lifts on time");
        assert!(!job(false, Hold::UntilAsked, None, t).api_allowed(), "a Keychain denial waits for the user");
        assert!(job(true, later, None, t).api_allowed());
        assert!(job(true, Hold::UntilAsked, None, t).api_allowed());
    }

    #[test]
    fn unreadable_file_never_means_expired() {
        let mut setback = None;
        assert_eq!(settle(Outcome::Expired, History::Unreadable, &mut setback), Outcome::failed(APP_FILE_UNREADABLE));
        assert_eq!(settle(Outcome::SignedOut, History::Unreadable, &mut setback), Outcome::failed(APP_FILE_UNREADABLE));
        assert_eq!(settle(Outcome::Expired, History::Missing, &mut setback), Outcome::Expired);
        assert_eq!(setback, None);
    }

    #[test]
    fn finish_records_api_call_and_setback() {
        let t = Instant::now();
        let mut shared = Shared::default();
        let mut job = Job::take(&mut shared, false, t);
        job.api_called = true;
        job.api_setback = Some(Outcome::Failed { message: String::new(), retry_after: Some(Duration::from_secs(90)) });
        job.finish(&mut shared, Outcome::Unchanged);
        assert_eq!(shared.last_api, Some(t));
        assert_eq!(shared.hold, Hold::Until(t + Duration::from_secs(90)));
        assert_eq!((shared.usage, shared.version), (crate::PlanUsage::Loading, 0));
    }
}
