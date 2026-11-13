//! The calendar page: the days, the month and the editor.

use jiff::tz::TimeZone;
use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use crate::Msg;
use crate::browse::Browse;
use crate::day::{self, Days, shift};
use crate::meeting::Meeting;
use crate::status::Status;

const DOT: f32 = 8.0;

pub struct Agenda<'a> {
    pub days: Days,
    pub now: u64,
    pub meetings: &'a [Meeting],
    pub status: Status,
    pub problems: Vec<String>,
    pub all_day: bool,
    pub apple: bool,
    pub browse: Browse,
    pub monday_first: bool,
    pub zone: &'a TimeZone,
    pub week: bool,
    pub clock_24h: bool,
}

pub fn page(agenda: &Agenda) -> Element<Msg> {
    let mut parts: Vec<Element<Msg>> = vec![controls(agenda), title(agenda)];
    match agenda.week {
        true => parts.push(days_of(agenda)),
        false => parts.extend([
            agenda
                .browse
                .grid(agenda.days.today, agenda.meetings, agenda.monday_first, agenda.zone)
                .on_select(Msg::Pick)
                .on_scroll(Msg::Shift)
                .into(),
            day_header(agenda),
        ]),
    }
    if let Some(explained) = explain(agenda.status) {
        parts.push(explained);
    }
    parts
        .extend(agenda.problems.iter().map(|problem| text(problem.clone()).caption().tone(Tone::Orange).wrap().into()));
    if !agenda.week && agenda.status != Status::Loading {
        let (from, to) = agenda.browse.selected_range(agenda.zone);
        let day = shown(agenda, from, to);
        parts.push(match day.is_empty() {
            true => text("No events").secondary().into(),
            false => list(day.into_iter().map(|meeting| row_of(meeting, agenda))),
        });
    }
    column(parts).spacing(10.0).into()
}

fn controls(agenda: &Agenda) -> Element<Msg> {
    let view = segmented(["Days", "Month"], if agenda.week { 0 } else { 1 }).on_select(Msg::View);
    let mut items: Vec<Element<Msg>> = vec![view.into(), spacer()];
    if agenda.browse != Browse::at(agenda.days.today) {
        items.push(button("Today").plain().on_press(Msg::Today).into());
    }
    if agenda.week {
        items.push(button("").icon(Icon::Plus).on_press(Msg::New).into());
    }
    items.push(button("").icon(Icon::ChevronLeft).on_press(Msg::Shift(-1)).into());
    items.push(button("").icon(Icon::ChevronRight).on_press(Msg::Shift(1)).into());
    row(items).spacing(6.0).into()
}

fn title(agenda: &Agenda) -> Element<Msg> {
    let words = match agenda.week {
        true => agenda.browse.days_title(agenda.days.today),
        false => agenda.browse.title(),
    };
    text(words).title().into()
}

fn days_of(agenda: &Agenda) -> Element<Msg> {
    let days = agenda.browse.days(agenda.days.today).into_iter().map(|date| {
        let (from, to) = (day::midnight(date, agenda.zone), day::midnight(shift(date, 1), agenda.zone));
        let events = shown(agenda, from, to)
            .into_iter()
            .filter(|meeting| !meeting.declined)
            .map(|meeting| WeekEvent {
                id: meeting.id.clone(),
                time: if meeting.all_day {
                    String::new()
                } else {
                    day::short_time(meeting.start, agenda.zone, agenda.clock_24h)
                },
                title: meeting.title.clone(),
                color: meeting.color,
            })
            .collect();
        WeekDay {
            year: i32::from(date.year()),
            month: date.month() as u8,
            day: date.day() as u8,
            weekday: day::short_weekday(date).to_owned(),
            today: date == agenda.days.today,
            selected: date == agenda.browse.selected,
            events,
        }
    });
    week(days).on_day(Msg::Pick).on_event(Msg::Tapped).on_scroll(Msg::Shift).into()
}

fn day_header(agenda: &Agenda) -> Element<Msg> {
    let selected = agenda.browse.selected;
    let title = match selected {
        today if today == agenda.days.today => "Today".to_owned(),
        tomorrow if tomorrow == shift(agenda.days.today, 1) => "Tomorrow".to_owned(),
        other => day::heading(other),
    };
    let mut items: Vec<Element<Msg>> = vec![text(title).callout().into(), spacer()];
    if agenda.apple {
        items.push(button("Accounts").plain().on_press(Msg::Accounts).into());
    }
    items.push(button("").icon(Icon::Plus).on_press(Msg::New).into());
    row(items).spacing(8.0).into()
}

fn explain(status: Status) -> Option<Element<Msg>> {
    let (title, detail, action) = match status {
        Status::Ready => return None,
        Status::Setup => return Some(setup()),
        Status::Loading => ("Loading your calendar\u{2026}", "", None),
        Status::Waiting => {
            ("Waiting for access", "Allow Calendars when notchbase asks, or in Settings under Permissions.", None)
        }
        Status::Denied => (
            "notchbase can't see your calendars",
            "Allow it in System Settings, under Privacy & Security, Calendars.",
            Some(("Open Privacy Settings", Msg::Privacy)),
        ),
        Status::Unavailable => {
            ("Apple Calendar needs a Mac", "Use an iCal link instead.", Some(("Open Settings", Msg::OpenSettings)))
        }
    };
    let mut lines: Vec<Element<Msg>> = vec![text(title).callout().into()];
    if !detail.is_empty() {
        lines.push(text(detail).caption().secondary().wrap().into());
    }
    if let Some((label, msg)) = action {
        lines.push(button(label).on_press(msg).into());
    }
    Some(card(column(lines).spacing(6.0)))
}

fn setup() -> Element<Msg> {
    let line = row![
        text("Add a calendar to see your events").subheadline(),
        spacer(),
        button("Open Settings").on_press(Msg::OpenSettings),
        button("").icon(Icon::Xmark).on_press(Msg::SkipSetup)
    ];
    card(line.spacing(6.0)).padding(10.0)
}

fn shown<'a>(agenda: &Agenda<'a>, from: u64, to: u64) -> Vec<&'a Meeting> {
    agenda
        .meetings
        .iter()
        .filter(|meeting| meeting.overlaps(from, to) && (agenda.all_day || !meeting.all_day))
        .collect()
}

fn row_of(meeting: &Meeting, agenda: &Agenda) -> Element<Msg> {
    let over = !meeting.all_day && meeting.end <= agenda.now;
    let on = !meeting.all_day && meeting.start <= agenda.now && agenda.now < meeting.end;
    let tone = if over || meeting.declined { Tone::Tertiary } else { Tone::Primary };
    let color = meeting.color.unwrap_or_else(|| Tone::Blue.into());
    let mut detail: Vec<Element<Msg>> = vec![when(meeting)];
    if on {
        detail.push(text("Now").caption().tone(Tone::Green).into());
    }
    if !meeting.location.is_empty() && meeting.join.as_deref() != Some(meeting.location.as_str()) {
        detail.push(text(format!("\u{b7} {}", meeting.location)).caption().secondary().into());
    }
    let words = column![text(meeting.title.clone()).callout().tone(tone), row(detail).spacing(4.0)].spacing(2.0);
    let mut items: Vec<Element<Msg>> = vec![dot(color, DOT), words.into(), spacer()];
    if let Some(link) = meeting.join.clone().filter(|_| !over) {
        let service = crate::join::service(&link);
        items.push(button(service.name).icon(service.icon).tinted(service.color).on_press(Msg::Join(link)).into());
    }
    if meeting.editable {
        items.push(button("Edit").plain().on_press(Msg::Edit(meeting.id.clone())).into());
    }
    keyed(meeting.id.clone(), row(items).spacing(10.0))
}

fn when(meeting: &Meeting) -> Element<Msg> {
    if meeting.all_day {
        return text("All day").caption().secondary().into();
    }
    let clock = |at: u64| timer(at).clock().style(TextStyle::Caption).tone(Tone::Secondary);
    row![clock(meeting.start), text("\u{2013}").caption().secondary(), clock(meeting.end)].spacing(3.0).into()
}
