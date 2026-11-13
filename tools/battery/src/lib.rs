//! Battery: charger, low battery and fully charged notices.

mod notices;

use notchbase_sdk::prelude::*;

pub const ID: &str = "battery";
const CHARGER: &str = "charger";
const LOW: &str = "low";
const FULL: &str = "full";
pub(crate) const LOW_AT: u8 = 20;
pub(crate) const CRITICAL_AT: u8 = 10;
const FULL_AT: u8 = 100;

pub struct Battery {
    power: Option<Power>,
    warned: Option<u8>,
    full: bool,
    quiet: bool,
    prefs: Prefs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefs {
    charger: bool,
    low: bool,
    full: bool,
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self { charger: settings.bool(CHARGER), low: settings.bool(LOW), full: settings.bool(FULL) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Power(Power),
    Focus(Focus),
    Configured(Settings),
}

impl Tool for Battery {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Battery")
            .background()
            .description("Charging, low battery and fully charged alerts")
            .version(env!("CARGO_PKG_VERSION"))
            .setting(
                SettingSpec::toggle(CHARGER, "Charger alerts", true)
                    .detail("Show when the charger is plugged in or unplugged"),
            )
            .setting(SettingSpec::toggle(LOW, "Low battery warning", true).detail("Warn at 20% and again at 10%"))
            .setting(SettingSpec::toggle(FULL, "Fully charged alert", true).detail("Show when the battery is full"))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = Prefs::from(context.settings());
        (Self { power: None, warned: None, full: false, quiet: false, prefs }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Power(power) => self.follow(power),
            Msg::Focus(focus) => {
                self.quiet = focus.active;
                Command::none()
            }
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::power(Msg::Power),
            Subscription::focus(Msg::Focus),
            Subscription::settings(Msg::Configured),
        ])
    }
}

impl Battery {
    fn follow(&mut self, now: Power) -> Command<Msg> {
        let before = self.power.replace(now);
        let Some(level) = now.level else { return Command::none() };
        let known = before.is_some_and(|before| before.level.is_some());
        let replugged = before.is_some_and(|before| before.plugged != now.plugged);
        if !known || replugged {
            self.warned = threshold(level).filter(|_| !now.plugged);
            self.full = now.plugged && level >= FULL_AT;
            return show(known && self.prefs.charger && !self.quiet, || notices::charger(now, level));
        }
        if now.plugged {
            let filled = level >= FULL_AT && !self.full;
            self.full |= filled;
            return show(filled && self.prefs.full && !self.quiet, notices::full);
        }
        match threshold(level) {
            Some(crossed) if self.warned.is_none_or(|warned| crossed < warned) => {
                self.warned = Some(crossed);
                show(self.prefs.low, || notices::low(level))
            }
            Some(_) => Command::none(),
            None => {
                self.warned = None;
                Command::none()
            }
        }
    }
}

fn threshold(level: u8) -> Option<u8> {
    [CRITICAL_AT, LOW_AT].into_iter().find(|at| level <= *at)
}

fn show(allowed: bool, notice: impl FnOnce() -> Notice) -> Command<Msg> {
    if allowed { Command::notice(notice()) } else { Command::none() }
}

notchbase_sdk::export!(Battery);

#[cfg(test)]
mod tests;
