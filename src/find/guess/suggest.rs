//! The empty field's list: predictions first, then recent files, a hot folder and the most used.

use std::path::Path;

use crate::find::context::Context;
use crate::find::model::{EntryId, Live};
use crate::find::query::Away;
use crate::find::rank::{far, launcher};
use crate::find::usage::{FileRecord, Model};
use crate::find::view::{ONE_SCREEN, Why};

use super::newest::{human, newest};
use super::predict::predictions;
use super::why::busy;

const PREDICTIONS: usize = 2;
const RECENTS_END_AT: usize = 10;
const HOT_FOLDER_MIN: f32 = 3.0;
const HOT_FOLDER_ROW: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Suggested {
    pub id: EntryId,
    pub why: Why,
    pub predicted: bool,
}

#[cfg(test)]
pub fn suggest(live: &Live, model: &Model, context: &Context, most: usize) -> Vec<Suggested> {
    suggest_below(live, model, context, most, None, false)
}

pub fn suggest_below(
    live: &Live,
    model: &Model,
    context: &Context,
    most: usize,
    home: Option<&Path>,
    everything: bool,
) -> Vec<Suggested> {
    let sight = Sight { home: home.and_then(|home| live.resolve(home)), everything };
    topped_up(live, model, context, (most, sight), home, &human)
}

#[derive(Clone, Copy, Default)]
pub(super) struct Sight {
    home: Option<EntryId>,
    everything: bool,
}

pub(super) fn topped_up(
    live: &Live,
    model: &Model,
    context: &Context,
    (most, sight): (usize, Sight),
    home: Option<&Path>,
    wanted: &dyn Fn(&[u8], u8) -> bool,
) -> Vec<Suggested> {
    let mut list = learned(live, model, context, most, sight);
    if list.rows.len() < ONE_SCREEN.min(most) {
        for (id, why) in newest(live, context.now, home, most, wanted) {
            list.add(id, why, false);
        }
    }
    list.rows
}

fn learned<'a>(live: &'a Live, model: &Model, context: &Context, most: usize, sight: Sight) -> List<'a> {
    let now = context.now;
    let mut list = List { live, rows: Vec::new(), most, sight };
    for guess in predictions(live, model, context) {
        if list.rows.len() >= PREDICTIONS.min(most) {
            break;
        }
        list.offer(guess.path, guess.why, true);
    }
    let mut opened: Vec<(f32, &FileRecord, &str)> = model
        .files()
        .filter(|(record, _)| record.opened())
        .map(|(record, path)| (record.opens_at(now), record, path))
        .collect();
    opened.sort_by(|a, b| b.1.last_open.cmp(&a.1.last_open).then(a.2.cmp(b.2)));
    for (_, record, path) in &opened {
        if list.rows.len() >= RECENTS_END_AT.min(most) {
            break;
        }
        list.offer(path, Why::Opened(record.last_open), false);
    }
    list.offer_hot_folder(model, context);
    opened.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(b.2)));
    for (_, record, path) in &opened {
        list.offer(path, Why::Opened(record.last_open), false);
    }
    list
}

struct List<'a> {
    live: &'a Live,
    rows: Vec<Suggested>,
    most: usize,
    sight: Sight,
}

impl List<'_> {
    fn offer(&mut self, path: &str, why: Why, predicted: bool) {
        if self.rows.len() < self.most
            && let Some(id) = self.live.resolve(Path::new(path))
        {
            self.add(id, why, predicted);
        }
    }

    fn add(&mut self, id: EntryId, why: Why, predicted: bool) {
        let room = self.rows.len() < self.most;
        if room && !self.live.is_dead(id) && self.in_sight(id) && self.rows.iter().all(|row| row.id != id) {
            self.rows.push(Suggested { id, why, predicted });
        }
    }

    fn in_sight(&self, id: EntryId) -> bool {
        let (live, flags) = (self.live, self.live.flags(id));
        let app = launcher(live, id, live.folded(id), flags);
        self.sight.everything || app || far(live, id, self.sight.home) == Away::InSight
    }

    fn offer_hot_folder(&mut self, model: &Model, context: &Context) {
        let now = context.now;
        let heated = model.folders().map(|(folder, path)| (folder.heat_at(now), folder.key, path));
        let hottest =
            heated.filter(|(heat, ..)| *heat >= HOT_FOLDER_MIN).max_by(|a, b| a.0.total_cmp(&b.0).then(b.2.cmp(a.2)));
        let Some((_, key, path)) = hottest else { return };
        let (Some(id), Some(why)) = (self.live.resolve(Path::new(path)), busy(model, key, now)) else { return };
        let row = HOT_FOLDER_ROW.min(self.rows.len());
        let offered = !self.live.is_dead(id) && self.in_sight(id);
        if row < self.most && offered && self.rows.iter().all(|row| row.id != id) {
            self.rows.truncate(self.most.saturating_sub(1));
            self.rows.insert(row, Suggested { id, why, predicted: false });
        }
    }
}
