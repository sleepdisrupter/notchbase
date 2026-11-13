//! The battery on Windows, from GetSystemPowerStatus.

use notchbase_sdk::protocol::Power;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

const ON_AC: u8 = 1;
const CHARGING: u8 = 8;
const NO_BATTERY: u8 = 128;
const UNKNOWN: u8 = 255;

pub(super) fn read() -> Power {
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return Power::default();
    }
    power(status.ACLineStatus, status.BatteryFlag, status.BatteryLifePercent)
}

fn power(line: u8, flags: u8, percent: u8) -> Power {
    let fitted = flags != UNKNOWN && flags & NO_BATTERY == 0;
    let level = (fitted && percent <= 100).then_some(percent);
    let plugged = line == ON_AC;
    Power { level, plugged, charging: plugged && fitted && flags & CHARGING != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_has_no_battery_level() {
        assert_eq!(power(ON_AC, NO_BATTERY, UNKNOWN), Power { level: None, plugged: true, charging: false });
        assert_eq!(power(ON_AC, UNKNOWN, UNKNOWN).level, None);
    }

    #[test]
    fn laptop_reports_level_and_charge() {
        assert_eq!(power(ON_AC, CHARGING, 64), Power { level: Some(64), plugged: true, charging: true });
        assert_eq!(power(0, 0, 41), Power { level: Some(41), plugged: false, charging: false });
        assert_eq!(power(ON_AC, 0, 100), Power { level: Some(100), plugged: true, charging: false });
    }
}
