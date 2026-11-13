//! major.minor.patch versions. Pre-release and build tags are ignored.

use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version([u32; 3]);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let core = text.strip_prefix('v').unwrap_or(text).split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let mut number = || parts.next().map_or(Some(0), |part| part.parse().ok());
        let version = Self([number()?, number()?, number()?]);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_versions() {
        assert_eq!(Version::parse("0.2.10"), Some(Version([0, 2, 10])));
        assert_eq!(Version::parse("v1.4"), Some(Version([1, 4, 0])));
        assert_eq!(Version::parse("2.0.0-beta.1"), Some(Version([2, 0, 0])));
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("soon"), None);
        assert_eq!(Version([0, 10, 0]).to_string(), "0.10.0");
    }

    #[test]
    fn compares_numerically() {
        assert!(Version::parse("0.10.0") > Version::parse("0.9.9"));
    }

    #[test]
    fn missing_parts_count_as_zero() {
        assert_eq!(Version::parse("3"), Some(Version([3, 0, 0])));
        assert_eq!(Version::parse("3.1"), Some(Version([3, 1, 0])));
    }

    #[test]
    fn ignores_space_and_metadata() {
        assert_eq!(Version::parse("  1.2.3\n"), Some(Version([1, 2, 3])));
        assert_eq!(Version::parse("1.2.3+build.7"), Some(Version([1, 2, 3])));
        assert_eq!(Version::parse("v0.16.2-rc.1+abc"), Some(Version([0, 16, 2])));
    }

    #[test]
    fn leading_zeros_are_ignored() {
        assert_eq!(Version::parse("01.002.0003"), Some(Version([1, 2, 3])));
    }

    #[test]
    fn rejects_non_versions() {
        for text in
            ["", "v", "V1.2.3", "-1.0.0", "1..3", "1.2.", "1.x.3", "\u{0661}.\u{0662}.\u{0663}", "4294967296.0.0"]
        {
            assert_eq!(Version::parse(text), None, "{text}");
        }
    }

    #[test]
    fn largest_numbers_fit() {
        let most = u32::MAX.to_string();
        let text = format!("{most}.{most}.{most}");
        assert_eq!(Version::parse(&text).map(|version| version.to_string()), Some(text));
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(Version::default().to_string(), "0.0.0");
        assert!(Version::default() < Version([0, 0, 1]));
    }

    #[test]
    fn orders_major_minor_patch() {
        assert!(Version([1, 0, 0]) > Version([0, 99, 99]));
        assert!(Version([0, 2, 0]) > Version([0, 1, 99]));
        assert!(Version([0, 0, 2]) > Version([0, 0, 1]));
    }

    #[test]
    fn display_round_trips() {
        for version in [Version([0, 16, 2]), Version([10, 0, 7]), Version::default()] {
            assert_eq!(Version::parse(&version.to_string()), Some(version));
        }
    }
}
