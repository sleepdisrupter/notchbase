//! Keeps the cursor gliding through Wi-Fi gaps, the way multiplayer games keep other players
//! moving between network updates.
//!
//! Positions are shown the moment they arrive. But Wi-Fi sometimes goes quiet for tens of
//! milliseconds (on a Mac, about twice a second, while its radio visits AirDrop's channel), and
//! a cursor that waits for the next packet freezes, then jumps. So when nothing at all arrives
//! for a moment while the cursor was moving, it carries on the way it was going, easing off so it
//! never runs far. When real positions come back, the difference is blended away over a few
//! frames instead of jumping. A packet with no movement in it means the mouse stopped, and the
//! cursor stops with it.

/// Nothing heard for this long while moving: Wi-Fi went quiet, keep the cursor going.
const LATE: u64 = 9_000;
/// Positions arriving closer together than this came in one burst.
const BURST: u64 = 1_000;
/// Never guess further ahead than this; past it the cursor waits.
const FURTHEST: u64 = 90_000;
/// How quickly a guess eases off: the cursor never runs more than this long at full speed.
const EASE: f64 = 25_000.0;
/// How quickly a wrong guess is blended away.
const BLEND: f64 = 30_000.0;
/// A position more than this much older than the last one means the sender's clock started
/// over (it restarted Roam), not a late packet.
const RESTART: u64 = 250_000;
/// Moves further apart than this (sender time) don't say how fast the cursor is going.
const STALE: u64 = 60_000;
/// How often the cursor is moved along while guessing or blending.
pub const FRAME: u64 = 1_000;

#[derive(Clone, Copy)]
struct Real {
    /// When it was sent, in the sender's clock made continuous.
    sent: u64,
    arrived: u64,
    at: [f32; 2],
}

pub struct Smoother {
    on: bool,
    last_raw: Option<u32>,
    extended: u64,
    real: Option<Real>,
    /// Units per microsecond.
    velocity: [f64; 2],
    /// Last time anything arrived from the sender.
    heard: u64,
    /// What is shown minus the real position, fading away.
    offset: [f64; 2],
    faded: u64,
    /// The cursor on screen includes a guess.
    guessed: bool,
    /// Real positions are arriving in a burst right after a guess: each one is blended in.
    recovering: bool,
    /// How far apart positions usually arrive, in microseconds.
    interval: f64,
    shown: Option<[f32; 2]>,
}

impl Smoother {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            last_raw: None,
            extended: 0,
            real: None,
            velocity: [0.0; 2],
            heard: 0,
            offset: [0.0; 2],
            faded: 0,
            guessed: false,
            recovering: false,
            interval: 4_000.0,
            shown: None,
        }
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        if !on {
            self.velocity = [0.0; 2];
            self.offset = [0.0; 2];
        }
    }

    /// Forgets everything, for when the cursor arrives anew: it may come from a new session
    /// with a new clock.
    pub fn reset(&mut self) {
        self.last_raw = None;
        self.real = None;
        self.velocity = [0.0; 2];
        self.offset = [0.0; 2];
        self.guessed = false;
        self.shown = None;
    }

    /// Something arrived from the sender. Without movement in it, the mouse has stopped.
    pub fn heard(&mut self, now: u64, moved: bool) {
        if !moved && self.guessing(self.heard.max(now.saturating_sub(1)), now) {
            // Stop where the guess got to and blend back to the real spot.
            self.settle(now);
        }
        if !moved {
            self.velocity = [0.0; 2];
        }
        self.heard = now;
    }

    /// A position sent at `sent` (the sender's wrapping microsecond clock) arrived at `now`.
    /// Returns where to show the cursor now.
    pub fn push(&mut self, sent: u32, now: u64, at: [f32; 2]) -> Option<[f32; 2]> {
        let step = match self.last_raw {
            Some(last) => u64::from(sent.wrapping_sub(last)),
            None => 0,
        };
        if self.last_raw.is_some() && step > u64::from(u32::MAX / 2) {
            let back = u64::from(u32::MAX) - step + 1;
            if back <= RESTART {
                // From before the last one: a late packet, dropped.
                return None;
            }
            // The sender's clock started over: start over with it.
            self.reset();
            return self.push(sent, now, at);
        }
        self.last_raw = Some(sent);
        self.extended += step;
        let shown = self.shown.map(|s| [f64::from(s[0]), f64::from(s[1])]);
        let burst = self.real.is_some_and(|r| now.saturating_sub(r.arrived) < BURST);
        if let Some(previous) = self.real {
            let spacing = now.saturating_sub(previous.arrived) as f64;
            if spacing < LATE as f64 {
                self.interval = (self.interval * 0.9 + spacing * 0.1).clamp(1_000.0, LATE as f64);
            }
            let elapsed = self.extended.saturating_sub(previous.sent);
            if elapsed > 0 && elapsed < STALE {
                let elapsed = elapsed as f64;
                let speed = [f64::from(at[0] - previous.at[0]) / elapsed, f64::from(at[1] - previous.at[1]) / elapsed];
                self.velocity = [self.velocity[0] * 0.4 + speed[0] * 0.6, self.velocity[1] * 0.4 + speed[1] * 0.6];
            } else {
                self.velocity = [0.0; 2];
            }
        }
        self.real = Some(Real { sent: self.extended, arrived: now, at });
        self.heard = now;
        if !self.on {
            self.shown = Some(at);
            return Some(at);
        }
        // Coming out of a guess: start from what is on screen and blend toward the real spot.
        // Coming out of a guess, and for the rest of the burst that follows it: carry on from
        // what is on screen and blend toward the newest real spot.
        self.recovering = self.guessed || (self.recovering && burst);
        if let Some(shown) = shown
            && self.recovering
        {
            self.offset = [shown[0] - f64::from(at[0]), shown[1] - f64::from(at[1])];
        }
        self.guessed = false;
        self.faded = now;
        self.show(now)
    }

    /// Shows the real position at once: a click or key should land where the cursor really is.
    pub fn catch_up(&mut self) -> Option<[f32; 2]> {
        let real = self.real?;
        self.offset = [0.0; 2];
        self.velocity = [0.0; 2];
        self.guessed = false;
        if self.shown == Some(real.at) {
            return None;
        }
        self.shown = Some(real.at);
        Some(real.at)
    }

    /// Where the cursor should be at `now`, if that moved it.
    pub fn poll(&mut self, now: u64) -> Option<[f32; 2]> {
        if !self.on || self.real.is_none() {
            return None;
        }
        self.show(now)
    }

    /// Whether the cursor still needs moving along without new positions.
    pub fn pending(&self) -> bool {
        let speed = self.velocity[0].abs() + self.velocity[1].abs();
        let offset = self.offset[0].abs() + self.offset[1].abs();
        self.on && self.real.is_some_and(|r| r.arrived > 0) && (speed > 1e-6 || offset > 0.25)
    }

    fn guessing(&self, heard: u64, now: u64) -> bool {
        let Some(real) = self.real else { return false };
        let speed = self.velocity[0].abs() + self.velocity[1].abs();
        speed > 1e-6 && now.saturating_sub(heard.max(real.arrived)) > LATE
    }

    /// How far the cursor is guessed to have gone since the last real position.
    fn guess(&self, now: u64) -> [f64; 2] {
        let Some(real) = self.real else { return [0.0; 2] };
        if !self.guessing(self.heard, now) {
            return [0.0; 2];
        }
        // Start from about one usual step past the last position, not from a standstill.
        let elapsed = now.saturating_sub(real.arrived).min(FURTHEST) as f64 - LATE as f64 + self.interval;
        let run = EASE * (1.0 - (-elapsed / EASE).exp());
        [self.velocity[0] * run, self.velocity[1] * run]
    }

    /// Freezes the current guess into the offset, to be blended away.
    fn settle(&mut self, now: u64) {
        let guess = self.guess(now);
        self.offset = [self.offset[0] + guess[0], self.offset[1] + guess[1]];
        self.velocity = [0.0; 2];
        self.guessed = false;
        self.faded = now;
    }

    fn show(&mut self, now: u64) -> Option<[f32; 2]> {
        let real = self.real?;
        let fade = (-(now.saturating_sub(self.faded) as f64) / BLEND).exp();
        self.faded = now;
        self.offset = [self.offset[0] * fade, self.offset[1] * fade];
        if self.offset[0].abs() + self.offset[1].abs() < 0.25 {
            self.offset = [0.0; 2];
        }
        let guess = self.guess(now);
        // A guess on screen makes the next real position blend in rather than jump.
        self.guessed = guess != [0.0; 2];
        let at = [
            (f64::from(real.at[0]) + guess[0] + self.offset[0]) as f32,
            (f64::from(real.at[1]) + guess[1] + self.offset[1]) as f32,
        ];
        if self.shown == Some(at) {
            return None;
        }
        self.shown = Some(at);
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moves 1 unit per ms, a position every 2 ms.
    fn steady(smoother: &mut Smoother, from: u32, count: u32) -> u64 {
        let mut now = 0;
        for i in from..from + count {
            let sent = i * 2_000;
            now = u64::from(sent) + 1_000;
            assert_eq!(smoother.push(sent, now, [i as f32 * 2.0, 0.0]), Some([i as f32 * 2.0, 0.0]));
        }
        now
    }

    #[test]
    fn a_steady_network_shows_positions_as_they_are() {
        let mut smoother = Smoother::new(true);
        steady(&mut smoother, 0, 100);
    }

    #[test]
    fn the_cursor_glides_through_a_gap_and_blends_back() {
        let mut smoother = Smoother::new(true);
        let last = steady(&mut smoother, 0, 50); // at x = 98, moving 1 per ms
        let mut shown = vec![98.0f32];
        let mut now = last;
        // Wi-Fi goes quiet for 60 ms.
        for _ in 0..60 {
            now += FRAME;
            if let Some([x, _]) = smoother.poll(now) {
                shown.push(x);
            }
        }
        let furthest = *shown.last().unwrap();
        assert!(furthest > 115.0, "kept going through the gap: {furthest}");
        assert!(furthest < 158.0, "eased off rather than running on: {furthest}");
        // Then everything that was held up arrives together.
        for i in 50..80u32 {
            if let Some([x, _]) = smoother.push(i * 2_000, now, [i as f32 * 2.0, 0.0]) {
                shown.push(x);
            }
        }
        // The mouse stops: the next packet has no movement in it.
        for step in 0..200 {
            now += FRAME;
            if step == 3 {
                smoother.heard(now, false);
            }
            if let Some([x, _]) = smoother.poll(now) {
                shown.push(x);
            }
        }
        for pair in shown.windows(2) {
            assert!(pair[1] >= pair[0] - 0.01, "went back: {pair:?} in {shown:?}");
        }
        assert!((shown.last().unwrap() - 158.0).abs() < 0.5, "ends on the real spot: {shown:?}");
    }

    #[test]
    fn a_stopped_mouse_stops_the_cursor() {
        let mut smoother = Smoother::new(true);
        let mut now = steady(&mut smoother, 0, 50);
        // The next packet says nothing moved.
        smoother.heard(now + 3_000, false);
        for _ in 0..60 {
            now += FRAME;
            assert_eq!(smoother.poll(now), None, "no guessing once it stopped");
        }
        assert!(!smoother.pending());
    }

    #[test]
    fn a_click_lands_on_the_real_spot() {
        let mut smoother = Smoother::new(true);
        let mut now = steady(&mut smoother, 0, 50);
        now += 30_000;
        assert!(smoother.poll(now).is_some_and(|[x, _]| x > 98.0));
        assert_eq!(smoother.catch_up(), Some([98.0, 0.0]));
    }

    #[test]
    fn switched_off_never_guesses() {
        let mut smoother = Smoother::new(false);
        let now = steady(&mut smoother, 0, 50);
        assert_eq!(smoother.poll(now + 40_000), None);
    }

    #[test]
    fn a_sender_that_restarts_is_followed() {
        let mut smoother = Smoother::new(true);
        smoother.push(600_000_000, 1_000, [0.0, 0.0]);
        smoother.push(600_004_000, 5_000, [1.0, 0.0]);
        // The other computer restarted Roam: its clock is back near zero.
        assert_eq!(smoother.push(2_000, 9_000, [2.0, 0.0]), Some([2.0, 0.0]));
        assert_eq!(smoother.push(6_000, 13_000, [3.0, 0.0]), Some([3.0, 0.0]));
    }

    #[test]
    fn the_clock_wrapping_is_not_a_late_packet() {
        let mut smoother = Smoother::new(true);
        smoother.push(u32::MAX - 1_000, 5_000, [0.0, 0.0]);
        assert!(smoother.push(3_000, 9_100, [1.0, 0.0]).is_some());
        // But a genuinely older one is dropped.
        assert!(smoother.push(1_000, 9_200, [0.5, 0.0]).is_none());
    }
}
