//! One event and its repeats, as the feed describes them.

use jiff::SignedDuration;

use super::component::Component;
use super::recur::Rule;
use super::stamp::Stamp;

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub uid: String,
    pub title: String,
    pub location: String,
    pub notes: String,
    pub url: String,
    pub conference: String,
    pub start: Stamp,
    pub end: Option<Stamp>,
    pub length: Option<SignedDuration>,
    pub rule: Option<Rule>,
    pub extra: Vec<Stamp>,
    pub skipped: Vec<Stamp>,
    pub moved_from: Option<Stamp>,
    pub cancelled: bool,
    pub declined: bool,
}

impl Series {
    pub fn read(event: &Component, owner: Option<&str>) -> Option<Series> {
        let start = event.get("DTSTART").and_then(Stamp::from_property)?;
        Some(Series {
            uid: event.text("UID"),
            title: event.text("SUMMARY").trim().to_owned(),
            location: event.text("LOCATION"),
            notes: event.text("DESCRIPTION"),
            url: event.get("URL").map(|prop| prop.value.trim().to_owned()).unwrap_or_default(),
            conference: event.get("X-GOOGLE-CONFERENCE").map(|prop| prop.value.trim().to_owned()).unwrap_or_default(),
            start,
            end: event.get("DTEND").and_then(Stamp::from_property),
            length: event.get("DURATION").and_then(|prop| duration(&prop.value)),
            rule: event.get("RRULE").and_then(|prop| Rule::parse(&prop.value)),
            extra: event.all("RDATE").flat_map(Stamp::list).collect(),
            skipped: event.all("EXDATE").flat_map(Stamp::list).collect(),
            moved_from: event.get("RECURRENCE-ID").and_then(Stamp::from_property),
            cancelled: event.get("STATUS").is_some_and(|prop| prop.value.trim().eq_ignore_ascii_case("CANCELLED")),
            declined: owner.is_some_and(|owner| declined_by(event, owner)),
        })
    }
}

fn declined_by(event: &Component, owner: &str) -> bool {
    event.all("ATTENDEE").any(|attendee| {
        let address = attendee.value.trim();
        let address =
            address.get(..7).filter(|scheme| scheme.eq_ignore_ascii_case("mailto:")).map_or(address, |_| &address[7..]);
        address.eq_ignore_ascii_case(owner)
            && attendee.param("PARTSTAT").is_some_and(|status| status.eq_ignore_ascii_case("DECLINED"))
    })
}

pub fn duration(text: &str) -> Option<SignedDuration> {
    let text = text.trim();
    let (sign, rest) = match text.as_bytes().first()? {
        b'-' => (-1, &text[1..]),
        b'+' => (1, &text[1..]),
        _ => (1, text),
    };
    let rest = rest.strip_prefix(['P', 'p'])?;
    let mut seconds: i64 = 0;
    let mut number = String::new();
    for c in rest.chars() {
        match c.to_ascii_uppercase() {
            digit if digit.is_ascii_digit() => number.push(digit),
            'T' => {}
            unit => {
                let value: i64 = number.parse().ok()?;
                number.clear();
                seconds += value
                    * match unit {
                        'W' => 7 * 86_400,
                        'D' => 86_400,
                        'H' => 3_600,
                        'M' => 60,
                        'S' => 1,
                        _ => return None,
                    };
            }
        }
    }
    number.is_empty().then(|| SignedDuration::from_secs(sign * seconds))
}

#[cfg(test)]
mod tests {
    use super::super::lines::properties;
    use super::*;

    fn event(text: &str) -> Component {
        Component::tree(properties(text)).descendants("VEVENT").into_iter().next().cloned().unwrap()
    }

    #[test]
    fn durations_read_all_units() {
        assert_eq!(duration("PT1H30M"), Some(SignedDuration::from_mins(90)));
        assert_eq!(duration("P1W"), Some(SignedDuration::from_hours(24 * 7)));
        assert_eq!(duration("P1DT2H"), Some(SignedDuration::from_hours(26)));
        assert_eq!(duration("-PT15M"), Some(SignedDuration::from_mins(-15)));
        assert_eq!(duration("1H"), None);
        assert_eq!(duration("PT5"), None);
    }

    #[test]
    fn declined_from_own_attendee_line() {
        let text = "BEGIN:VEVENT\nUID:a\nDTSTART:20260930T090000Z\nATTENDEE;CN=Other;PARTSTAT=DECLINED:mailto:other@example.com\nATTENDEE;PARTSTAT=ACCEPTED:mailto:me@example.com\nEND:VEVENT";
        assert!(!Series::read(&event(text), Some("me@example.com")).unwrap().declined);
        let mine = text.replace("ACCEPTED:mailto:me", "DECLINED:MAILTO:Me");
        assert!(Series::read(&event(&mine), Some("me@example.com")).unwrap().declined);
        assert!(!Series::read(&event(&mine), None).unwrap().declined);
    }

    #[test]
    fn event_without_start_is_skipped() {
        assert!(Series::read(&event("BEGIN:VEVENT\nUID:a\nSUMMARY:No time\nEND:VEVENT"), None).is_none());
    }
}
