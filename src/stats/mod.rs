//! The Stats page's numbers: hardware gauges, the apps and storage behind them, and Claude or ChatGPT plan usage.
//! Each source works off the main thread, and `Stats::sync` copies what's new into the snapshot.

mod apps;
pub(crate) mod disk;
mod hardware;
mod history;
mod sampler;

use std::sync::Arc;
use std::time::SystemTime;

pub use apps::{AppUsage, LISTED, LISTS_APPS, Resource, top};
pub use disk::{Item, Part, PartUsage, Storage};
pub use history::{DAYS, Day, UsageHistory, Week};
pub use notchbase_usage::{PlanUsage, USAGE_STALE_AFTER, UsageWindow};

use notchbase_usage::{Claude, Codex};

use crate::assistant::Provider;
use crate::poll::{Wake, lock};
use hardware::Readings;
use history::History;
use sampler::Sampler;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub gpu_name: Option<String>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub disk_free: Option<u64>,
    pub disk_total: Option<u64>,
    pub listing: Option<Resource>,
    pub apps: Vec<AppUsage>,
    pub storage: Storage,
    pub claude: PlanUsage,
    pub claude_refreshing: bool,
    pub chatgpt: PlanUsage,
    pub chatgpt_refreshing: bool,
    pub history: UsageHistory,
}

impl Snapshot {
    pub fn usage(&self, assistant: Provider) -> (&PlanUsage, bool) {
        match assistant {
            Provider::Claude => (&self.claude, self.claude_refreshing),
            Provider::Codex => (&self.chatgpt, self.chatgpt_refreshing),
        }
    }

    fn show(&mut self, readings: Readings) {
        self.cpu = readings.cpu;
        self.gpu = readings.gpu;
        self.gpu_name = readings.gpu_name;
        (self.mem_used, self.mem_total) = readings.memory.map(|m| (m.used, m.total)).unzip();
        (self.disk_free, self.disk_total) = readings.disk.map(|d| (d.free, d.total)).unzip();
        if let Some(apps) = readings.apps {
            self.apps = apps;
        }
    }
}

pub struct Stats {
    snapshot: Snapshot,
    sampler: Sampler,
    disk: disk::Disk,
    claude: Claude,
    codex: Codex,
    history: History,
}

impl Stats {
    pub fn new(wake: Wake, logs_history: bool) -> Self {
        Self {
            snapshot: Snapshot::default(),
            sampler: Sampler::spawn(Arc::clone(&wake)),
            disk: disk::Disk::new(Arc::clone(&wake)),
            history: History::open(Arc::clone(&wake), logs_history),
            codex: Codex::new(Arc::clone(&wake)),
            claude: Claude::new(wake),
        }
    }

    /// Called once per refresh while the page shows. Asks for new readings and polls the chosen assistant's usage.
    pub fn sample(&mut self, assistant: Option<Provider>) {
        self.sampler.request(self.lists_apps());
        if self.snapshot.listing == Some(Resource::Disk) {
            self.disk.measure();
        }
        match assistant {
            Some(Provider::Claude) => self.claude.poll(),
            Some(Provider::Codex) => self.codex.poll(),
            None => {}
        }
        self.sync();
    }

    pub fn list_apps(&mut self, by: Option<Resource>) {
        self.snapshot.listing = by;
        if self.lists_apps() {
            self.sampler.request(true);
        }
        if by == Some(Resource::Disk) {
            self.disk.measure();
        }
    }

    fn lists_apps(&self) -> bool {
        self.snapshot.listing.is_some_and(Resource::lists_apps)
    }

    /// Shows fixed storage, for previews.
    pub fn pin_storage(&mut self, storage: Storage) {
        self.snapshot.listing = Some(Resource::Disk);
        self.snapshot.storage = storage;
    }

    /// Only an app in the snapshot that's marked quittable is touched.
    pub fn quit_app(&self, key: &str, force: bool) -> bool {
        self.snapshot.apps.iter().any(|app| app.key == key && app.quittable) && apps::quit(key, force)
    }

    /// Shows a fixed app list, for previews.
    pub fn pin_apps(&mut self, by: Resource, apps: Vec<AppUsage>) {
        self.snapshot.listing = Some(by);
        self.snapshot.apps = apps;
    }

    pub fn refresh_usage(&mut self) {
        self.claude.force();
        self.codex.force();
        self.snapshot.claude_refreshing = self.claude.busy();
        self.snapshot.chatgpt_refreshing = self.codex.busy();
    }

    pub fn sync(&mut self) {
        if let Some(readings) = self.sampler.take() {
            self.snapshot.show(readings);
        }
        if let Some(storage) = self.disk.take() {
            self.snapshot.storage = storage;
        }
        if let Some(usage) = self.claude.latest() {
            self.history.observe(Provider::Claude, &usage);
            self.snapshot.claude = usage;
        }
        if let Some(usage) = self.codex.latest() {
            self.history.observe(Provider::Codex, &usage);
            self.snapshot.chatgpt = usage;
        }
        if self.history.sync(SystemTime::now()) {
            self.snapshot.history = *self.history.weeks();
        }
        self.snapshot.claude_refreshing = self.claude.busy();
        self.snapshot.chatgpt_refreshing = self.codex.busy();
    }

    /// Shows a fixed week of usage, for previews.
    pub fn pin_history(&mut self, history: UsageHistory) {
        self.history.pin(history);
        self.snapshot.history = history;
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::hardware::{Disk, Memory};
    use super::*;

    #[test]
    fn readings_replace_the_hardware_half() {
        let mut snapshot = Snapshot { claude_refreshing: true, ..Snapshot::default() };
        snapshot.show(Readings {
            cpu: Some(0.25),
            gpu: None,
            gpu_name: Some("Apple M4 GPU".into()),
            memory: Some(Memory { used: 3, total: 8 }),
            disk: Some(Disk { free: 5, total: 9 }),
            apps: Some(vec![AppUsage {
                key: "com.apple.Safari".into(),
                name: "Safari".into(),
                icon: None,
                cpu: 0.1,
                memory: 2,
                quittable: true,
            }]),
        });
        assert_eq!(
            (snapshot.cpu, snapshot.gpu, snapshot.gpu_name.as_deref()),
            (Some(0.25), None, Some("Apple M4 GPU"))
        );
        assert_eq!(
            (snapshot.mem_used, snapshot.mem_total, snapshot.disk_free, snapshot.disk_total),
            (Some(3), Some(8), Some(5), Some(9))
        );
        snapshot.show(Readings::default());
        assert_eq!((snapshot.mem_used, snapshot.mem_total, snapshot.disk_free), (None, None, None));
        assert_eq!(snapshot.apps.len(), 1, "a sample that did not read the apps keeps the last list");
        assert!(snapshot.claude_refreshing && snapshot.claude == PlanUsage::Loading, "Claude state is left alone");
    }
}
