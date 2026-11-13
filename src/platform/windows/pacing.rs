//! Sleeps each frame until the display's next vblank, read from DWM, and opts out of the power throttling Windows 11
//! gives a notch that never takes focus.

use std::time::Duration;

use windows_sys::Win32::Graphics::Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling,
    SetProcessInformation,
};

const FALLBACK: Duration = Duration::from_micros(16_667);
const LONGEST: Duration = Duration::from_millis(34);
const TOO_CLOSE: f64 = 0.15;

pub fn until_vblank() -> Option<Duration> {
    Some(next_vblank().unwrap_or(FALLBACK).min(LONGEST))
}

pub fn keep_full_speed() {
    let masks = [
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    ];
    for mask in masks {
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: mask,
            StateMask: 0,
        };
        let size = size_of_val(&state) as u32;
        let set = unsafe {
            SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, (&raw const state).cast(), size)
        };
        if set != 0 {
            return;
        }
    }
}

fn next_vblank() -> Option<Duration> {
    let mut info: DWM_TIMING_INFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<DWM_TIMING_INFO>() as u32;
    if unsafe { DwmGetCompositionTimingInfo(std::ptr::null_mut(), &mut info) } < 0 {
        return None;
    }
    let (mut now, mut frequency) = (0i64, 0i64);
    unsafe {
        QueryPerformanceCounter(&mut now);
        QueryPerformanceFrequency(&mut frequency);
    }
    let period = i64::try_from(info.qpcRefreshPeriod).ok().filter(|period| *period > 0)?;
    let last = i64::try_from(info.qpcVBlank).ok()?;
    if frequency <= 0 {
        return None;
    }
    let mut ahead = (last - now).rem_euclid(period);
    // Too close to make it, so wait for the one after.
    if (ahead as f64) < period as f64 * TOO_CLOSE {
        ahead += period;
    }
    Some(Duration::from_secs_f64(ahead as f64 / frequency as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_waits_over_two_frames() {
        let wait = until_vblank().expect("a wait");
        assert!(wait > Duration::ZERO && wait <= LONGEST, "{wait:?}");
    }

    #[test]
    fn opting_out_of_throttling_is_harmless() {
        keep_full_speed();
    }
}
