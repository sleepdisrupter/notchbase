//! The World Time page, drawn on a canvas: a heading and a band of hours for each city.

use jiff::tz::TimeZone;
use notchbase_sdk::prelude::*;

use crate::strip::{self, HOUR_MS, Hour, Light};
use crate::zones::Place;
use crate::{Msg, REACH};

pub const WIDTH: f32 = 340.0;
pub const CELL_W: f32 = 37.5;
pub const HEAD_H: f32 = 22.0;
pub const STRIP_GAP: f32 = 6.0;
pub const BAND_H: f32 = 38.0;
pub const BLOCK_GAP: f32 = 14.0;
const EACH_SIDE: i64 = 6; // hours drawn either side of the middle
const BAND_RADIUS: f32 = 14.0;
const FRAME: f32 = 18.0;
const FRAME_STROKE: f32 = 1.5;
const GUTTER: f32 = 13.0;
const NAME_AT: f32 = 32.0;
const HOME_DOT: f32 = 2.5;
const NAME: f32 = 17.0;
const NUMBER: f32 = 13.0;
const SMALL: f32 = 10.0;
const NOTE_GAP: f32 = 6.0;
const UPPER: f32 = 12.0;
const LOWER: f32 = 26.0;
const STEP_MIN: f32 = 5.0; // the cursor reads in 5 minute steps
const MINUTE_MS: i64 = 60_000;
const GUIDE: f32 = 0.55;
const GUIDE_REACH: f32 = 3.0;
const DAY: u32 = 24;
const HOW: &str = "Move over the hours to read a time. Drag to scroll.";
const NOT_FOUND: &str = "Not found. Try a bigger city nearby.";
const NO_CITIES: &str = "Add cities in Settings to compare them.";

struct Cursor {
    at: i64,
    line: f32,
}

pub struct View<'a> {
    pub places: &'a [Place],
    pub home: &'a TimeZone,
    pub now: u64,
    pub pan: f32,
    pub chosen: i32,
    pub hover: Option<f32>,
    pub full_day: bool,
}

impl View<'_> {
    pub fn page(&self) -> Element<Msg> {
        let cursor = self.cursor();
        let mut shapes = Vec::new();
        let mut top = 0.0;
        for place in self.places {
            shapes.extend(self.heading(place, top + HEAD_H * 0.5, cursor.as_ref()));
            top += HEAD_H;
            if let Some(zone) = &place.zone {
                top += STRIP_GAP;
                shapes.extend(self.band(zone, top));
                shapes.extend(cursor.iter().map(|cursor| guide(cursor.line, top)));
                top += BAND_H;
            }
            top += BLOCK_GAP;
        }
        let drawing = canvas(WIDTH, (top - BLOCK_GAP).max(HEAD_H))
            .extend(shapes)
            .stretch()
            .on_pointer(Msg::Pointer)
            .on_scroll(Msg::Scrolled);
        let hint = (self.places.len() < 2).then(|| text(NO_CITIES).caption().secondary().into());
        column([drawing.into()].into_iter().chain(hint)).push(self.footer(cursor.as_ref())).spacing(BLOCK_GAP).into()
    }

    pub fn glance(&self) -> Option<String> {
        let (place, zone) = self.places.iter().skip(1).find_map(|place| Some((place, place.zone.as_ref()?)))?;
        Some(format!("{} {}", place.name, strip::time(self.shown(), zone, self.full_day)))
    }

    fn heading(&self, place: &Place, middle: f32, cursor: Option<&Cursor>) -> Vec<Shape> {
        let small = |words: String| draw::text(words, 0.0, 0.0).size(SMALL).color(Tone::Secondary);
        let name = draw::text(place.name.clone(), 0.0, 0.0).size(NAME).semibold();
        let Some(zone) = &place.zone else {
            let name = draw::run([name.color(Tone::Tertiary)], NAME_AT, middle).anchor(Anchor::Left);
            return vec![name, draw::run([small(NOT_FOUND.to_owned())], WIDTH, middle).anchor(Anchor::Right)];
        };
        let shown = cursor.map_or_else(|| self.shown(), |cursor| cursor.at);
        let read = if cursor.is_some() { Tone::Yellow } else { Tone::Primary };
        let (clock, half) = strip::clock(shown, zone, self.full_day);
        let mark = match strip::gap(shown, self.home, zone) {
            Some(gap) => draw::text(gap, GUTTER, middle).size(SMALL).color(Tone::Secondary).centered(),
            None => draw::circle(GUTTER, middle, HOME_DOT).fill(Tone::White),
        };
        let half = (!half.is_empty()).then(|| small(half.to_lowercase()));
        let time = [small(strip::date(shown, zone)), draw::text(clock, 0.0, 0.0).size(NAME).semibold().color(read)];
        vec![
            mark,
            draw::run([name, small(strip::abbreviation(shown, zone))], NAME_AT, middle)
                .anchor(Anchor::Left)
                .gap(NOTE_GAP),
            draw::run(time.into_iter().chain(half), WIDTH, middle).anchor(Anchor::Right).gap(NOTE_GAP),
        ]
    }

    fn band(&self, zone: &TimeZone, top: f32) -> Vec<Shape> {
        let first = middle(self.pan) - EACH_SIDE;
        let hours = strip::hours(self.at(middle(self.pan)), EACH_SIDE, zone, self.full_day);
        let chosen = usize::try_from(i64::from(self.chosen) - first).ok().filter(|chosen| *chosen < hours.len());
        let left = |index: usize| left_of(self.pan, first + index as i64);
        let mut shapes = Vec::new();
        let mut from = 0;
        // The band breaks into pieces where a new day starts and around the chosen hour.
        for index in 0..=hours.len() {
            let ends = index == hours.len() || chosen == Some(index) || hours[index].light == Light::NewDay;
            if ends && index > from {
                let width = (index - from) as f32 * CELL_W;
                shapes.push(draw::glass(left(from), top, width, BAND_H).radius(BAND_RADIUS));
            }
            if ends {
                from = if chosen == Some(index) { index + 1 } else { index };
            }
        }
        if let Some(chosen) = chosen {
            let ring = draw::circle(left(chosen) + CELL_W * 0.5, top + BAND_H * 0.5, FRAME);
            shapes.push(ring.stroke(FRAME_STROKE, Tone::White));
        }
        for (index, hour) in hours.iter().enumerate() {
            shapes.extend(cell(hour, chosen == Some(index), left(index) + CELL_W * 0.5, top));
        }
        shapes
    }

    fn cursor(&self) -> Option<Cursor> {
        let line = self.hover?;
        Some(Cursor { at: self.at(0) + minutes_at(self.pan, line) * MINUTE_MS, line })
    }

    fn footer(&self, cursor: Option<&Cursor>) -> Element<Msg> {
        let moved = self.chosen != 0 || self.pan != 0.0;
        let now = self.now.min(i64::MAX as u64) as i64;
        let words = match cursor {
            Some(cursor) => apart((cursor.at - now) as f32 / MINUTE_MS as f32),
            None => away(self.chosen),
        };
        row![text(words).caption().secondary(), spacer(), button("Now").enabled(moved).on_press(Msg::Now)].into()
    }

    fn at(&self, hours: i64) -> i64 {
        strip::hour_start(self.now, self.home) + hours * HOUR_MS
    }

    fn shown(&self) -> i64 {
        if self.chosen == 0 { self.now.min(i64::MAX as u64) as i64 } else { self.at(i64::from(self.chosen)) }
    }
}

pub fn hour_at(pan: f32, over: f32) -> i32 {
    let reach = REACH + EACH_SIDE as f32;
    hours_at(pan, over).floor().clamp(-reach, reach) as i32
}

pub fn away(hours: i32) -> String {
    let count = |count: u32, unit: &str| match count {
        0 => None,
        1 => Some(format!("1 {unit}")),
        count => Some(format!("{count} {unit}s")),
    };
    let span = hours.unsigned_abs();
    let span =
        [count(span / DAY, "day"), count(span % DAY, "hour")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    match hours {
        0 => HOW.to_owned(),
        hours if hours > 0 => format!("In {span}"),
        _ => format!("{span} ago"),
    }
}

pub fn apart(minutes: f32) -> String {
    let whole = ((minutes / STEP_MIN).round() * STEP_MIN) as i64;
    let (hours, rest) = (whole.abs() / 60, whole.abs() % 60);
    let span = match (hours, rest) {
        (0, rest) => format!("{rest} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, rest) => format!("{hours} h {rest} min"),
    };
    match whole {
        0 => "Now".to_owned(),
        whole if whole > 0 => format!("In {span}"),
        _ => format!("{span} ago"),
    }
}

fn guide(line: f32, top: f32) -> Shape {
    draw::line([(line, top - GUIDE_REACH), (line, top + BAND_H + GUIDE_REACH)]).stroke(1.0, Color::white(GUIDE))
}

fn middle(pan: f32) -> i64 {
    pan.round() as i64
}

fn left_of(pan: f32, hour: i64) -> f32 {
    WIDTH * 0.5 + (hour as f32 - pan - 0.5) * CELL_W
}

fn hours_at(pan: f32, over: f32) -> f32 {
    (over - WIDTH * 0.5) / CELL_W + pan + 0.5
}

fn minutes_at(pan: f32, over: f32) -> i64 {
    ((hours_at(pan, over) * 60.0 / STEP_MIN).round() * STEP_MIN) as i64
}

fn cell(hour: &Hour, chosen: bool, middle: f32, top: f32) -> [Shape; 2] {
    let (upper, lower) = match (chosen, hour.light) {
        (true, _) => (Tone::White, Tone::Secondary),
        (false, Light::NewDay) => (Tone::Yellow, Tone::Yellow),
        (false, Light::Day) => (Tone::Primary, Tone::Secondary),
        (false, Light::Dusk) => (Tone::Secondary, Tone::Tertiary),
        (false, Light::Night) => (Tone::Tertiary, Tone::Tertiary),
    };
    let (above, below) = match hour.light {
        Light::NewDay => (SMALL, NUMBER),
        _ => (NUMBER, SMALL),
    };
    let line = |words: &str, size: f32, tone: Tone, down: f32| {
        let words = draw::text(words, middle, top + down).size(size).color(tone).centered();
        if size == NUMBER { words.semibold() } else { words }
    };
    [line(&hour.top, above, upper, UPPER), line(&hour.bottom, below, lower, LOWER)]
}
