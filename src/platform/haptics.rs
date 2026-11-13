//! Trackpad feedback for snaps, flips and ticks, when haptics are on. Only Macs do anything with it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feel {
    Snap,
    Flip,
    Tick,
}

pub fn haptic(feel: Feel) {
    if cfg!(test) || !crate::config::current().notch.haptics {
        return;
    }
    super::imp::perform_haptic(feel);
}
