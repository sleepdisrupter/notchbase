//! Reading events and calendars out of EventKit.

use notchbase_sdk::protocol::{CalendarEvent, CalendarInfo, Color};
use objc2_app_kit::{NSColor, NSColorSpace};
use objc2_event_kit::{EKCalendar, EKEntityType, EKEvent, EKEventStatus, EKEventStore, EKParticipantStatus};
use objc2_foundation::NSDate;

pub(super) fn events(store: &EKEventStore, from_ms: u64, to_ms: u64) -> Vec<CalendarEvent> {
    let (from, to) = (date(from_ms), date(to_ms));
    let predicate = unsafe { store.predicateForEventsWithStartDate_endDate_calendars(&from, &to, None) };
    let found = unsafe { store.eventsMatchingPredicate(&predicate) };
    let mut events: Vec<CalendarEvent> = found.iter().filter_map(|event| event_of(&event)).collect();
    events.sort_by(|a, b| (a.start, &a.title).cmp(&(b.start, &b.title)));
    events
}

pub(super) fn calendars(store: &EKEventStore) -> Vec<CalendarInfo> {
    let found = unsafe { store.calendarsForEntityType(EKEntityType::Event) };
    let mut calendars: Vec<CalendarInfo> = found.iter().map(|calendar| calendar_of(&calendar)).collect();
    calendars.sort_by(|a, b| (&a.account, &a.title).cmp(&(&b.account, &b.title)));
    calendars
}

pub(super) fn event_id(event: &EKEvent) -> String {
    let identifier = unsafe { event.eventIdentifier() }.map(|id| id.to_string()).unwrap_or_default();
    format!("{identifier}@{}", millis(&*unsafe { event.startDate() }))
}

pub(super) fn date(ms: u64) -> objc2::rc::Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(ms as f64 / 1000.0)
}

pub(super) fn millis(date: &NSDate) -> u64 {
    (date.timeIntervalSince1970() * 1000.0).max(0.0) as u64
}

fn event_of(event: &EKEvent) -> Option<CalendarEvent> {
    if unsafe { event.status() } == EKEventStatus::Canceled {
        return None;
    }
    let (starts, ends) = unsafe { (event.startDate(), event.endDate()) };
    let calendar = unsafe { event.calendar() };
    let info = calendar.as_ref().map(|calendar| calendar_of(calendar)).unwrap_or_default();
    Some(CalendarEvent {
        id: event_id(event),
        title: unsafe { event.title() }.to_string(),
        start: millis(&starts),
        end: millis(&ends),
        all_day: unsafe { event.isAllDay() },
        location: unsafe { event.location() }.map(|text| text.to_string()).unwrap_or_default(),
        notes: unsafe { event.notes() }.map(|text| text.to_string()).unwrap_or_default(),
        url: unsafe { event.URL() }.and_then(|url| url.absoluteString()).map(|url| url.to_string()).unwrap_or_default(),
        calendar: info.title,
        color: info.color,
        declined: declined(event),
        calendar_id: info.id,
        editable: info.writable,
    })
}

fn calendar_of(calendar: &EKCalendar) -> CalendarInfo {
    CalendarInfo {
        id: unsafe { calendar.calendarIdentifier() }.to_string(),
        title: unsafe { calendar.title() }.to_string(),
        account: unsafe { calendar.source() }.map(|source| unsafe { source.title() }.to_string()).unwrap_or_default(),
        color: srgb(&*unsafe { calendar.color() }),
        writable: unsafe { calendar.allowsContentModifications() },
    }
}

fn declined(event: &EKEvent) -> bool {
    unsafe { event.attendees() }.is_some_and(|attendees| {
        attendees.iter().any(|attendee| unsafe {
            attendee.isCurrentUser() && attendee.participantStatus() == EKParticipantStatus::Declined
        })
    })
}

fn srgb(color: &NSColor) -> Option<Color> {
    let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Color::rgb(channel(srgb.redComponent()), channel(srgb.greenComponent()), channel(srgb.blueComponent())))
}
