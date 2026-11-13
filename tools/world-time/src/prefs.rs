//! World time's settings: home, four more cities and the clock style.

use notchbase_sdk::prelude::*;

pub const HOME: &str = "home";
pub const CITIES: [&str; 4] = ["city_2", "city_3", "city_4", "city_5"];
pub const HOURS: &str = "hours";
const TITLES: [&str; 4] = ["City 2", "City 3", "City 4", "City 5"];
const DEFAULTS: [&str; 4] = ["London", "New York", "Tokyo", ""];
const PLACEHOLDER: &str = "Tokyo or America/New York";
const SYSTEM: &str = "system";
const TWELVE: &str = "12";
const TWENTY_FOUR: &str = "24";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hours {
    #[default]
    System,
    Twelve,
    TwentyFour,
}

impl Hours {
    pub fn full_day(self, system: bool) -> bool {
        match self {
            Hours::System => system,
            Hours::Twelve => false,
            Hours::TwentyFour => true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub home: String,
    pub cities: Vec<String>,
    pub hours: Hours,
}

impl Prefs {
    pub fn declare(manifest: Manifest) -> Manifest {
        let home = SettingSpec::text(HOME, "Home", "").placeholder("This computer");
        let cities = CITIES
            .iter()
            .zip(TITLES)
            .zip(DEFAULTS)
            .map(|((key, title), default)| SettingSpec::text(*key, title, default).placeholder(PLACEHOLDER));
        let hours = SettingSpec::choice(
            HOURS,
            "Clock",
            [(SYSTEM, "System"), (TWELVE, "12-hour"), (TWENTY_FOUR, "24-hour")],
            SYSTEM,
        );
        cities.fold(manifest.setting(home), Manifest::setting).setting(hours)
    }

    pub fn from(settings: &Settings) -> Self {
        Self {
            home: settings.text(HOME).trim().to_owned(),
            cities: CITIES.iter().filter_map(|key| settings.string(key)).collect(),
            hours: match settings.text(HOURS) {
                TWELVE => Hours::Twelve,
                TWENTY_FOUR => Hours::TwentyFour,
                _ => Hours::System,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{SettingKind, Values};

    use super::*;

    fn settings(pairs: &[(&str, Value)]) -> Settings {
        Settings::new(pairs.iter().cloned().map(|(key, value)| (key.to_owned(), value)).collect::<Values>())
    }

    #[test]
    fn manifest_has_home_and_cities() {
        let manifest = Prefs::declare(Manifest::new("world-time", "World Time"));
        let keys: Vec<&str> = manifest.settings.iter().map(|setting| setting.key.as_str()).collect();
        assert_eq!(keys, [HOME, "city_2", "city_3", "city_4", "city_5", HOURS]);
        assert!(manifest.settings[..5].iter().all(|setting| matches!(setting.kind, SettingKind::Text { .. })));
        let prefs = Prefs::from(&Settings::new(manifest.defaults()));
        assert_eq!(
            prefs,
            Prefs { home: String::new(), cities: names(["London", "New York", "Tokyo"]), hours: Hours::System }
        );
    }

    #[test]
    fn blank_cities_are_skipped() {
        let prefs = Prefs::from(&settings(&[
            (HOME, "  Berlin ".into()),
            ("city_2", "   ".into()),
            ("city_3", " Lima".into()),
            ("city_5", 7.into()),
        ]));
        assert_eq!((prefs.home.as_str(), prefs.cities), ("Berlin", names(["Lima"])));
    }

    #[test]
    fn clock_follows_the_system() {
        let hours = |value: &str| Prefs::from(&settings(&[(HOURS, value.into())])).hours;
        assert_eq!(
            [hours("12"), hours("24"), hours("system"), hours("soon")],
            [Hours::Twelve, Hours::TwentyFour, Hours::System, Hours::System]
        );
        assert!(Hours::System.full_day(true) && !Hours::System.full_day(false));
        assert!(Hours::TwentyFour.full_day(false) && !Hours::Twelve.full_day(true));
    }

    fn names<const N: usize>(names: [&str; N]) -> Vec<String> {
        names.map(str::to_owned).to_vec()
    }
}
