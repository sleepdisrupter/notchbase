//! The calendar's settings.

use notchbase_sdk::prelude::*;

pub const SOURCE: &str = "source";
pub const LINK: &str = "link";
pub const LEAD: &str = "heads_up";
pub const NUDGE: &str = "nudge";
pub const ALL_DAY: &str = "all_day";
pub const WEEK_START: &str = "week_start";
pub const SKIPPED_SETUP: &str = "setup_skipped";
pub const VIEW: &str = "view";
pub const MONTH_VIEW: &str = "month";
const AUTO: &str = "auto";
const MONDAY: &str = "monday";
const SUNDAY: &str = "sunday";
const APPLE: &str = "apple";
const ICAL: &str = "ical";
const DEFAULT_LEAD: f64 = 10.0;
const MINUTE_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Apple,
    Links,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub source: Source,
    pub links: Vec<String>,
    pub lead_ms: u64,
    pub nudge: bool,
    pub all_day: bool,
    pub week_start: Option<bool>,
    pub skipped_setup: bool,
    pub week: bool,
}

impl Prefs {
    pub fn declare(manifest: Manifest) -> Manifest {
        let source =
            SettingSpec::choice(SOURCE, "Source", [(APPLE, "Apple Calendar"), (ICAL, "iCal link")], default_source())
                .detail("Where your events come from");
        let source = if cfg!(target_os = "macos") { source } else { source.hidden() };
        manifest
            .setting(source)
            .setting(
                SettingSpec::text(LINK, "iCal link", "")
                    .placeholder("Paste a secret address")
                    .detail("Separate several with spaces")
                    .secret()
                    .shown_when_is(SOURCE, ICAL),
            )
            .setting(
                SettingSpec::number(LEAD, "Heads-up", [0.0, 30.0], 5.0, DEFAULT_LEAD)
                    .unit("min")
                    .detail("How early a meeting shows up"),
            )
            .setting(SettingSpec::toggle(NUDGE, "Nudge when it starts", true))
            .setting(SettingSpec::toggle(ALL_DAY, "All-day events", true).detail("Show them on the page"))
            .setting(SettingSpec::data(SKIPPED_SETUP, Value::Bool(false)))
            .setting(SettingSpec::data(VIEW, Value::from("week")))
            .setting(
                SettingSpec::choice(
                    WEEK_START,
                    "Week starts on",
                    [(AUTO, "Auto"), (MONDAY, "Monday"), (SUNDAY, "Sunday")],
                    AUTO,
                )
                .detail("Auto follows your clock: Sunday with a 12-hour clock"),
            )
    }

    pub fn read(settings: &Settings) -> Prefs {
        let source = match settings.text(SOURCE) {
            APPLE => Source::Apple,
            ICAL => Source::Links,
            _ if cfg!(target_os = "macos") => Source::Apple,
            _ => Source::Links,
        };
        let lead = settings.get(LEAD).and_then(Value::as_f64).unwrap_or(DEFAULT_LEAD).clamp(0.0, 30.0);
        Prefs {
            source,
            links: links(settings.text(LINK)),
            lead_ms: lead.round() as u64 * MINUTE_MS,
            nudge: settings.get(NUDGE).and_then(Value::as_bool).unwrap_or(true),
            all_day: settings.get(ALL_DAY).and_then(Value::as_bool).unwrap_or(true),
            skipped_setup: settings.get(SKIPPED_SETUP).and_then(Value::as_bool).unwrap_or(false),
            week: settings.text(VIEW) != MONTH_VIEW,
            week_start: match settings.text(WEEK_START) {
                MONDAY => Some(true),
                SUNDAY => Some(false),
                _ => None,
            },
        }
    }

    pub fn monday_first(&self, clock_24h: bool) -> bool {
        self.week_start.unwrap_or(clock_24h)
    }
}

fn default_source() -> &'static str {
    if cfg!(target_os = "macos") { APPLE } else { ICAL }
}

pub fn links(text: &str) -> Vec<String> {
    let mut links: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(str::trim)
        .filter(|link| !link.is_empty())
        .map(|link| match link.get(..9) {
            Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => format!("https://{}", &link[9..]),
            _ => link.to_owned(),
        })
        .collect();
    links.dedup();
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webcal_becomes_https() {
        let found = links(" webcal://p01-caldav.icloud.com/published/2/abc ,https://calendar.google.com/x/basic.ics\n");
        assert_eq!(found, ["https://p01-caldav.icloud.com/published/2/abc", "https://calendar.google.com/x/basic.ics"]);
        assert!(links("   ").is_empty());
    }

    #[test]
    fn missing_settings_use_defaults() {
        let prefs = Prefs::read(&Settings::new(Default::default()));
        assert_eq!(prefs.lead_ms, 10 * MINUTE_MS);
        assert!(prefs.nudge && prefs.all_day);
        let expected = if cfg!(target_os = "macos") { Source::Apple } else { Source::Links };
        assert_eq!(prefs.source, expected);
    }
}
