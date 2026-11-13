//! The personal part of a score: how often and how lately you opened a file, and the habits around it.

use crate::find::context::Context;
use crate::find::model::flag;
use crate::find::rank::Candidate;
use crate::find::usage::{FileRecord, Model, faded, filled, since};
use crate::find::weights::{
    APP_HABIT, DAYPART, DAYPART_MIN_OPENS, EDITED, EDITED_FILLS_AT, HABIT_CAP, HEAT, HEAT_GRANDPARENT, OPEN_RECENT,
    OPEN_RECENT_HALF_LIFE, OPENED, OPENED_FILLS_AT, PREFIX, SEQUENCE,
};

use super::why::Signal;

const HEAT_FILLS_AT: f32 = 1.0;
const SIGNALS: usize = 6;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Personal {
    pub habit: f32,
    pub prefix: f32,
    pub why: Option<Signal>,
}

pub fn personal(model: &Model, who: &Candidate, typed: &[u8], context: &Context) -> Personal {
    let parts = habits(model, who, context);
    let habit: f32 = parts.iter().map(|part| part.worth).sum();
    let strongest =
        parts.iter().filter(|part| part.tellable && part.worth > 0.0).max_by(|a, b| a.worth.total_cmp(&b.worth));
    Personal {
        habit: habit.min(HABIT_CAP),
        prefix: PREFIX * model.remembered(typed, who.keys[0], context.now),
        why: strongest.map(|part| part.signal),
    }
}

struct Part {
    signal: Signal,
    worth: f32,
    tellable: bool,
}

fn habits(model: &Model, who: &Candidate, context: &Context) -> [Part; SIGNALS] {
    let now = context.now;
    let record = model.known(who.keys[0]);
    let opened = record.filter(|record| record.opened());
    let recent = |record: &FileRecord| faded(since(now, record.last_open), OPEN_RECENT_HALF_LIFE);
    let open =
        |record: &FileRecord| OPENED * filled(record.opens_at(now), OPENED_FILLS_AT) + OPEN_RECENT * recent(record);
    let edits = record.map_or(0.0, |record| EDITED * filled(record.edits_at(now), EDITED_FILLS_AT));
    let (near, above) =
        if who.flags & flag::DIR != 0 { (who.keys[0], who.keys[1]) } else { (who.keys[1], who.keys[2]) };
    let warmth = |key, most: f32| most * filled(model.heat(key, now), HEAT_FILLS_AT);
    let sequence = model.after_previous(who.keys[0], now).map_or(0.0, |follows| SEQUENCE * follows.share);
    let with_app = |record: &FileRecord| context.app.as_deref().and_then(|app| model.with_app(record, app));
    let around =
        |record: &FileRecord| record.around(context.daypart()).filter(|habit| habit.count >= DAYPART_MIN_OPENS);
    let part = |signal, worth| Part { signal, worth, tellable: true };
    [
        part(Signal::Opened, opened.map_or(0.0, open)),
        Part { tellable: opened.is_some(), ..part(Signal::Edited, edits) },
        part(Signal::Heat, warmth(near, HEAT) + warmth(above, HEAT_GRANDPARENT)),
        part(Signal::Sequence, sequence),
        part(Signal::App, opened.and_then(with_app).map_or(0.0, |habit| APP_HABIT * habit.share)),
        part(Signal::Daypart, opened.and_then(around).map_or(0.0, |habit| DAYPART * habit.share)),
    ]
}
