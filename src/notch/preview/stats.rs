//! Sample history, storage and apps for the stats previews.

use std::sync::Arc;

use crate::notch::Notch;
use crate::stats::{AppUsage, DAYS, Item, Part, PartUsage, Resource, Storage, UsageHistory, Week};

use super::Preview;

const STEADY: [Option<f32>; DAYS] = [Some(0.34), Some(0.58), None, Some(0.71), Some(0.46), Some(0.88), Some(0.52)];
const GIGABYTE: u64 = 1_000_000_000;
const APPS: [(&str, f32, u64, bool); 6] = [
    ("Chrome", 0.31, 2_100, true),
    ("Xcode", 0.18, 4_800, true),
    ("Figma", 0.06, 1_200, true),
    ("Finder", 0.02, 300, false),
    ("Slack", 0.012, 640, true),
    ("Music", 0.004, 210, true),
];
type SamplePart = (Part, u64, &'static [(&'static str, u64)]);
const PARTS: [SamplePart; 6] = [
    (Part::Documents, 91_000, &[("Final cut.mov", 9_400), ("Thesis", 6_100), ("Scans.zip", 2_300)]),
    (Part::Apps, 31_200, &[("Xcode", 12_400), ("Parallels Desktop", 3_100), ("Figma", 1_200), ("Slack", 640)]),
    (Part::Machines, 21_600, &[("Windows 11", 21_600)]),
    (Part::AppData, 24_500, &[("Chrome cache", 6_200), ("Chrome", 4_100), ("Mail", 3_300)]),
    (Part::Developer, 20_900, &[("notchbase target", 8_800), ("Homebrew", 5_200), ("Rustup", 1_700)]),
    (Part::Media, 8_700, &[("Photos Library", 7_900)]),
];
const DISK: (u64, u64) = (171 * GIGABYTE, 494 * GIGABYTE);
const HEAVY: [Option<f32>; DAYS] = [Some(0.62), Some(1.0), Some(0.41), None, Some(1.0), Some(0.77), Some(0.93)];

impl Notch {
    pub fn previewed_history(&self) -> Option<UsageHistory> {
        let peaks = match self.preview? {
            Preview::Stats | Preview::StatsApps | Preview::StatsDisk => STEADY,
            Preview::StatsHistory => HEAVY,
            _ => return None,
        };
        Some(UsageHistory::both(Week::ending_today(peaks)))
    }

    pub fn previewed_storage(&self) -> Option<(Storage, u64, u64)> {
        (self.preview? == Preview::StatsDisk).then(|| (sample_storage(), DISK.0, DISK.1))
    }

    pub fn previewed_apps(&self) -> Option<(Resource, Vec<AppUsage>)> {
        (self.preview? == Preview::StatsApps).then(|| (Resource::Cpu, sample_apps()))
    }
}

fn sample_storage() -> Storage {
    let item = |(name, megabytes): &(&str, u64)| Item {
        name: (*name).to_owned(),
        path: format!("/{name}").into(),
        size: megabytes * GIGABYTE / 1000,
        folder: !name.contains('.'),
        icon: None,
    };
    let part = |(part, megabytes, items): SamplePart| {
        Arc::new(PartUsage { part, size: megabytes * GIGABYTE / 1000, items: items.iter().map(item).collect() })
    };
    Storage { parts: PARTS.map(part).to_vec(), measuring: false }
}

fn sample_apps() -> Vec<AppUsage> {
    let app = |(name, cpu, megabytes, quittable): (&str, f32, u64, bool)| AppUsage {
        key: name.to_lowercase(),
        name: name.to_owned(),
        icon: None,
        cpu,
        memory: megabytes * GIGABYTE / 1000,
        quittable,
    };
    APPS.map(app).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::Provider;
    use crate::bridge::Page;
    use crate::platform::Style;

    fn history(preview: Preview) -> Option<UsageHistory> {
        Notch::with_preview(Style::Island, None, Some(preview)).previewed_history()
    }

    #[test]
    fn stats_previews_pin_sample_week() {
        let steady = history(Preview::Stats).unwrap();
        assert_eq!(steady.week(Provider::Claude).days_at_limit(), 0);
        assert_eq!(steady.week(Provider::Codex), steady.week(Provider::Claude));
        let heavy = history(Preview::StatsHistory).unwrap();
        assert_eq!(heavy.week(Provider::Claude).days_at_limit(), 2);
        assert_eq!(heavy.claude.days[3].peak, None);
        let pinned = Notch::with_preview(Style::Island, None, Some(Preview::StatsHistory));
        assert_eq!(pinned.pager.page, Page::Stats);
        assert_eq!(history(Preview::Agents), None);
        assert_eq!(history(Preview::StatsApps), steady.into());
        let (by, apps) = Notch::with_preview(Style::Island, None, Some(Preview::StatsApps)).previewed_apps().unwrap();
        assert_eq!((by, apps.len()), (Resource::Cpu, APPS.len()));
        assert!(pinned.previewed_apps().is_none());
        let disk = Notch::with_preview(Style::Island, None, Some(Preview::StatsDisk));
        let (storage, free, total) = disk.previewed_storage().unwrap();
        assert!(storage.measured() < total - free && storage.parts[0].size > storage.parts[1].size);
        assert!(pinned.previewed_storage().is_none());
    }
}
