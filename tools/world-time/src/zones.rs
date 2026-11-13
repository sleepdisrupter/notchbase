//! Turns what someone typed into a time zone: database names, cities, big cities that share a zone,
//! and abbreviations.

use jiff::tz::{self, TimeZone};

const LOCAL: &str = "Local";
const PREFIX_MIN: usize = 3; // shorter starts match too much

// Common abbreviations and nicknames.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("utc", "UTC"),
    ("gmt", "Etc/GMT"),
    ("pt", "America/Los_Angeles"),
    ("pst", "America/Los_Angeles"),
    ("pdt", "America/Los_Angeles"),
    ("mt", "America/Denver"),
    ("mst", "America/Denver"),
    ("mdt", "America/Denver"),
    ("ct", "America/Chicago"),
    ("cst", "America/Chicago"),
    ("cdt", "America/Chicago"),
    ("et", "America/New_York"),
    ("est", "America/New_York"),
    ("edt", "America/New_York"),
    ("bst", "Europe/London"),
    ("cet", "Europe/Paris"),
    ("cest", "Europe/Paris"),
    ("eet", "Europe/Athens"),
    ("eest", "Europe/Athens"),
    ("ist", "Asia/Kolkata"),
    ("sgt", "Asia/Singapore"),
    ("hkt", "Asia/Hong_Kong"),
    ("jst", "Asia/Tokyo"),
    ("kst", "Asia/Seoul"),
    ("aest", "Australia/Sydney"),
    ("aedt", "Australia/Sydney"),
    ("nzst", "Pacific/Auckland"),
    ("nzdt", "Pacific/Auckland"),
    ("nyc", "America/New_York"),
    ("sf", "America/Los_Angeles"),
    ("la", "America/Los_Angeles"),
    ("dc", "America/New_York"),
    ("uk", "Europe/London"),
    ("uae", "Asia/Dubai"),
];

// Big cities that use a zone named after another city.
const NEARBY: &[(&str, &str)] = &[
    ("san francisco", "America/Los_Angeles"),
    ("san jose", "America/Los_Angeles"),
    ("silicon valley", "America/Los_Angeles"),
    ("san diego", "America/Los_Angeles"),
    ("seattle", "America/Los_Angeles"),
    ("portland", "America/Los_Angeles"),
    ("las vegas", "America/Los_Angeles"),
    ("salt lake city", "America/Denver"),
    ("dallas", "America/Chicago"),
    ("houston", "America/Chicago"),
    ("austin", "America/Chicago"),
    ("minneapolis", "America/Chicago"),
    ("boston", "America/New_York"),
    ("washington", "America/New_York"),
    ("washington dc", "America/New_York"),
    ("philadelphia", "America/New_York"),
    ("atlanta", "America/New_York"),
    ("miami", "America/New_York"),
    ("montreal", "America/Toronto"),
    ("ottawa", "America/Toronto"),
    ("calgary", "America/Edmonton"),
    ("hawaii", "Pacific/Honolulu"),
    ("alaska", "America/Anchorage"),
    ("mexico", "America/Mexico_City"),
    ("rio", "America/Sao_Paulo"),
    ("rio de janeiro", "America/Sao_Paulo"),
    ("brasilia", "America/Sao_Paulo"),
    ("edinburgh", "Europe/London"),
    ("manchester", "Europe/London"),
    ("barcelona", "Europe/Madrid"),
    ("milan", "Europe/Rome"),
    ("munich", "Europe/Berlin"),
    ("frankfurt", "Europe/Berlin"),
    ("hamburg", "Europe/Berlin"),
    ("geneva", "Europe/Zurich"),
    ("st petersburg", "Europe/Moscow"),
    ("kiev", "Europe/Kyiv"),
    ("cape town", "Africa/Johannesburg"),
    ("tel aviv", "Asia/Jerusalem"),
    ("abu dhabi", "Asia/Dubai"),
    ("mumbai", "Asia/Kolkata"),
    ("delhi", "Asia/Kolkata"),
    ("new delhi", "Asia/Kolkata"),
    ("bangalore", "Asia/Kolkata"),
    ("bengaluru", "Asia/Kolkata"),
    ("hyderabad", "Asia/Kolkata"),
    ("chennai", "Asia/Kolkata"),
    ("india", "Asia/Kolkata"),
    ("islamabad", "Asia/Karachi"),
    ("lahore", "Asia/Karachi"),
    ("hanoi", "Asia/Bangkok"),
    ("saigon", "Asia/Ho_Chi_Minh"),
    ("kuala lumpur", "Asia/Kuala_Lumpur"),
    ("beijing", "Asia/Shanghai"),
    ("shenzhen", "Asia/Shanghai"),
    ("china", "Asia/Shanghai"),
    ("osaka", "Asia/Tokyo"),
    ("kyoto", "Asia/Tokyo"),
    ("japan", "Asia/Tokyo"),
    ("canberra", "Australia/Sydney"),
    ("wellington", "Pacific/Auckland"),
];

#[derive(Clone, Debug)]
pub struct Place {
    pub name: String,
    pub zone: Option<TimeZone>,
}

impl Place {
    pub fn named(typed: &str) -> Self {
        Self { name: title(typed), zone: find(typed) }
    }

    pub fn local(zone: TimeZone) -> Self {
        let name = zone.iana_name().map_or_else(|| LOCAL.to_owned(), title);
        Self { name, zone: Some(zone) }
    }
}

/// Tries abbreviations and nearby cities first, then a database name, then a city, then the
/// start of a city's name.
pub fn find(typed: &str) -> Option<TimeZone> {
    let wanted = plain(typed);
    if wanted.is_empty() {
        return None;
    }
    if let Some(name) = alias(&wanted) {
        return tz::db().get(name).ok();
    }
    let names: Vec<String> = tz::db().available().map(|name| name.as_str().to_owned()).collect();
    let first = |fits: &dyn Fn(&str) -> bool| names.iter().find(|name| fits(name));
    let near = || first(&|name| wanted.len() >= PREFIX_MIN && city(name).starts_with(&wanted));
    let name = first(&|name| plain(name) == wanted).or_else(|| first(&|name| city(name) == wanted)).or_else(near)?;
    tz::db().get(name).ok()
}

/// The name to show: the city part of a database name, or capitals for an abbreviation.
pub fn title(typed: &str) -> String {
    let typed = typed.trim();
    let name = typed.rsplit('/').next().unwrap_or(typed).replace('_', " ");
    if abbreviation(&plain(&name)).is_some() {
        return name.to_uppercase();
    }
    if name.chars().any(char::is_uppercase) {
        return name;
    }
    name.split_whitespace().map(capitalized).collect::<Vec<_>>().join(" ")
}

fn alias(wanted: &str) -> Option<&'static str> {
    abbreviation(wanted).or_else(|| NEARBY.iter().find(|(known, _)| *known == wanted).map(|(_, name)| *name))
}

fn abbreviation(wanted: &str) -> Option<&'static str> {
    ABBREVIATIONS.iter().find(|(known, _)| *known == wanted).map(|(_, name)| *name)
}

fn city(name: &str) -> String {
    plain(name.rsplit('/').next().unwrap_or(name))
}

fn plain(text: &str) -> String {
    let spaced: String = text
        .chars()
        .filter(|letter| !matches!(letter, '.' | ','))
        .map(|letter| if matches!(letter, '_' | '-') { ' ' } else { letter })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(typed: &str) -> Option<String> {
        find(typed).and_then(|zone| zone.iana_name().map(str::to_owned))
    }

    #[test]
    fn finds_cities_however_typed() {
        assert_eq!(zone("London").as_deref(), Some("Europe/London"));
        assert_eq!(zone("  new york ").as_deref(), Some("America/New_York"));
        assert_eq!(zone("Europe/Paris").as_deref(), Some("Europe/Paris"));
        assert_eq!(zone("America/New York").as_deref(), Some("America/New_York"));
        assert_eq!(zone("america / new york"), None);
        assert_eq!(zone("asia/ho_chi_minh").as_deref(), Some("Asia/Ho_Chi_Minh"));
        assert_eq!(zone("Port au Prince").as_deref(), Some("America/Port-au-Prince"));
    }

    #[test]
    fn big_cities_borrow_a_zone() {
        assert_eq!(zone("San Francisco").as_deref(), Some("America/Los_Angeles"));
        assert_eq!(zone("mumbai").as_deref(), Some("Asia/Kolkata"));
        assert_eq!(zone("Washington, DC").as_deref(), Some("America/New_York"));
        assert_eq!(zone("St. Petersburg").as_deref(), Some("Europe/Moscow"));
    }

    #[test]
    fn abbreviations_name_a_zone() {
        assert_eq!(zone("PST").as_deref(), Some("America/Los_Angeles"));
        assert_eq!(zone("ist").as_deref(), Some("Asia/Kolkata"));
        assert!(find("UTC").is_some());
    }

    #[test]
    fn start_of_a_name_is_enough() {
        assert_eq!(zone("Amster").as_deref(), Some("Europe/Amsterdam"));
        assert_eq!(zone("to"), None);
        assert_eq!(zone("Lodnon"), None);
        assert_eq!(zone("   "), None);
    }

    #[test]
    fn names_read_naturally() {
        assert_eq!(title("  new york "), "New York");
        assert_eq!(title("San Francisco"), "San Francisco");
        assert_eq!(title("america/los_angeles"), "Los Angeles");
        assert_eq!(title("America/New York"), "New York");
        assert_eq!(title("pst"), "PST");
        assert_eq!(title("McMurdo"), "McMurdo");
    }

    #[test]
    fn local_zone_named_after_city() {
        let place = Place::local(find("Asia/Tokyo").expect("a zone"));
        assert_eq!(place.name, "Tokyo");
        assert_eq!(Place::named("Lodnon").name, "Lodnon");
        assert!(Place::named("Lodnon").zone.is_none());
    }
}
