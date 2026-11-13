//! The app rows. Processes are grouped by app, and an app is listed while it plays, has a tap, or was heard lately.

use std::collections::HashMap;
use std::time::Instant;

use super::app_labels::load_label;
use super::processes::{AppProcesses, Process, ProcessScanner, ScannedProcess};
use super::taps::{Mix, Taps};
use crate::audio::activity::Activity;
use crate::audio::grouping::{Group, Member, group_members};
use crate::audio::keys::app_id;
use crate::audio::labels::AppLabels;
use crate::audio::sorting::sort_key;
use crate::audio::{AppId, AudioApp};

#[derive(Default)]
pub struct AppList {
    rows: Vec<AudioApp>,
    owners: Vec<AppProcesses>,
    scanner: ProcessScanner,
    labels: AppLabels,
    activity: Activity,
}

impl AppList {
    pub fn rows(&self) -> &[AudioApp] {
        &self.rows
    }

    pub fn position(&self, id: AppId) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    pub fn id(&self, i: usize) -> AppId {
        self.rows[i].id
    }

    pub fn owner(&self, i: usize) -> &AppProcesses {
        &self.owners[i]
    }

    pub fn mix(&self, i: usize) -> Mix {
        Mix { volume: self.rows[i].volume, muted: self.rows[i].muted }
    }

    pub fn set_mix(&mut self, i: usize, mix: Mix) {
        self.rows[i].volume = mix.volume;
        self.rows[i].muted = mix.muted;
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.owners.clear();
    }

    pub fn update_levels(&mut self, peak_of: impl Fn(AppId) -> f32) {
        let now = Instant::now();
        for row in &mut self.rows {
            row.peak = peak_of(row.id);
            self.activity.note(row.id, row.peak, now);
        }
    }

    pub fn refresh(&mut self, taps: &Taps) {
        let now = Instant::now();
        let members: Vec<Member<Process>> = self
            .scanner
            .scan()
            .into_iter()
            .filter_map(|scanned| {
                let id = app_id(&scanned.key);
                let listed = scanned.running || taps.is_active(id) || self.activity.is_recent(id, now);
                listed.then(|| member(scanned, taps.desired(id)))
            })
            .collect();
        let old_peaks: HashMap<AppId, f32> = self.rows.iter().map(|row| (row.id, row.peak)).collect();
        let mut listed: Vec<(AudioApp, AppProcesses)> = group_members(members)
            .into_iter()
            .map(|group| {
                self.activity.note(group.id, group.peak, now);
                let peak = old_peaks.get(&group.id).copied().unwrap_or(0.0);
                let row = self.app_row(&group, peak);
                (row, AppProcesses { key: group.key, processes: group.members })
            })
            .collect();
        listed.sort_by_cached_key(|(row, _)| sort_key(row, false, self.activity.is_recent(row.id, now)));
        self.activity.retain(|id| listed.iter().any(|(row, _)| row.id == id));
        (self.rows, self.owners) = listed.into_iter().unzip();
    }

    fn app_row(&mut self, group: &Group<Process>, peak: f32) -> AudioApp {
        let pid = group.members[0].pid;
        let label = self.labels.get_or_load(&group.key, || load_label(&group.key, pid));
        AudioApp {
            id: group.id,
            name: label.name.clone(),
            volume: group.volume.clamp(0.0, 1.0),
            muted: group.muted,
            peak,
            icon: label.icon.clone(),
        }
    }
}

fn member(scanned: ScannedProcess, mix: Mix) -> Member<Process> {
    Member {
        key: scanned.key,
        system: false,
        volume: mix.volume,
        muted: mix.muted,
        peak: if scanned.running { 1.0 } else { 0.0 },
        handle: scanned.process,
    }
}
