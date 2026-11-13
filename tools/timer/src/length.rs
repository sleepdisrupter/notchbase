//! The length the wheels set, in hours, minutes and seconds.

const SECOND: u64 = 1_000;
const MINUTE: u64 = 60 * SECOND;
const HOUR: u64 = 60 * MINUTE;
const MOST_HOURS: u64 = 23;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Length {
    pub hours: u64,
    pub minutes: u64,
    pub seconds: u64,
}

impl Length {
    pub fn from_ms(ms: u64) -> Self {
        let seconds = ms / SECOND;
        Self { hours: (seconds / 3600).min(MOST_HOURS), minutes: seconds / 60 % 60, seconds: seconds % 60 }
    }

    pub fn ms(self) -> u64 {
        self.hours * HOUR + self.minutes * MINUTE + self.seconds * SECOND
    }

    pub fn set(self, column: usize, value: usize) -> Self {
        let value = value as u64;
        match column {
            0 => Self { hours: value.min(MOST_HOURS), ..self },
            1 => Self { minutes: value.min(59), ..self },
            2 => Self { seconds: value.min(59), ..self },
            _ => self,
        }
    }

    pub fn words(self) -> String {
        let parts = [(self.hours, "hr"), (self.minutes, "min"), (self.seconds, "sec")];
        let said: Vec<String> =
            parts.iter().filter(|(count, _)| *count > 0).map(|(count, unit)| format!("{count} {unit}")).collect();
        if said.is_empty() { "0 sec".to_owned() } else { said.join(" ") }
    }
}

pub fn most_hours() -> usize {
    MOST_HOURS as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_ms() {
        let length = Length { hours: 1, minutes: 30, seconds: 5 };
        assert_eq!(Length::from_ms(length.ms()), length);
        assert_eq!(Length::from_ms(99 * HOUR).hours, MOST_HOURS);
    }

    #[test]
    fn wheels_set_their_own_part() {
        let length = Length::from_ms(5 * MINUTE);
        assert_eq!(length.set(0, 2).set(2, 30), Length { hours: 2, minutes: 5, seconds: 30 });
        assert_eq!(length.set(1, 99).minutes, 59);
        assert_eq!(length.set(7, 1), length);
    }

    #[test]
    fn lengths_read_naturally() {
        assert_eq!(Length::from_ms(5 * MINUTE).words(), "5 min");
        assert_eq!(Length { hours: 1, minutes: 0, seconds: 45 }.words(), "1 hr 45 sec");
        assert_eq!(Length::from_ms(0).words(), "0 sec");
    }
}
