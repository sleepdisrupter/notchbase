//! Per-app usage on macOS: CPU time and footprint from proc_pid_rusage, grouped under the app responsible for each
//! process. A share is of the whole machine, like the gauge.

use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};
use objc2_foundation::NSString;

use super::{AppUsage, leading_keys};
use crate::audio::{load_label, parent_bundle_id, pid_in_key, pid_key, responsible_pid, running_app};

const PRIME: Duration = Duration::from_millis(250);
const SPARE_PIDS: usize = 64;
const APPLE: &str = "com.apple.";
const FINDER: &str = "com.apple.finder";

#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}

struct Group {
    key: String,
    owner: i32,
    quittable: bool,
}

struct Label {
    name: String,
    icon: Option<Arc<egui::ColorImage>>,
}

#[derive(Default)]
struct Reading {
    busy: u64,
    memory: u64,
    owner: i32,
    quittable: bool,
}

pub(in crate::stats) struct Watch {
    cores: f64,
    ticks_to_nanos: f64,
    groups: HashMap<i32, Option<Group>>,
    labels: HashMap<String, Label>,
    spent: HashMap<i32, u64>,
    read_at: Option<Instant>,
}

impl Watch {
    pub(in crate::stats) fn open() -> Self {
        Self {
            cores: thread::available_parallelism().map_or(1.0, |cores| cores.get() as f64),
            ticks_to_nanos: ticks_to_nanos(),
            groups: HashMap::new(),
            labels: HashMap::new(),
            spent: HashMap::new(),
            read_at: None,
        }
    }

    pub(in crate::stats) fn sample(&mut self) -> Vec<AppUsage> {
        autoreleasepool(|_| {
            if self.read_at.is_none() {
                self.read();
                // The first read only sets a baseline, so wait a moment and read again.
                thread::sleep(PRIME);
            }
            let (readings, elapsed) = self.read();
            self.usage(&readings, elapsed)
        })
    }

    fn read(&mut self) -> (HashMap<String, Reading>, Duration) {
        let now = Instant::now();
        let elapsed = self.read_at.replace(now).map_or(Duration::ZERO, |at| now.duration_since(at));
        let me = std::process::id() as i32;
        let mut spent = HashMap::new();
        let mut readings: HashMap<String, Reading> = HashMap::new();
        for pid in all_pids() {
            let Some((ticks, memory)) = rusage(pid) else { continue };
            let Some(group) = self.groups.entry(pid).or_insert_with(|| group_of(pid, me)) else { continue };
            let reading = readings.entry(group.key.clone()).or_default();
            reading.busy += self.spent.get(&pid).map_or(0, |before| ticks.saturating_sub(*before));
            reading.memory += memory;
            reading.quittable = group.quittable;
            reading.owner = group.owner;
            spent.insert(pid, ticks);
        }
        self.groups.retain(|pid, _| spent.contains_key(pid));
        self.spent = spent;
        (readings, elapsed)
    }

    fn usage(&mut self, readings: &HashMap<String, Reading>, elapsed: Duration) -> Vec<AppUsage> {
        let capacity = elapsed.as_nanos() as f64 * self.cores;
        let share =
            |busy: u64| if capacity > 0.0 { (busy as f64 * self.ticks_to_nanos / capacity) as f32 } else { 0.0 };
        let leaders = leading_keys(readings.iter().map(|(key, r)| (key.as_str(), share(r.busy), r.memory)));
        self.labels.retain(|key, _| readings.contains_key(key));
        let mut apps = Vec::with_capacity(leaders.len());
        for key in leaders {
            let reading = &readings[key];
            let label = self.labels.entry(key.to_owned()).or_insert_with(|| {
                let label = load_label(key, reading.owner);
                Label { name: label.name, icon: label.icon }
            });
            apps.push(AppUsage {
                key: key.to_owned(),
                name: label.name.clone(),
                icon: label.icon.clone(),
                cpu: share(reading.busy).clamp(0.0, 1.0),
                memory: reading.memory,
                quittable: reading.quittable,
            });
        }
        apps
    }
}

pub(in crate::stats) fn quit(key: &str, force: bool) -> bool {
    let apps: Vec<_> = match pid_in_key(key) {
        Some(pid) => pid.parse().ok().and_then(running_app).into_iter().collect(),
        None => NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(key)).to_vec(),
    };
    let mut asked = false;
    for app in apps {
        asked |= if force { app.forceTerminate() } else { app.terminate() };
    }
    asked
}

fn group_of(pid: i32, me: i32) -> Option<Group> {
    let owner = responsible_pid(pid).unwrap_or(pid);
    let app = running_app(owner)?;
    let policy = app.activationPolicy();
    // An owner that may never activate isn't an app.
    if policy == NSApplicationActivationPolicy::Prohibited {
        return None;
    }
    let bundle = app.bundleIdentifier().map(|id| id.to_string()).filter(|id| !id.is_empty());
    let quittable = owner != me && may_quit(bundle.as_deref(), policy == NSApplicationActivationPolicy::Regular);
    let key = bundle.map_or_else(|| pid_key(owner), |bundle| parent_bundle_id(&bundle).to_owned());
    Some(Group { key, owner, quittable })
}

/// Finder can't be quit, nor can an Apple agent without a Dock icon.
fn may_quit(bundle: Option<&str>, in_dock: bool) -> bool {
    let bundle = bundle.unwrap_or_default().to_ascii_lowercase();
    if in_dock { bundle != FINDER } else { !bundle.starts_with(APPLE) }
}

fn all_pids() -> Vec<i32> {
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    // Room for processes started between the two calls.
    let mut pids = vec![0i32; usize::try_from(count).unwrap_or(0) + SPARE_PIDS];
    let bytes = (pids.len() * size_of::<i32>()) as i32;
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(filled).unwrap_or(0));
    pids.retain(|pid| *pid > 0);
    pids
}

fn rusage(pid: i32) -> Option<(u64, u64)> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    let read = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast()) } == 0;
    read.then(|| {
        let info = unsafe { info.assume_init() };
        (info.ri_user_time + info.ri_system_time, info.ri_phys_footprint)
    })
}

fn ticks_to_nanos() -> f64 {
    let mut base = Timebase { numer: 0, denom: 0 };
    unsafe { mach_timebase_info(&mut base) };
    if base.denom == 0 { 1.0 } else { f64::from(base.numer) / f64::from(base.denom) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_agents_and_finder_have_no_quit() {
        assert!(may_quit(Some("com.google.Chrome"), true));
        assert!(may_quit(Some("com.apple.Safari"), true));
        assert!(!may_quit(Some("com.apple.finder"), true));
        assert!(may_quit(Some("com.mitchellh.ghostty.menu"), false));
        assert!(!may_quit(Some("com.apple.controlcenter"), false));
        assert!(may_quit(None, true));
    }

    #[test]
    fn own_process_is_never_quittable() {
        let me = std::process::id() as i32;
        assert!(all_pids().contains(&me));
        let (_, memory) = rusage(me).unwrap();
        assert!(memory > 0);
        let owner = responsible_pid(me).unwrap_or(me);
        assert!(group_of(me, owner).is_none_or(|group| !group.quittable), "the app that owns this process");
        assert!(ticks_to_nanos() > 0.0);
    }

    #[test]
    fn sample_reports_sane_shares() {
        let mut watch = Watch::open();
        for app in watch.sample() {
            assert!((0.0..=1.0).contains(&app.cpu), "{}: {}", app.name, app.cpu);
            assert!(!app.key.is_empty() && !app.name.is_empty());
        }
        assert!(watch.read_at.is_some());
    }
}
