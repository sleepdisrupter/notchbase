//! Hardware readings: CPU and memory through sysinfo, GPU and the system disk from each platform. The disk is read at
//! most every 10 seconds, and the app list every 1.5 seconds while it's open.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;

use std::time::{Duration, Instant};

use sysinfo::{MemoryRefreshKind, System};

use super::apps::{AppUsage, Watch};

const DISK_EVERY: Duration = Duration::from_secs(10);
const APPS_EVERY: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Memory {
    pub(super) used: u64,
    pub(super) total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Disk {
    pub(super) free: u64,
    pub(super) total: u64,
}

#[derive(Debug, Default)]
pub(super) struct Readings {
    pub(super) cpu: Option<f32>,
    pub(super) gpu: Option<f32>,
    pub(super) gpu_name: Option<String>,
    pub(super) memory: Option<Memory>,
    pub(super) disk: Option<Disk>,
    pub(super) apps: Option<Vec<AppUsage>>,
}

pub(super) struct Probe {
    system: System,
    cpu_primed: bool,
    gpu: Option<imp::Gpu>,
    gpu_name: Option<String>,
    disk: Option<Disk>,
    disk_read_at: Option<Instant>,
    apps: Option<Watch>,
    apps_read_at: Option<Instant>,
}

impl Probe {
    pub(super) fn open() -> Self {
        let mut probe = Self {
            system: System::new(),
            cpu_primed: false,
            gpu: imp::Gpu::open(),
            gpu_name: imp::gpu_name(),
            disk: None,
            disk_read_at: None,
            apps: None,
            apps_read_at: None,
        };
        probe.refresh_disk(Instant::now());
        probe
    }

    pub(super) fn statics(&self) -> Readings {
        Readings { gpu_name: self.gpu_name.clone(), disk: self.disk, ..Readings::default() }
    }

    pub(super) fn sample(&mut self, lists_apps: bool) -> Readings {
        self.refresh_disk(Instant::now());
        Readings {
            apps: self.apps(lists_apps),
            cpu: self.cpu(),
            gpu: self.gpu.as_mut().and_then(imp::Gpu::sample),
            gpu_name: self.gpu_name.clone(),
            memory: self.memory(),
            disk: self.disk,
        }
    }

    fn cpu(&mut self) -> Option<f32> {
        self.system.refresh_cpu_usage();
        // CPU usage needs two readings, so the first one has nothing to report.
        if !std::mem::replace(&mut self.cpu_primed, true) {
            return None;
        }
        let usage = self.system.global_cpu_usage();
        usage.is_finite().then(|| (usage / 100.0).clamp(0.0, 1.0))
    }

    fn apps(&mut self, listed: bool) -> Option<Vec<AppUsage>> {
        if !listed {
            // A folded list drops its reader, so nothing is measured for a list nobody sees.
            (self.apps, self.apps_read_at) = (None, None);
            return None;
        }
        if self.apps_read_at.is_some_and(|at| at.elapsed() < APPS_EVERY) {
            return None;
        }
        let apps = self.apps.get_or_insert_with(Watch::open).sample();
        self.apps_read_at = Some(Instant::now());
        Some(apps)
    }

    fn memory(&mut self) -> Option<Memory> {
        self.system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let total = self.system.total_memory();
        (total > 0).then(|| Memory { used: self.system.used_memory().min(total), total })
    }

    fn refresh_disk(&mut self, now: Instant) {
        if self.disk_read_at.is_none_or(|at| now.duration_since(at) >= DISK_EVERY) {
            self.disk = imp::system_disk();
            self.disk_read_at = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_reports_sane_readings() {
        let mut probe = Probe::open();
        let statics = probe.statics();
        assert_eq!((statics.cpu, statics.gpu, statics.memory), (None, None, None));
        let first = probe.sample(false);
        assert_eq!(first.cpu, None, "the first CPU reading only primes the delta");
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let second = probe.sample(false);
        assert!(second.cpu.is_some_and(|cpu| (0.0..=1.0).contains(&cpu)), "cpu: {:?}", second.cpu);
        assert!(second.gpu.is_none_or(|gpu| (0.0..=1.0).contains(&gpu)), "gpu: {:?}", second.gpu);
        assert!(second.memory.is_some_and(|m| 0 < m.used && m.used <= m.total), "memory: {:?}", second.memory);
        assert!(second.disk.is_some_and(|d| d.free <= d.total), "disk: {:?}", second.disk);
        assert_eq!(second.disk, statics.disk, "the disk is re-read only every {DISK_EVERY:?}");
        assert!(second.apps.is_none(), "apps are only read while they are listed");
        assert_eq!(probe.sample(true).apps.is_some_and(|apps| !apps.is_empty()), crate::stats::LISTS_APPS);
        assert!(probe.sample(true).apps.is_none(), "apps are re-read only every {APPS_EVERY:?}");
    }
}
