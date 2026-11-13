//! Fresh permission answers. macOS keeps some answers for the life of a process, so while the permissions page is open
//! a fresh copy of the app is asked every second and prints what it sees.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{Access, Standing, standing};

pub const FLAG: &str = "--permission-standings";
const THREAD: &str = "notchbase-permission-probe";
const EVERY: Duration = Duration::from_secs(1);
const FRESH_FOR: Duration = Duration::from_secs(3);
const GRANTED: &str = "granted";
const DENIED: &str = "denied";
const UNDECIDED: &str = "undecided";

pub type Standings = HashMap<Access, Standing>;

pub fn is_command(first: Option<&OsStr>) -> bool {
    first.is_some_and(|first| first == FLAG)
}

pub fn run() -> i32 {
    print!("{}", report(standing));
    0
}

fn report(look: impl Fn(Access) -> Option<Standing>) -> String {
    let lines =
        Access::ALL.into_iter().filter_map(|access| Some(format!("{}={}\n", access.key(), word(look(access)?))));
    lines.collect()
}

fn word(standing: Standing) -> &'static str {
    match standing {
        Standing::Granted => GRANTED,
        Standing::Denied => DENIED,
        Standing::Undecided => UNDECIDED,
    }
}

fn parse(text: &str) -> Standings {
    let entry = |line: &str| {
        let (key, value) = line.trim().split_once('=')?;
        let access = Access::ALL.into_iter().find(|access| access.key() == key)?;
        let standing = match value {
            GRANTED => Standing::Granted,
            DENIED => Standing::Denied,
            UNDECIDED => Standing::Undecided,
            _ => return None,
        };
        Some((access, standing))
    };
    text.lines().filter_map(entry).collect()
}

#[derive(Default)]
struct Shared {
    wanted: AtomicBool,
    running: AtomicBool,
    latest: Mutex<Option<(Instant, Standings)>>,
}

#[derive(Default)]
pub struct Prober {
    shared: Arc<Shared>,
}

impl Prober {
    pub fn follow(&self, wanted: bool) {
        self.shared.wanted.store(wanted, Ordering::Relaxed);
        if !wanted || self.shared.running.swap(true, Ordering::Relaxed) {
            return;
        }
        let shared = self.shared.clone();
        if std::thread::Builder::new().name(THREAD.into()).spawn(move || ask_fresh(&shared)).is_err() {
            self.shared.running.store(false, Ordering::Relaxed);
        }
    }

    pub fn latest(&self, now: Instant) -> Option<Standings> {
        let latest = self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner);
        latest.as_ref().filter(|(at, _)| now.duration_since(*at) < FRESH_FOR).map(|(_, standings)| standings.clone())
    }
}

impl Drop for Prober {
    fn drop(&mut self) {
        self.shared.wanted.store(false, Ordering::Relaxed);
    }
}

fn ask_fresh(shared: &Shared) {
    if let Ok(me) = std::env::current_exe() {
        while shared.wanted.load(Ordering::Relaxed) {
            let answer = Command::new(&me).arg(FLAG).stdin(Stdio::null()).stderr(Stdio::null()).output();
            if let Ok(output) = answer
                && output.status.success()
            {
                let standings = parse(&String::from_utf8_lossy(&output.stdout));
                *shared.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), standings));
            }
            std::thread::sleep(EVERY);
        }
    }
    shared.running.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_output_round_trips() {
        let look = |access| match access {
            Access::Accessibility => Some(Standing::Granted),
            Access::Calendar => Some(Standing::Denied),
            Access::Automation => None,
            _ => Some(Standing::Undecided),
        };
        let printed = report(look);
        assert!(printed.starts_with("accessibility=granted\n"));
        assert!(!printed.contains("automation"), "a permission that doesn't apply now is left out");
        let read = parse(&printed);
        assert_eq!(read.len(), Access::ALL.len() - 1);
        assert!(Access::ALL.into_iter().all(|access| read.get(&access).copied() == look(access)));
    }

    #[test]
    fn stray_output_is_ignored() {
        let read = parse("dyld: noise\nbluetooth=granted\nunknown=granted\ncalendar=maybe\n\n");
        assert_eq!(read, Standings::from([(Access::Bluetooth, Standing::Granted)]));
    }

    #[test]
    fn only_its_own_flag_runs_the_helper() {
        assert!(is_command(Some(OsStr::new(FLAG))));
        assert!(!is_command(Some(OsStr::new("--install"))));
        assert!(!is_command(None));
    }
}
