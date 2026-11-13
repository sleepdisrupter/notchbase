//! Handing over from one copy of notchbase to another: starting the new one, which waits for the old one to
//! exit, and stopping any other copies that are running.

use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::location::{Target, aside, remove};

// A new copy is started with one of these and the old one's pid, so it knows to wait and what to say.
const UPDATED: &str = "--replaces";
const INSTALLED: &str = "--installed";
/// Copies started as `notchbase hook` are assistant hooks, not the app, so they're left running.
const HOOK: &str = "hook";
/// How long a new copy waits for the one it replaces to exit.
const WAIT_FOR_PREVIOUS: Duration = Duration::from_secs(10);
/// How long the new copy keeps trying to delete the old one, which may still hold its files for a moment.
const TIDY_FOR: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(50);

/// How this copy was started: fresh, by the updater, or by an install.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arrival {
    #[default]
    Fresh,
    Updated,
    Installed,
}

impl Arrival {
    fn flag(self) -> Option<&'static str> {
        match self {
            Self::Fresh => None,
            Self::Updated => Some(UPDATED),
            Self::Installed => Some(INSTALLED),
        }
    }
}

/// Reads how this copy was started. After an update or install it waits for the old copy to exit and deletes it.
pub fn arrive() -> Arrival {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (arrival, pid) = parse(&args);
    if let Some(pid) = pid {
        wait_for_exit(Pid::from_u32(pid));
        if let Ok(target) = Target::current() {
            tidy(&aside(target.path()));
        }
    }
    arrival
}

/// Starts `target` and tells it how it arrived, with this process's id to wait for.
pub fn relaunch(target: &Target, arrival: Arrival) -> io::Result<()> {
    let flag: Vec<String> =
        arrival.flag().map(|flag| vec![flag.to_owned(), std::process::id().to_string()]).unwrap_or_default();
    match target {
        Target::Binary(exe) => {
            let mut command = Command::new(exe);
            command.args(&flag).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Some(dir) = exe.parent() {
                command.current_dir(dir);
            }
            command.spawn().map(drop)
        }
        Target::Bundle(app) => {
            // Through Launch Services, so it starts like any app. `-n` opens a new one even while this one runs.
            let status = Command::new("/usr/bin/open").arg("-n").arg(app).arg("--args").args(&flag).status()?;
            if status.success() { Ok(()) } else { Err(io::Error::other(status.to_string())) }
        }
    }
}

/// Stops every other running notchbase except hook processes, and waits for them to exit.
pub fn stop_others() {
    let own = sysinfo::get_current_pid().ok();
    let name = std::env::current_exe().ok().and_then(|exe| exe.file_name().map(ToOwned::to_owned));
    let Some(name) = name else { return };
    let mut system = System::new();
    let what = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always);
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, what);
    let others: Vec<Pid> = system
        .processes()
        .iter()
        .filter(|(pid, process)| {
            Some(**pid) != own
                && process.name() == name.as_os_str()
                && process.cmd().get(1).is_none_or(|arg| arg.to_str() != Some(HOOK))
        })
        .map(|(pid, _)| *pid)
        .collect();
    for pid in &others {
        if let Some(process) = system.process(*pid) {
            process.kill();
        }
    }
    for pid in others {
        wait_for_exit(pid);
    }
}

fn parse(args: &[String]) -> (Arrival, Option<u32>) {
    let arrival = match args.first().map(String::as_str) {
        Some(UPDATED) => Arrival::Updated,
        Some(INSTALLED) => Arrival::Installed,
        _ => return (Arrival::Fresh, None),
    };
    (arrival, args.get(1).and_then(|pid| pid.parse().ok()))
}

fn wait_for_exit(pid: Pid) {
    let mut system = System::new();
    let deadline = Instant::now() + WAIT_FOR_PREVIOUS;
    while Instant::now() < deadline {
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if system.process(pid).is_none() {
            return;
        }
        std::thread::sleep(POLL);
    }
}

fn tidy(path: &std::path::Path) {
    let deadline = Instant::now() + TIDY_FOR;
    while path.exists() && Instant::now() < deadline {
        remove(path);
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn new_copy_knows_why_it_started() {
        assert_eq!(parse(&args(&["--replaces", "42"])), (Arrival::Updated, Some(42)));
        assert_eq!(parse(&args(&["--installed", "7"])), (Arrival::Installed, Some(7)));
        assert_eq!(parse(&args(&[])), (Arrival::Fresh, None));
        assert_eq!(parse(&args(&["hook", "claude"])), (Arrival::Fresh, None));
    }

    #[test]
    fn fresh_start_passes_no_flags() {
        assert_eq!(Arrival::Fresh.flag(), None);
        assert_eq!(Arrival::Updated.flag(), Some(UPDATED));
    }
}
