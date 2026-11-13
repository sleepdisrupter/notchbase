use std::cell::Cell;

use jiff::civil::{Time, date};
use jiff::tz::TimeZone;
use notchbase_sdk::protocol::{MonthNode, Node, Request, WeekNode};
use notchbase_sdk::testing::Harness;

use super::*;

const MIN: u64 = 60_000;
const LINK: &str = "test://team";

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(0) };
}

struct Fake;

impl Clock for Fake {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }

    fn zone() -> TimeZone {
        TimeZone::get("America/New_York").unwrap()
    }
}

type Tested = Calendar<Fake>;

fn clock(hour: i8, minute: i8) -> u64 {
    day::at(date(2026, 9, 30), Time::new(hour, minute, 0, 0).unwrap(), &Fake::zone())
}

fn at(now: u64) {
    NOW.with(|cell| cell.set(now));
}

const FEED: &str = "BEGIN:VCALENDAR
X-WR-CALNAME:Team
BEGIN:VEVENT
UID:standup
SUMMARY:Standup
DTSTART;TZID=America/New_York:20260930T100000
DTEND;TZID=America/New_York:20260930T101500
DESCRIPTION:https://meet.google.com/abc-defg-hij
END:VEVENT
BEGIN:VEVENT
UID:lunch
SUMMARY:Lunch
DTSTART;TZID=America/New_York:20261001T123000
DTEND;TZID=America/New_York:20261001T133000
END:VEVENT
END:VCALENDAR";

fn linked() -> Harness<Tested> {
    at(clock(9, 0));
    let mut harness = Harness::<Tested>::start_with([(prefs::SOURCE, "ical"), (prefs::LINK, LINK)]);
    let generation = harness.model().generation;
    harness.send(Msg::Loaded { link: LINK.into(), generation, result: Feed::parse(FEED), cached: false });
    harness
}

fn apple() -> Harness<Tested> {
    at(clock(9, 0));
    let mut harness = Harness::<Tested>::start_with([(prefs::SOURCE, "apple")]);
    let event = CalendarEvent {
        id: "sync@1".into(),
        title: "Sync".into(),
        start: clock(11, 0),
        end: clock(11, 30),
        calendar: "Work".into(),
        calendar_id: "work".into(),
        editable: true,
        ..CalendarEvent::default()
    };
    let work = CalendarInfo { id: "work".into(), title: "Work".into(), writable: true, ..CalendarInfo::default() };
    harness.calendar(CalendarFeed::Events { events: vec![event], calendars: vec![work] });
    harness
}

fn grid(harness: &mut Harness<Tested>) -> MonthNode {
    fn find(node: &Node) -> Option<MonthNode> {
        match node {
            Node::Month(month) => Some(month.clone()),
            other => other.children().into_iter().find_map(find),
        }
    }
    find(&harness.page().expect("a page")).expect("a month on the page")
}

#[test]
fn has_a_tab_not_a_tile() {
    let manifest = Tested::manifest();
    assert!(manifest.header_tab && manifest.page.as_deref() == Some("Calendar"));
    assert!(linked().tile().is_none());
}

#[test]
fn linked_feed_fills_month_and_day() {
    let mut harness = linked();
    harness.select("Month");
    assert!(harness.shows("September 2026") && harness.shows("Today"));
    assert!(harness.shows("Standup"));
    assert!(!harness.shows("Lunch"), "tomorrow's lunch waits for its own day");
    assert!(!harness.shows("Accounts"), "an iCal link can't be edited");
    assert!(harness.live().is_none(), "the standup is an hour away");
    let month = grid(&mut harness);
    assert_eq!((month.year, month.month, month.today, month.selected), (2026, 9, Some(30), Some(30)));
    assert_eq!(month.marks.iter().map(|mark| mark.day).collect::<Vec<_>>(), [30]);
    assert!(!month.monday_first, "a 12-hour clock starts the week on Sunday");
}

fn week_of(harness: &mut Harness<Tested>) -> WeekNode {
    fn find(node: &Node) -> Option<WeekNode> {
        match node {
            Node::Week(week) => Some(week.clone()),
            other => other.children().into_iter().find_map(find),
        }
    }
    find(&harness.page().expect("a page")).expect("a week on the page")
}

#[test]
fn days_view_is_the_default() {
    let mut harness = linked();
    assert!(harness.shows("Sep 30 \u{2013} Oct 3"));
    let week = week_of(&mut harness);
    let days: Vec<(u8, &str, bool, bool)> =
        week.days.iter().map(|day| (day.day, day.weekday.as_str(), day.today, day.selected)).collect();
    assert_eq!(
        days,
        [(30, "Wed", true, true), (1, "Thu", false, false), (2, "Fri", false, false), (3, "Sat", false, false)]
    );
    let chips = |day: usize| {
        week.days[day].events.iter().map(|event| (event.time.clone(), event.title.clone())).collect::<Vec<_>>()
    };
    assert_eq!(chips(0), [("10a".to_owned(), "Standup".to_owned())]);
    assert_eq!(chips(1), [("12:30p".to_owned(), "Lunch".to_owned())]);
    assert!(chips(2).is_empty());
    assert!(!harness.shows("No events") && !harness.shows("Today"), "the days aren't listed a second time");
}

#[test]
fn tapping_and_paging_days() {
    let mut harness = linked();
    harness.tap_week_day(2026, 10, 1);
    assert_eq!(week_of(&mut harness).days.iter().position(|day| day.selected), Some(1));
    harness.click_icon(Icon::ChevronRight, 0);
    assert!(harness.shows("Oct 4 \u{2013} Oct 7"));
    assert_eq!(week_of(&mut harness).days.iter().position(|day| day.selected), Some(1));
    harness.click("Today");
    assert!(harness.shows("Sep 30 \u{2013} Oct 3"));
    harness.click_icon(Icon::ChevronLeft, 0);
    assert!(harness.shows("Sep 26 \u{2013} Sep 29"));
}

#[test]
fn scrolling_pages_days_and_months() {
    let mut harness = linked();
    harness.scroll_calendar(1);
    assert!(harness.shows("Oct 4 \u{2013} Oct 7"));
    harness.scroll_calendar(-1);
    harness.scroll_calendar(-1);
    assert!(harness.shows("Sep 26 \u{2013} Sep 29"));
    harness.select("Month");
    harness.scroll_calendar(1);
    assert!(harness.shows("October 2026"));
}

#[test]
fn chosen_view_is_remembered() {
    let mut harness = linked();
    let saved = harness.select("Month").to_vec();
    assert_eq!(saved, [Request::SaveSetting { key: prefs::VIEW.into(), value: "month".into() }]);
    assert!(harness.shows("September 2026"));
    let later = Harness::<Tested>::start_with([(prefs::SOURCE, "ical"), (prefs::VIEW, "month")]);
    assert!(!later.model().prefs.week);
}

#[test]
fn tapping_an_event_opens_it() {
    let mut harness = linked();
    harness.tap_week_event("lunch@1790872200000");
    let selected = week_of(&mut harness).days.iter().position(|day| day.selected);
    assert_eq!(selected, Some(1), "a linked event can't be edited, so its day is selected");
    let standup = week_of(&mut harness).days[0].events[0].id.clone();
    let joined = harness.tap_week_event(&standup).to_vec();
    assert_eq!(joined, [Request::Open { target: "https://meet.google.com/abc-defg-hij".into() }]);
    let mut apple = apple();
    apple.tap_week_event("sync@1");
    assert!(apple.shows("Edit event"));
}

#[test]
fn picking_days_and_turning_months() {
    let mut harness = linked();
    harness.select("Month");
    harness.pick_day(12);
    assert!(harness.shows("Saturday, September 12") && harness.shows("No events"));
    harness.click_icon(Icon::ChevronRight, 0);
    assert!(harness.shows("October 2026") && harness.shows("Tomorrow") && harness.shows("Lunch"));
    assert_eq!(grid(&mut harness).today, None);
    harness.click("Today");
    assert!(harness.shows("September 2026") && harness.shows("Standup"));
}

#[test]
fn next_month_day_can_be_picked() {
    let mut harness = linked();
    harness.select("Month");
    harness.pick_date(2026, 10, 1);
    assert!(harness.shows("October 2026") && harness.shows("Lunch"), "it turns to October and shows that day");
    let month = grid(&mut harness);
    assert_eq!((month.month, month.selected), (10, Some(1)));
    harness.pick_date(2026, 9, 28);
    assert!(harness.shows("September 2026") && harness.shows("Monday, September 28"));
}

#[test]
fn meeting_shows_early_and_nudges() {
    let mut harness = linked();
    at(clock(9, 52));
    harness.send(Msg::Tick);
    assert!(harness.live().is_some());
    assert_eq!(harness.model().tick_every(), QUICK);
    let requests = harness.click_icon(Icon::Video, 0).to_vec();
    assert_eq!(requests, [Request::Open { target: "https://meet.google.com/abc-defg-hij".into() }]);
    at(clock(10, 0) + 5_000);
    let started = harness.send(Msg::Tick).to_vec();
    assert!(matches!(started.as_slice(), [Request::Notice { .. }]), "{started:?}");
    assert!(harness.send(Msg::Tick).is_empty(), "only once");
    at(clock(10, 20));
    harness.send(Msg::Tick);
    assert!(harness.live().is_none());
}

#[test]
fn hiding_lasts_for_the_meeting() {
    let mut harness = linked();
    at(clock(9, 55));
    harness.send(Msg::Tick);
    harness.click_icon(Icon::Xmark, 0);
    assert!(harness.live().is_none());
    assert!(harness.shows("Standup"), "it's still on the page");
}

#[test]
fn no_link_points_to_settings() {
    at(clock(9, 0));
    let mut harness = Harness::<Tested>::start_with([(prefs::SOURCE, "ical")]);
    assert!(harness.shows("Add a calendar to see your events"));
    assert_eq!(harness.click("Open Settings").to_vec(), [Request::OpenSettings]);
}

#[test]
fn setup_card_can_be_dismissed() {
    at(clock(9, 0));
    let mut harness = Harness::<Tested>::start_with([(prefs::SOURCE, "ical")]);
    let requests = harness.click_icon(Icon::Xmark, 0).to_vec();
    assert_eq!(requests, [Request::SaveSetting { key: prefs::SKIPPED_SETUP.into(), value: true.into() }]);
    assert!(!harness.shows("Add a calendar to see your events"));
    let mut later = Harness::<Tested>::start_with([
        (prefs::SOURCE, Value::from("ical")),
        (prefs::SKIPPED_SETUP, Value::from(true)),
    ]);
    assert!(!later.shows("Add a calendar to see your events"), "it stays dismissed after a restart");
    assert!(later.shows("Sep 30 \u{2013} Oct 3"));
}

#[test]
fn apple_events_can_be_added() {
    let mut harness = apple();
    harness.select("Month");
    assert!(harness.shows("Sync") && harness.shows("Accounts"));
    harness.click_icon(Icon::Plus, 0);
    assert!(harness.shows("New event"));
    harness.type_text("New event", "Dentist");
    harness.send(Msg::Editor(Change::Hour(15)));
    harness.send(Msg::Editor(Change::Minute(30)));
    let saved = harness.click("Save").to_vec();
    let [Request::SaveEvent { event }, Request::Hold { held: false }] = saved.as_slice() else { panic!("{saved:?}") };
    assert_eq!((event.id.as_deref(), event.title.as_str(), event.calendar_id.as_str()), (None, "Dentist", "work"));
    assert_eq!(event.start, clock(15, 30));
    assert_eq!(event.end - event.start, 30 * MIN);
    assert!(harness.shows("Sync"), "back to the agenda");
}

#[test]
fn new_event_lands_on_picked_day() {
    let mut harness = apple();
    harness.select("Month");
    harness.click_icon(Icon::ChevronRight, 0);
    harness.pick_day(14);
    harness.click_icon(Icon::Plus, 0);
    harness.type_text("New event", "Flight");
    let saved = harness.click("Save").to_vec();
    let [Request::SaveEvent { event }, Request::Hold { held: false }] = saved.as_slice() else { panic!("{saved:?}") };
    let start = day::local(event.start, &Fake::zone());
    assert_eq!(start, date(2026, 10, 14).at(9, 0, 0, 0));
}

#[test]
fn apple_asked_for_whole_month() {
    let mut harness = apple();
    harness.click_icon(Icon::ChevronRight, 0);
    let (from, to) = harness.model().reach(&Fake::zone());
    assert!(from <= clock(0, 0), "today stays covered for the live activity");
    assert!(to >= day::midnight(date(2026, 11, 1), &Fake::zone()), "and all of October");
}

#[test]
fn apple_events_move_and_delete() {
    let mut harness = apple();
    harness.select("Month");
    harness.click("Edit");
    assert!(harness.shows("Edit event"));
    harness.send(Msg::Editor(Change::Hour(13)));
    let saved = harness.click("Save").to_vec();
    let [Request::SaveEvent { event }, Request::Hold { held: false }] = saved.as_slice() else { panic!("{saved:?}") };
    assert_eq!((event.id.as_deref(), event.start, event.end), (Some("sync@1"), clock(13, 0), clock(13, 30)));
    harness.click("Edit");
    assert_eq!(
        harness.click("Delete").to_vec(),
        [Request::DeleteEvent { id: "sync@1".into() }, Request::Hold { held: false }]
    );
}

#[test]
fn asks_for_calendar_access() {
    let mut harness = apple();
    harness.select("Month");
    assert_eq!(harness.click("Accounts").to_vec(), [Request::Open { target: ACCOUNTS.into() }]);
    harness.calendar(CalendarFeed::Denied);
    assert!(harness.shows("can't see your calendars"));
    assert_eq!(harness.click("Open Privacy Settings").to_vec(), [Request::Open { target: PRIVACY.into() }]);
    assert!(harness.model().editor.is_none() && !harness.shows("Sync"));
}

#[test]
fn no_apple_keeps_events_local() {
    let mut harness = linked();
    harness.select("Month");
    harness.click_icon(Icon::Plus, 0);
    assert!(harness.shows("Only on this computer"));
    harness.type_text("New event", "Gym");
    harness.send(Msg::Editor(Change::Hour(18)));
    assert!(harness.click("Save").iter().all(|request| !matches!(request, Request::SaveEvent { .. })));
    assert!(harness.shows("Gym") && harness.shows("Standup"), "it joins the linked meetings");
    harness.click_nth("Edit", 0);
    harness.type_text("New event", "Gym, legs");
    harness.click("Save");
    assert!(harness.shows("Gym, legs"));
    harness.click_nth("Edit", 0);
    harness.click("Delete");
    assert!(!harness.shows("Gym"));
}

#[test]
fn event_color_and_service_link() {
    let mut harness = linked();
    harness.click_icon(Icon::Plus, 0);
    harness.type_text("New event", "Gym");
    harness.type_text("Link, like a Zoom or FaceTime invite", "zoom.us/j/9");
    harness.pick_color(3);
    harness.click("Save");
    let gym = week_of(&mut harness).days[0].events.iter().find(|event| event.title == "Gym").cloned().unwrap();
    assert_eq!(gym.color, Some(Tone::Yellow.color()));
    harness.select("Month");
    assert!(harness.shows("Zoom") && harness.shows("Meet"), "each meeting's button says where it goes");
    harness.click_nth("Edit", 0);
    harness.pick_color(0);
    harness.click("Save");
    assert!(gym_is_back_to_its_calendar_color(&mut harness), "the calendar's own color comes back");
}

fn gym_is_back_to_its_calendar_color(harness: &mut Harness<Tested>) -> bool {
    harness.select("Days");
    week_of(harness).days[0]
        .events
        .iter()
        .any(|event| event.title == "Gym" && event.color != Some(Tone::Yellow.color()))
}

#[test]
fn events_without_any_calendar() {
    at(clock(9, 0));
    let mut harness = Harness::<Tested>::start_with([(prefs::SOURCE, "ical")]);
    harness.click_icon(Icon::Plus, 0);
    harness.type_text("New event", "Dentist");
    harness.click("Save");
    assert!(harness.shows("Dentist") && harness.shows("Add a calendar to see your events"));
    harness.click_icon(Icon::Plus, 0);
    harness.click("Save");
    assert!(harness.shows("New event"), "an event without a title is still saved, as New event");
}

#[test]
fn local_event_can_move_to_apple() {
    let mut harness = linked();
    harness.select("Month");
    harness.click_icon(Icon::Plus, 0);
    harness.type_text("New event", "Gym");
    harness.click("Save");
    harness.change_settings([(prefs::SOURCE, "apple")]);
    let work = CalendarInfo {
        id: "work".into(),
        title: "Work".into(),
        account: "iCloud".into(),
        writable: true,
        ..CalendarInfo::default()
    };
    harness.calendar(CalendarFeed::Events { events: Vec::new(), calendars: vec![work] });
    harness.click_icon(Icon::Plus, 0);
    assert!(harness.shows("Saves to iCloud"));
    harness.click_icon(Icon::Xmark, 0);
    harness.click("Edit");
    harness.select("Work \u{b7} iCloud");
    let moved = harness.click("Save").to_vec();
    let [Request::SaveEvent { event }, Request::Hold { held: false }] = moved.as_slice() else { panic!("{moved:?}") };
    assert_eq!((event.id.as_deref(), event.calendar_id.as_str(), event.title.as_str()), (None, "work", "Gym"));
    assert!(!harness.shows("Gym"), "it left this computer once Apple Calendar has it");
}

#[test]
fn link_switch_drops_editor() {
    let mut harness = apple();
    harness.click_icon(Icon::Plus, 0);
    harness.change_settings([(prefs::SOURCE, "ical")]);
    assert!(harness.model().editor.is_none());
    assert!(harness.shows("Add a calendar to see your events"));
}

#[test]
fn linked_feed_refreshes_on_view() {
    let mut harness = linked();
    harness.wait_for(|model| model.fetching == 0);
    assert_eq!(harness.model().refresh_every(), REFRESH);
    let before = harness.model().generation;
    harness.notch(on(ID));
    assert_eq!(harness.model().generation, before + 1, "opening the Calendar fetches straight away");
    assert_eq!(harness.model().refresh_every(), WATCHED, "and keeps checking often while it's on screen");
    harness.notch(on(ID));
    assert_eq!(harness.model().generation, before + 1, "not again while nothing changed");
    harness.wait_for(|model| model.fetching == 0);
    harness.notch(NotchState::default());
    harness.notch(on("media"));
    assert_eq!(harness.model().generation, before + 2, "opening the notch anywhere checks too");
    assert_eq!(harness.model().refresh_every(), REFRESH);
}

fn on(page: &str) -> NotchState {
    NotchState { open: true, page: Some(page.into()), ..NotchState::default() }
}

#[test]
fn notch_stays_open_while_editing() {
    let mut harness = linked();
    assert_eq!(harness.click_icon(Icon::Plus, 0).to_vec(), [Request::Hold { held: true }]);
    assert!(harness.notch(on(ID)).is_empty());
    assert_eq!(harness.notch(on("media")).to_vec(), [Request::Hold { held: false }], "another tab lets it go");
    assert_eq!(harness.notch(on(ID)).to_vec(), [Request::Hold { held: true }], "coming back holds it again");
    assert!(harness.shows("New event"), "the unfinished event is still there");
    assert_eq!(harness.click_icon(Icon::Xmark, 0).to_vec(), [Request::Hold { held: false }]);
    harness.click_icon(Icon::Plus, 0);
    harness.type_text("New event", "Gym");
    let saved = harness.click("Save").to_vec();
    assert!(saved.contains(&Request::Hold { held: false }), "saving lets it go: {saved:?}");
}
