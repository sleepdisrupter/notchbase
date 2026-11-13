//! The event editor: a draft with a title, day, start, length and link, set with wheels.

use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use crate::Msg;
use crate::day::{self, at, local, shift};
use crate::meeting::Meeting;
use crate::paint::PALETTE;
use crate::{join, local};

const LENGTHS: [u64; 7] = [15, 30, 45, 60, 90, 120, 180];
const MORNING: i8 = 9;
const MINUTE_STEP: i8 = 5;
const HALF_HOUR: i8 = 30;
const MINUTE_MS: u64 = 60_000;
const DEFAULT_LENGTH: u64 = 30;
const MARK: f32 = 13.0;
const UNTITLED: &str = "New event";
const NOON: i8 = 12;
const LINK_HINT: &str = "Link, like a Zoom or FaceTime invite";

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Title(String),
    Hour(i8),
    Minute(i8),
    Dates,
    Turn(i32),
    PickDate(i32, u8, u8),
    Length(usize),
    Calendar(usize),
    Link(String),
    Color(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub id: Option<String>,
    pub title: String,
    date: Date,
    hour: i8,
    minute: i8,
    minutes: u64,
    calendar: Option<String>,
    picking: Option<Date>,
    link: String,
    color: Option<Tone>,
}

impl Editor {
    pub fn new(now: u64, zone: &TimeZone, on: Date) -> Editor {
        let now = local(now, zone);
        let rounded = (now.minute() / HALF_HOUR + 1) * HALF_HOUR;
        let (later, hour, minute) = match (on == now.date(), now.hour(), rounded) {
            (false, _, _) => (0, MORNING, 0),
            (true, 23, 60) => (1, 0, 0),
            (true, hour, 60) => (0, hour + 1, 0),
            (true, hour, minute) => (0, hour, minute),
        };
        Editor {
            id: None,
            title: String::new(),
            date: shift(on, later),
            hour,
            minute,
            minutes: DEFAULT_LENGTH,
            calendar: None,
            picking: None,
            link: String::new(),
            color: None,
        }
    }

    pub fn editing(meeting: &Meeting, zone: &TimeZone, color: Option<Tone>) -> Editor {
        let start = local(meeting.start, zone);
        Editor {
            id: Some(meeting.id.clone()),
            title: meeting.title.clone(),
            date: start.date(),
            hour: start.hour(),
            minute: start.minute(),
            minutes: ((meeting.end - meeting.start) / MINUTE_MS).max(1),
            calendar: Some(meeting.calendar_id.clone()),
            picking: None,
            link: meeting.link.clone(),
            color,
        }
    }

    pub fn change(&mut self, change: Change, calendars: &[CalendarInfo]) {
        match change {
            Change::Title(title) => self.title = title,
            Change::Hour(hour) => self.hour = hour.clamp(0, 23),
            Change::Minute(minute) => self.minute = minute.clamp(0, 59),
            Change::Dates => {
                self.picking = match self.picking {
                    Some(_) => None,
                    None => Some(self.date.first_of_month()),
                }
            }
            Change::Turn(months) => self.picking = self.picking.map(|first| turn(first, months)),
            Change::PickDate(year, month, day) => {
                let picked = i16::try_from(year).ok().and_then(|year| Date::new(year, month as i8, day as i8).ok());
                if let Some(picked) = picked.filter(|_| self.picking.is_some()) {
                    self.date = picked;
                    self.picking = None;
                }
            }
            Change::Length(index) => self.minutes = lengths(self.minutes).get(index).copied().unwrap_or(self.minutes),
            Change::Calendar(index) => self.calendar = calendars.get(index).map(|calendar| calendar.id.clone()),
            Change::Link(link) => self.link = link,
            Change::Color(index) => self.color = self.swatches(calendars).get(index).copied().flatten(),
        }
    }

    pub fn color(&self) -> Option<Tone> {
        self.color
    }

    pub fn draft(&self, zone: &TimeZone, calendars: &[CalendarInfo]) -> Option<EventDraft> {
        let title = match self.title.trim() {
            "" => UNTITLED,
            title => title,
        };
        let calendar = self.chosen(calendars)?;
        let start = at(self.date, Time::new(self.hour, self.minute, 0, 0).ok()?, zone);
        Some(EventDraft {
            id: self.id.clone(),
            calendar_id: calendars[calendar].id.clone(),
            title: title.to_owned(),
            start,
            end: start + self.minutes * MINUTE_MS,
            url: Some(join::direct(&self.link).unwrap_or_default()),
            ..EventDraft::default()
        })
    }

    pub fn view(&self, today: Date, calendars: &[CalendarInfo], clock_24h: bool, monday_first: bool) -> Element<Msg> {
        let heading = if self.id.is_some() { "Edit event" } else { "New event" };
        let top = row![text(heading).headline(), spacer(), button("").icon(Icon::Xmark).on_press(Msg::Discard)];
        let title =
            text_input(self.title.clone()).placeholder(UNTITLED).on_change(|title| Msg::Editor(Change::Title(title)));
        let link =
            text_input(self.link.clone()).placeholder(LINK_HINT).on_change(|link| Msg::Editor(Change::Link(link)));
        let mut parts: Vec<Element<Msg>> = vec![top.into(), title.into(), link.into()];
        match self.picking {
            Some(first) => {
                parts.push(self.details(today, calendars));
                parts.push(self.dates(first, today, monday_first));
            }
            None => {
                parts.push(row![spacer(), self.clock(clock_24h), spacer()].into());
                parts.push(self.details(today, calendars));
            }
        }
        parts.push(self.bottom(calendars));
        column(parts).spacing(12.0).into()
    }

    fn clock(&self, clock_24h: bool) -> Element<Msg> {
        let (hour, minute) = (self.hour, self.minute);
        let minutes =
            wheel((0..60).step_by(MINUTE_STEP as usize).map(|minute| format!("{minute:02}")), minute_index(minute))
                .looping();
        if clock_24h {
            let hours = wheel((0..24).map(|hour| format!("{hour:02}")), hour as usize).looping();
            return picker([hours, minutes])
                .on_change(move |column, index| match column {
                    0 => Msg::Editor(Change::Hour(index as i8)),
                    _ => Msg::Editor(Change::Minute(index as i8 * MINUTE_STEP)),
                })
                .into();
        }
        let hours =
            wheel((0..NOON).map(|hour| if hour == 0 { NOON } else { hour }.to_string()), (hour % NOON) as usize)
                .looping();
        let halves = wheel(["AM", "PM"], usize::from(hour >= NOON));
        picker([hours, minutes, halves])
            .on_change(move |column, index| match column {
                0 => Msg::Editor(Change::Hour(index as i8 + if hour >= NOON { NOON } else { 0 })),
                1 => Msg::Editor(Change::Minute(index as i8 * MINUTE_STEP)),
                _ => Msg::Editor(Change::Hour(hour % NOON + if index == 1 { NOON } else { 0 })),
            })
            .into()
    }

    fn details(&self, today: Date, calendars: &[CalendarInfo]) -> Element<Msg> {
        let chevron = if self.picking.is_some() { Icon::ChevronUp } else { Icon::ChevronDown };
        let day = button(day::named(self.date, today)).icon(chevron).plain().on_press(Msg::Editor(Change::Dates));
        let lengths = lengths(self.minutes);
        let length = lengths.iter().position(|minutes| *minutes == self.minutes);
        let duration = dropdown(lengths.iter().map(|minutes| length_label(*minutes)), length)
            .on_select(|index| Msg::Editor(Change::Length(index)));
        let mut rows = vec![labelled("Date", day), labelled("Duration", duration)];
        if calendars.len() > 1 {
            let calendar = dropdown(calendars.iter().map(calendar_label), self.chosen(calendars))
                .on_select(|index| Msg::Editor(Change::Calendar(index)));
            rows.push(labelled("Calendar", calendar));
        }
        rows.push(labelled("Color", self.palette(calendars)));
        list(rows)
    }

    fn palette(&self, calendars: &[CalendarInfo]) -> Element<Msg> {
        let own = self.own_color(calendars);
        let swatches = self.swatches(calendars);
        let chosen = swatches.iter().position(|swatch| *swatch == self.color).unwrap_or(0);
        let colors = swatches.iter().map(|swatch| swatch.map_or(own, Tone::color));
        palette(colors).selected(Some(chosen)).on_select(|index| Msg::Editor(Change::Color(index))).into()
    }

    fn own_color(&self, calendars: &[CalendarInfo]) -> Color {
        self.chosen(calendars).and_then(|index| calendars[index].color).unwrap_or_else(|| Tone::Blue.color())
    }

    fn swatches(&self, calendars: &[CalendarInfo]) -> Vec<Option<Tone>> {
        let own = self.own_color(calendars);
        std::iter::once(None).chain(PALETTE.into_iter().filter(|tone| tone.color() != own).map(Some)).collect()
    }

    fn dates(&self, first: Date, today: Date, monday_first: bool) -> Element<Msg> {
        let header = row![
            text(day::month_title(first)).callout(),
            spacer(),
            button("").icon(Icon::ChevronLeft).on_press(Msg::Editor(Change::Turn(-1))),
            button("").icon(Icon::ChevronRight).on_press(Msg::Editor(Change::Turn(1)))
        ]
        .spacing(6.0);
        let mut grid = month(i32::from(first.year()), first.month() as u8).monday_first(monday_first);
        if today.first_of_month() == first {
            grid = grid.today(today.day() as u8);
        }
        if self.date.first_of_month() == first {
            grid = grid.selected(self.date.day() as u8);
        }
        card(
            column![header, grid.on_select(|year, month, day| Msg::Editor(Change::PickDate(year, month, day)))]
                .spacing(6.0),
        )
    }

    fn bottom(&self, calendars: &[CalendarInfo]) -> Element<Msg> {
        let ready = self.chosen(calendars).is_some();
        let mut parts: Vec<Element<Msg>> = Vec::new();
        if let Some(calendar) = self.chosen(calendars).and_then(|index| calendars.get(index)) {
            let (glyph, tone, words) = where_it_goes(calendar);
            parts.push(icon(glyph).size(MARK).tone(tone).into());
            parts.push(text(words).caption().secondary().into());
        }
        parts.push(spacer());
        if self.id.is_some() {
            parts.push(button("Delete").destructive().on_press(Msg::Delete).into());
        }
        parts.push(button("Save").filled().enabled(ready).on_press(Msg::Save).into());
        row(parts).spacing(6.0).into()
    }

    fn chosen(&self, calendars: &[CalendarInfo]) -> Option<usize> {
        let wanted = self.calendar.as_deref();
        wanted
            .and_then(|id| calendars.iter().position(|calendar| calendar.id == id))
            .or_else(|| (!calendars.is_empty()).then_some(0))
    }
}

fn turn(first: Date, months: i32) -> Date {
    let total = i32::from(first.year()) * 12 + i32::from(first.month() - 1) + months;
    let year = i16::try_from(total.div_euclid(12)).unwrap_or(first.year());
    Date::new(year, (total.rem_euclid(12) + 1) as i8, 1).unwrap_or(first)
}

fn labelled(label: &str, control: impl Into<Element<Msg>>) -> Element<Msg> {
    let control: Element<Msg> = control.into();
    row![text(label).callout(), spacer(), control].into()
}

fn minute_index(minute: i8) -> usize {
    ((minute + MINUTE_STEP / 2) / MINUTE_STEP).min(11) as usize
}

fn where_it_goes(calendar: &CalendarInfo) -> (Icon, Tone, String) {
    if calendar.id == local::CALENDAR_ID {
        return (Icon::Display, Tone::Secondary, "Only on this computer".to_owned());
    }
    let words = match calendar.account.trim() {
        "iCloud" => "Saves to iCloud".to_owned(),
        "" | "On My Mac" => "Saves to Apple Calendar".to_owned(),
        account => format!("Saves to {account}"),
    };
    (Icon::Cloud, Tone::Blue, words)
}

fn lengths(current: u64) -> Vec<u64> {
    let mut lengths = LENGTHS.to_vec();
    if !lengths.contains(&current) {
        lengths.push(current);
        lengths.sort_unstable();
    }
    lengths
}

fn length_label(minutes: u64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} hr"),
        (hours, minutes) => format!("{hours} hr {minutes} min"),
    }
}

fn calendar_label(calendar: &CalendarInfo) -> String {
    match calendar.account.trim() {
        "" => calendar.title.clone(),
        account if account == calendar.title => calendar.title.clone(),
        account => format!("{} \u{b7} {account}", calendar.title),
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn zone() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }

    fn calendars() -> Vec<CalendarInfo> {
        let calendar = |id: &str, title: &str| CalendarInfo {
            id: id.into(),
            title: title.into(),
            account: "iCloud".into(),
            color: None,
            writable: true,
        };
        vec![calendar("home", "Home"), calendar("work", "Work")]
    }

    fn starts(editor: &Editor) -> (Date, i8, i8) {
        (editor.date, editor.hour, editor.minute)
    }

    #[test]
    fn today_starts_at_next_half_hour() {
        let today = date(2026, 9, 30);
        let now = at(today, Time::new(9, 10, 0, 0).unwrap(), &zone());
        assert_eq!(starts(&Editor::new(now, &zone(), today)), (today, 9, 30));
        let late = at(today, Time::new(23, 45, 0, 0).unwrap(), &zone());
        assert_eq!(starts(&Editor::new(late, &zone(), today)), (date(2026, 10, 1), 0, 0));
    }

    #[test]
    fn other_days_start_in_the_morning() {
        let now = at(date(2026, 9, 30), Time::new(15, 10, 0, 0).unwrap(), &zone());
        assert_eq!(starts(&Editor::new(now, &zone(), date(2026, 10, 14))), (date(2026, 10, 14), MORNING, 0));
    }

    #[test]
    fn wheels_build_a_local_draft() {
        let today = date(2026, 9, 30);
        let mut editor = Editor::new(at(today, Time::new(9, 0, 0, 0).unwrap(), &zone()), &zone(), today);
        assert_eq!(
            editor.draft(&zone(), &calendars()).map(|draft| draft.title),
            Some("New event".into()),
            "no title is fine"
        );
        editor.change(Change::Title("  Dentist ".into()), &calendars());
        editor.change(Change::Dates, &calendars());
        editor.change(Change::Turn(1), &calendars());
        editor.change(Change::PickDate(2026, 10, 1), &calendars());
        editor.change(Change::Hour(14), &calendars());
        editor.change(Change::Minute(15), &calendars());
        editor.change(Change::Length(3), &calendars());
        editor.change(Change::Calendar(1), &calendars());
        let draft = editor.draft(&zone(), &calendars()).unwrap();
        assert_eq!(draft.title, "Dentist");
        assert_eq!(draft.calendar_id, "work");
        assert_eq!(local(draft.start, &zone()), date(2026, 10, 1).at(14, 15, 0, 0));
        assert_eq!(draft.end - draft.start, 60 * MINUTE_MS);
        assert_eq!((draft.location, draft.notes), (None, None), "notes and place are left as they were");
        assert_eq!(draft.url.as_deref(), Some(""));
        editor.change(Change::Link("zoom.us/j/42".into()), &calendars());
        editor.change(Change::Color(2), &calendars());
        let draft = editor.draft(&zone(), &calendars()).unwrap();
        assert_eq!((draft.url.as_deref(), editor.color()), (Some("https://zoom.us/j/42"), Some(PALETTE[1])));
        editor.change(Change::Color(0), &calendars());
        assert_eq!(editor.color(), None, "the first swatch is the calendar's own color");
        let purple = CalendarInfo { color: Some(Tone::Purple.color()), ..calendars()[0].clone() };
        assert!(!editor.swatches(&[purple]).contains(&Some(Tone::Purple)), "its twin isn't offered again");
    }

    #[test]
    fn any_future_date_can_be_picked() {
        let today = date(2026, 9, 30);
        let mut editor = Editor::new(at(today, Time::new(9, 0, 0, 0).unwrap(), &zone()), &zone(), today);
        editor.change(Change::Dates, &calendars());
        assert_eq!(editor.picking, Some(date(2026, 9, 1)));
        editor.change(Change::Turn(4), &calendars());
        assert_eq!(editor.picking, Some(date(2027, 1, 1)));
        editor.change(Change::PickDate(2027, 1, 31), &calendars());
        assert_eq!((editor.date, editor.picking), (date(2027, 1, 31), None), "picking a day closes the calendar");
        editor.change(Change::Dates, &calendars());
        editor.change(Change::Turn(1), &calendars());
        editor.change(Change::PickDate(2027, 2, 30), &calendars());
        assert_eq!(editor.date, date(2027, 1, 31), "February has no 30th");
        editor.change(Change::Dates, &calendars());
        assert_eq!(editor.picking, None, "the date row closes it again");
    }

    #[test]
    fn editing_keeps_odd_times() {
        let today = date(2026, 9, 30);
        let start = at(today, Time::new(10, 7, 0, 0).unwrap(), &zone());
        let meeting = Meeting {
            id: "e@1".into(),
            title: "Sync".into(),
            start,
            end: start + 50 * MINUTE_MS,
            all_day: false,
            location: String::new(),
            join: None,
            link: String::new(),
            calendar: "Work".into(),
            color: None,
            declined: false,
            calendar_id: "work".into(),
            editable: true,
        };
        let editor = Editor::editing(&meeting, &zone(), None);
        let draft = editor.draft(&zone(), &calendars()).unwrap();
        assert_eq!((draft.id.as_deref(), draft.start, draft.end), (Some("e@1"), meeting.start, meeting.end));
        assert_eq!(draft.calendar_id, "work");
        assert_eq!(lengths(50), [15, 30, 45, 50, 60, 90, 120, 180]);
    }

    #[test]
    fn form_names_the_calendar() {
        let apple = |account: &str| CalendarInfo { id: "x".into(), account: account.into(), ..CalendarInfo::default() };
        assert_eq!(where_it_goes(&apple("iCloud")), (Icon::Cloud, Tone::Blue, "Saves to iCloud".to_owned()));
        assert_eq!(where_it_goes(&apple("me@gmail.com")).2, "Saves to me@gmail.com");
        assert_eq!(where_it_goes(&apple("On My Mac")).2, "Saves to Apple Calendar");
        let (glyph, _, words) = where_it_goes(&local::calendar());
        assert_eq!((glyph, words.as_str()), (Icon::Display, "Only on this computer"));
    }

    #[test]
    fn lengths_snap_to_the_wheel() {
        assert_eq!(minute_index(7), 1);
        assert_eq!(minute_index(58), 11);
        assert_eq!(length_label(90), "1 hr 30 min");
        assert_eq!(length_label(45), "45 min");
    }
}
