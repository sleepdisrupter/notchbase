//! Saving and deleting events in EventKit.

use notchbase_sdk::protocol::EventDraft;
use objc2::rc::Retained;
use objc2_event_kit::{EKEvent, EKEventStore, EKSpan};
use objc2_foundation::{NSError, NSString, NSURL};

use super::read::{date, event_id};

const GONE: &str = "That event isn't in Apple Calendar anymore";
const NO_CALENDAR: &str = "There's no calendar to add it to";
const NEARBY_MS: u64 = 60_000;

pub(super) fn save(store: &EKEventStore, draft: &EventDraft) -> Result<(), String> {
    let event = match &draft.id {
        Some(id) => find(store, id).ok_or_else(|| GONE.to_owned())?,
        None => unsafe { EKEvent::eventWithEventStore(store) },
    };
    let calendar = unsafe { store.calendarWithIdentifier(&NSString::from_str(&draft.calendar_id)) }
        .or_else(|| unsafe { event.calendar() })
        .or_else(|| unsafe { store.defaultCalendarForNewEvents() })
        .ok_or_else(|| NO_CALENDAR.to_owned())?;
    let text = |value: &str| (!value.trim().is_empty()).then(|| NSString::from_str(value.trim()));
    let (location, notes) = (draft.location.as_deref().map(text), draft.notes.as_deref().map(text));
    unsafe {
        event.setCalendar(Some(&calendar));
        event.setTitle(Some(&NSString::from_str(draft.title.trim())));
        event.setAllDay(draft.all_day);
        event.setStartDate(Some(&date(draft.start)));
        event.setEndDate(Some(&date(draft.end.max(draft.start))));
        if let Some(location) = &location {
            event.setLocation(location.as_deref());
        }
        if let Some(notes) = &notes {
            event.setNotes(notes.as_deref());
        }
        if let Some(url) = &draft.url {
            let url = (!url.trim().is_empty()).then(|| NSURL::URLWithString(&NSString::from_str(url.trim()))).flatten();
            event.setURL(url.as_deref());
        }
        store.saveEvent_span_commit_error(&event, EKSpan::ThisEvent, true).map_err(|error| described(&error))
    }
}

pub(super) fn delete(store: &EKEventStore, id: &str) -> Result<(), String> {
    let event = find(store, id).ok_or_else(|| GONE.to_owned())?;
    unsafe { store.removeEvent_span_commit_error(&event, EKSpan::ThisEvent, true) }.map_err(|error| described(&error))
}

fn find(store: &EKEventStore, id: &str) -> Option<Retained<EKEvent>> {
    let (identifier, start) = id.rsplit_once('@')?;
    let start: u64 = start.parse().ok()?;
    let from = date(start.saturating_sub(NEARBY_MS));
    let to = date(start + NEARBY_MS);
    let predicate = unsafe { store.predicateForEventsWithStartDate_endDate_calendars(&from, &to, None) };
    let nearby = unsafe { store.eventsMatchingPredicate(&predicate) };
    nearby
        .iter()
        .find(|event| event_id(event) == id)
        .or_else(|| unsafe { store.eventWithIdentifier(&NSString::from_str(identifier)) })
}

fn described(error: &NSError) -> String {
    format!("Apple Calendar didn't save it: {}", error.localizedDescription())
}
