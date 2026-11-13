//! The battery notices, with a ring that shows the level.

use std::time::Duration;

use notchbase_sdk::prelude::*;

use crate::{CRITICAL_AT, LOW_AT};

const GLYPH: f32 = 15.0;
const RING: f32 = 13.0;
const SHOWN: Duration = Duration::from_millis(3200);
const WARNED: Duration = Duration::from_secs(6);
const CHARGING: &str = "Charging";
const PLUGGED_IN: &str = "Plugged In";
const ON_BATTERY: &str = "On Battery";
const LOW_BATTERY: &str = "Low Battery";
const FULLY_CHARGED: &str = "Fully Charged";

pub fn charger(power: Power, level: u8) -> Notice {
    match (power.plugged, power.charging) {
        (true, true) => gauge(Icon::Bolt, Tone::Green, CHARGING, level, Tone::Green).pop(),
        (true, false) => gauge(Icon::Bolt, Tone::Green, PLUGGED_IN, level, Tone::Green).pop(),
        (false, _) => gauge(Icon::Battery, Tone::Secondary, ON_BATTERY, level, fill(level)),
    }
}

pub fn low(level: u8) -> Notice {
    let notice = gauge(Icon::Battery, fill(level), LOW_BATTERY, level, fill(level)).duration(WARNED);
    if level <= CRITICAL_AT { notice.shake() } else { notice.pop() }
}

pub fn full() -> Notice {
    gauge(Icon::Bolt, Tone::Green, FULLY_CHARGED, 100, Tone::Green).pop()
}

fn fill(level: u8) -> Tone {
    match level {
        level if level <= CRITICAL_AT => Tone::Red,
        level if level <= LOW_AT => Tone::Orange,
        _ => Tone::Green,
    }
}

fn gauge(glyph: Icon, tint: Tone, words: &str, level: u8, fill: Tone) -> Notice {
    let mark = || icon(glyph).size(GLYPH).tone(tint);
    let leading = adaptive(mark(), row![mark(), text(words).callout()].spacing(7.0));
    let share = f32::from(level.min(100)) / 100.0;
    let trailing = row![text(format!("{level}%")).value().secondary(), ring(share).size(RING).tone(fill)].spacing(6.0);
    Notice::new(leading, trailing).duration(SHOWN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_warns_as_it_drains() {
        assert_eq!(fill(64), Tone::Green);
        assert_eq!(fill(20), Tone::Orange);
        assert_eq!(fill(10), Tone::Red);
    }

    #[test]
    fn only_critical_shakes() {
        assert!(!low(18).into_node().shake);
        assert!(low(9).into_node().shake);
        assert_eq!(low(9).into_node().duration_ms, WARNED.as_millis() as u64);
    }
}
