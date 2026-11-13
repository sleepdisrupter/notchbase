//! Updates from notchbase's GitHub releases. A thread checks for a newer release three seconds after start and every
//! three minutes after that, and downloads one when asked.

mod install;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use notchbase_release::{Release, Server};

use crate::poll::Wake;

pub use install::install;
pub use notchbase_release::Version;

const USER_AGENT: &str = notchbase_net::user_agent!();

/// The first check comes this long after starting.
const FIRST_CHECK: Duration = Duration::from_secs(3);
/// `check_soon` checks after this short wait.
const SOON: Duration = Duration::from_secs(2);
/// Checks repeat this often.
const EVERY: Duration = Duration::from_secs(3 * 60);
/// A failed check or download is tried again after this.
const RETRY: Duration = Duration::from_secs(2 * 60);
/// Download progress reaches the app in steps of 1%, so it isn't woken for every chunk.
const PROGRESS_STEP: f32 = 0.01;
/// Downloads wait in this folder in the data folder, which is cleared at each start.
const STAGING: &str = "updates";
const UNNAMED_DOWNLOAD: &str = "notchbase-update";

/// The version of this build.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_default()
}

/// What the updater is doing, for the notch and the Settings row.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// There's nowhere to download to.
    Unavailable,
    Waiting,
    Current,
    Available(Version),
    Downloading(Version, f32),
    /// Downloaded and unpacked, ready for `install`.
    Ready(Version, PathBuf),
    Failed(String),
}

enum Order {
    Check,
    Fetch,
}

/// The app's handle on the update thread.
pub struct Updater {
    status: Arc<Mutex<Status>>,
    orders: Option<Sender<Order>>,
}

impl Updater {
    pub fn start(wake: Wake) -> Self {
        let server = Server::built_in(USER_AGENT);
        let Some(staging) = crate::platform::data_dir().map(|dir| dir.join(STAGING)) else {
            return Self { status: Arc::new(Mutex::new(Status::Unavailable)), orders: None };
        };
        let status = Arc::new(Mutex::new(Status::Waiting));
        let (orders, inbox) = mpsc::channel();
        let worker = Worker { server, staging, status: status.clone(), wake, found: None };
        let spawned = std::thread::Builder::new().name("notchbase-updates".into()).spawn(move || worker.run(inbox));
        Self { status, orders: spawned.is_ok().then_some(orders) }
    }

    pub fn status(&self) -> Status {
        self.lock().clone()
    }

    pub fn check_soon(&self) {
        self.send(Order::Check);
    }

    /// Downloads the update that was found, if there is one.
    pub fn fetch(&self) {
        let mut status = self.lock();
        if let Status::Available(version) = *status {
            *status = Status::Downloading(version, 0.0);
            drop(status);
            self.send(Order::Fetch);
        }
    }

    /// Shows a problem that came from outside the thread, like an install that failed.
    pub fn failed(&self, problem: String) {
        *self.lock() = Status::Failed(problem);
    }

    fn lock(&self) -> MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn send(&self, order: Order) {
        if let Some(orders) = &self.orders {
            let _ = orders.send(order);
        }
    }
}

struct Worker {
    server: Server,
    staging: PathBuf,
    status: Arc<Mutex<Status>>,
    wake: Wake,
    found: Option<(Version, Release)>,
}

impl Worker {
    fn run(mut self, inbox: Receiver<Order>) {
        crate::installer::remove(&self.staging);
        let mut next_check = Instant::now() + FIRST_CHECK;
        loop {
            match inbox.recv_timeout(next_check.saturating_duration_since(Instant::now())) {
                Ok(Order::Check) => next_check = Instant::now() + SOON,
                Ok(Order::Fetch) => {
                    if !self.fetch() {
                        next_check = Instant::now() + RETRY;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    next_check = Instant::now() + if self.check() { EVERY } else { RETRY };
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Asks for the newest release. A download already under way for it keeps its status.
    fn check(&mut self) -> bool {
        let found = match self.server.releases(&notchbase_release::platform()) {
            Ok(releases) => notchbase_release::newest(releases, current_version()),
            Err(problem) => {
                self.set(Status::Failed(problem));
                return false;
            }
        };
        match (&found, self.status()) {
            (Some((version, _)), Status::Ready(ready, _) | Status::Downloading(ready, _)) if *version == ready => {}
            (Some((version, _)), _) => self.set(Status::Available(*version)),
            (None, _) => self.set(Status::Current),
        }
        self.found = found;
        true
    }

    fn fetch(&mut self) -> bool {
        let Some((version, release)) = self.found.clone() else {
            self.set(Status::Waiting);
            return true;
        };
        match self.download(version, &release) {
            Ok(path) => {
                self.set(Status::Ready(version, path));
                true
            }
            Err(problem) => {
                self.set(Status::Failed(problem));
                false
            }
        }
    }

    fn download(&self, version: Version, release: &Release) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.staging).map_err(|err| err.to_string())?;
        let file = self.staging.join(download_name(&release.url));
        self.set(Status::Downloading(version, 0.0));
        let mut shown = 0.0;
        notchbase_release::download(release, &file, USER_AGENT, |progress| {
            if progress - shown >= PROGRESS_STEP || progress >= 1.0 {
                shown = progress;
                self.set(Status::Downloading(version, progress));
            }
        })?;
        install::prepare(&file, &self.staging.join(version.to_string())).map_err(|err| err.to_string())
    }

    fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Sets the status, and wakes the app only when it changed.
    fn set(&self, status: Status) {
        let mut current = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        if *current != status {
            *current = status;
            drop(current);
            (self.wake)();
        }
    }
}

fn download_name(url: &str) -> &str {
    url.rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or(UNNAMED_DOWNLOAD)
}

#[cfg(test)]
mod tests;
