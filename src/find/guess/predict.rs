//! Predicts what you will open next from what usually follows, the app in front, the time of day and more.

use std::path::Path;

use crate::find::context::Context;
use crate::find::key::key_of;
use crate::find::model::Live;
use crate::find::usage::{FileRecord, Model, day_of, faded, filled, since};
use crate::find::view::Why;
use crate::find::weights::{DAYPART_MIN_OPENS, OPEN_RECENT_HALF_LIFE, SKIPS_OUT};

use super::why::{after, with};

const PREDICT_MIN: f32 = 0.45;
const FROM_SEQUENCE: f32 = 0.30;
const FROM_APP: f32 = 0.20;
const FROM_DAYPART: f32 = 0.15;
const FROM_HEAT: f32 = 0.15;
const FROM_RECENCY: f32 = 0.20;
const FROM_SKIPS: f32 = 0.15;
const PAIR_MIN_WEIGHT: f32 = 2.0;
const PAIR_MIN_SHARE: f32 = 0.5;
const APP_MIN_PICKS: f32 = 2.0;
const DAYPART_MIN_SHARE: f32 = 0.6;
const HEAT_FILLS_AT: f32 = 1.0;

pub(super) struct Guess<'a> {
    pub chance: f32,
    pub path: &'a str,
    pub why: Why,
}

pub(super) fn predictions<'a>(live: &Live, model: &'a Model, context: &Context) -> Vec<Guess<'a>> {
    let in_hand = model.just_used(context.now);
    let may_be_guessed = |record: &FileRecord| record.opened() && !record.benched() && Some(record.key) != in_hand;
    let mut guesses: Vec<Guess<'a>> = model
        .files()
        .filter(|(record, _)| may_be_guessed(record))
        .filter_map(|(record, path)| guess(live, model, record, path, context))
        .collect();
    guesses.sort_by(|a, b| b.chance.total_cmp(&a.chance).then(a.path.cmp(b.path)));
    guesses
}

fn guess<'a>(live: &Live, model: &Model, record: &FileRecord, path: &'a str, context: &Context) -> Option<Guess<'a>> {
    let now = context.now;
    let follows = model.after_previous(record.key, now);
    let sequence = follows.filter(|follows| follows.weight >= PAIR_MIN_WEIGHT && follows.share >= PAIR_MIN_SHARE);
    let with_app = context.app.as_deref().and_then(|app| model.with_app(record, app));
    let around = record.around(context.daypart());
    let evidence = [
        FROM_SEQUENCE * sequence.map_or(0.0, |follows| follows.share),
        FROM_APP * with_app.filter(|habit| habit.count >= APP_MIN_PICKS).map_or(0.0, |habit| habit.share),
        FROM_DAYPART
            * around
                .filter(|habit| habit.count >= DAYPART_MIN_OPENS && habit.share >= DAYPART_MIN_SHARE)
                .map_or(0.0, |habit| habit.share),
    ];
    let recency = FROM_RECENCY * faded(since(now, record.last_open), OPEN_RECENT_HALF_LIFE);
    let skips = FROM_SKIPS * (f32::from(record.skips_on(day_of(now))) / f32::from(SKIPS_OUT)).min(1.0);
    let before_heat = evidence.iter().sum::<f32>() + recency - skips;
    if before_heat + FROM_HEAT < PREDICT_MIN {
        return None;
    }
    let folder = Path::new(path).parent().map(key_of);
    let heat = FROM_HEAT * folder.map_or(0.0, |folder| filled(model.heat(folder, now), HEAT_FILLS_AT));
    let chance = before_heat + heat;
    let strongest =
        (0..evidence.len()).filter(|at| evidence[*at] > 0.0).max_by(|a, b| evidence[*a].total_cmp(&evidence[*b]));
    let told = match strongest {
        Some(0) => after(live, model, record.key, now),
        Some(1) => with(model, record.key, context),
        Some(_) => Some(Why::AroundNow),
        None => None,
    };
    (chance >= PREDICT_MIN).then(|| Guess { chance, path, why: told.unwrap_or(Why::Opened(record.last_open)) })
}
