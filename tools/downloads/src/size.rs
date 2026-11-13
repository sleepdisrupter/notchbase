//! File sizes in decimal units, like Finder. Anything under a megabyte rounds up to whole KB, so a
//! tiny file shows as 1 KB rather than 0.

const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
const STEP: f64 = 1000.0;

pub fn bytes(count: u64) -> String {
    let mut amount = count as f64 / STEP;
    let mut unit = 0;
    while amount >= STEP && unit + 1 < UNITS.len() {
        amount /= STEP;
        unit += 1;
    }
    match (unit, amount) {
        (0, amount) => format!("{} {}", amount.ceil(), UNITS[0]),
        (unit, amount) if amount >= 100.0 => format!("{amount:.0} {}", UNITS[unit]),
        (unit, amount) => format!("{amount:.1} {}", UNITS[unit]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_a_file_browser() {
        assert_eq!(bytes(0), "0 KB");
        assert_eq!(bytes(1), "1 KB");
        assert_eq!(bytes(52_000), "52 KB");
        assert_eq!(bytes(12_400_000), "12.4 MB");
        assert_eq!(bytes(480_000_000), "480 MB");
        assert_eq!(bytes(2_340_000_000), "2.3 GB");
    }
}
