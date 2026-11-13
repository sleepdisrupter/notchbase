//! The focus timer's settings, and the session count it keeps with them.

use notchbase_sdk::prelude::*;

pub const FOCUS_MINUTES: &str = "focus_minutes";
pub const BREAK_MINUTES: &str = "break_minutes";
pub const AUTO_BREAK: &str = "auto_break";
pub const SESSIONS: &str = "sessions";
const FOCUS_RANGE: [u64; 2] = [5, 90];
const BREAK_RANGE: [u64; 2] = [1, 30];
const FOCUS_STEP: u64 = 5;
const BREAK_STEP: u64 = 1;
const DEFAULT_FOCUS: u64 = 25;
const DEFAULT_BREAK: u64 = 5;
const MINUTE_MS: u64 = 60_000;
const UNIT: &str = "min";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub focus_minutes: u64,
    pub break_minutes: u64,
    pub auto_break: bool,
}

impl Prefs {
    pub fn declare(manifest: Manifest) -> Manifest {
        manifest
            .setting(
                number(FOCUS_MINUTES, "Focus length", FOCUS_RANGE, FOCUS_STEP, DEFAULT_FOCUS)
                    .detail("How long each focus session runs"),
            )
            .setting(
                number(BREAK_MINUTES, "Break length", BREAK_RANGE, BREAK_STEP, DEFAULT_BREAK)
                    .detail("How long the break after a session lasts"),
            )
            .setting(
                SettingSpec::toggle(AUTO_BREAK, "Start breaks automatically", true)
                    .detail("Begin a break as soon as a focus session ends"),
            )
            .setting(SettingSpec::data(SESSIONS, Value::from(0))) // hidden, keeps the count between runs
    }

    pub fn from(settings: &Settings) -> Self {
        Self {
            focus_minutes: minutes(settings, FOCUS_MINUTES, FOCUS_RANGE),
            break_minutes: minutes(settings, BREAK_MINUTES, BREAK_RANGE),
            auto_break: settings.bool(AUTO_BREAK),
        }
    }

    pub fn focus_ms(&self) -> u64 {
        self.focus_minutes * MINUTE_MS
    }

    pub fn break_ms(&self) -> u64 {
        self.break_minutes * MINUTE_MS
    }
}

pub fn sessions(settings: &Settings) -> u64 {
    settings.number(SESSIONS).round() as u64
}

fn number(key: &str, title: &str, [min, max]: [u64; 2], step: u64, default: u64) -> SettingSpec {
    SettingSpec::number(key, title, [min as f64, max as f64], step as f64, default as f64).unit(UNIT)
}

fn minutes(settings: &Settings, key: &str, [min, max]: [u64; 2]) -> u64 {
    (settings.number(key).round() as u64).clamp(min, max)
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{SettingKind, Values};

    use super::*;

    fn settings(pairs: &[(&str, Value)]) -> Settings {
        Settings::new(pairs.iter().cloned().map(|(key, value)| (key.to_owned(), value)).collect::<Values>())
    }

    #[test]
    fn manifest_declares_every_setting() {
        let manifest = Prefs::declare(Manifest::new("focus-timer", "Focus"));
        let keys: Vec<&str> = manifest.settings.iter().map(|setting| setting.key.as_str()).collect();
        assert_eq!(keys, [FOCUS_MINUTES, BREAK_MINUTES, AUTO_BREAK, SESSIONS]);
        let focus = &manifest.settings[0];
        assert_eq!(focus.kind, SettingKind::Number { min: 5.0, max: 90.0, step: 5.0, unit: "min".into() });
        assert_eq!(focus.default, Value::from(25));
        let rest = &manifest.settings[1];
        assert_eq!(rest.kind, SettingKind::Number { min: 1.0, max: 30.0, step: 1.0, unit: "min".into() });
        assert_eq!(rest.default, Value::from(5));
        assert_eq!(manifest.settings[2].default, Value::Bool(true));
        let sessions = &manifest.settings[3];
        assert_eq!((&sessions.kind, sessions.visible), (&SettingKind::Data, false));
        let defaults = Settings::new(manifest.defaults());
        let prefs = Prefs::from(&defaults);
        assert_eq!(prefs, Prefs { focus_minutes: 25, break_minutes: 5, auto_break: true });
        assert_eq!(super::sessions(&defaults), 0);
    }

    #[test]
    fn lengths_become_milliseconds() {
        let prefs = Prefs::from(&settings(&[(FOCUS_MINUTES, 50.into()), (BREAK_MINUTES, 10.into())]));
        assert_eq!((prefs.focus_ms(), prefs.break_ms()), (50 * 60_000, 10 * 60_000));
    }

    #[test]
    fn bad_lengths_are_clamped() {
        let prefs = Prefs::from(&settings(&[(FOCUS_MINUTES, 500.into()), (BREAK_MINUTES, (-3).into())]));
        assert_eq!((prefs.focus_minutes, prefs.break_minutes), (90, 1));
        let prefs = Prefs::from(&settings(&[(FOCUS_MINUTES, "soon".into())]));
        assert_eq!((prefs.focus_minutes, prefs.break_minutes, prefs.auto_break), (5, 1, false));
    }

    #[test]
    fn count_reads_any_number() {
        assert_eq!(sessions(&settings(&[(SESSIONS, 7.into())])), 7);
        assert_eq!(sessions(&settings(&[(SESSIONS, 7.0.into())])), 7);
        assert_eq!(sessions(&settings(&[(SESSIONS, (-2).into())])), 0);
    }
}
