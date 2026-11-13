//! The webhook's settings: on or off, the port and an optional token.

use notchbase_sdk::prelude::*;

pub const ENABLED: &str = "enabled";
pub const PORT: &str = "port";
pub const TOKEN: &str = "token";
pub const DEFAULT_PORT: u16 = 47832;
const LOWEST_PORT: u16 = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub port: u16,
    pub token: Option<String>,
}

impl Config {
    pub fn declare(manifest: Manifest) -> Manifest {
        let ports = [f64::from(LOWEST_PORT), f64::from(u16::MAX)];
        manifest
            .setting(
                SettingSpec::toggle(ENABLED, "Webhook alerts", true)
                    .detail("Show alerts sent to http://127.0.0.1:<port>/alert"),
            )
            .setting(
                SettingSpec::number(PORT, "Webhook port", ports, 1.0, f64::from(DEFAULT_PORT))
                    .detail("Only apps on this computer can reach it"),
            )
            .setting(SettingSpec::text(TOKEN, "Webhook token", "").placeholder("No token").hidden())
    }

    pub fn from(settings: &Settings) -> Self {
        Self { enabled: settings.bool(ENABLED), port: port(settings), token: settings.string(TOKEN) }
    }
}

fn port(settings: &Settings) -> u16 {
    let value = settings.get(PORT);
    let number = value.and_then(Value::as_f64).or_else(|| value.and_then(Value::as_str)?.trim().parse().ok());
    number
        .filter(|number: &f64| number.is_finite())
        .map_or(DEFAULT_PORT, |number| number.round().clamp(f64::from(LOWEST_PORT), f64::from(u16::MAX)) as u16)
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::Values;

    use super::*;

    fn config(port: Value, token: &str) -> Config {
        let mut values = Values::new();
        values.insert(ENABLED.into(), true.into());
        values.insert(PORT.into(), port);
        values.insert(TOKEN.into(), token.into());
        Config::from(&Settings::new(values))
    }

    #[test]
    fn ports_are_read_leniently() {
        assert_eq!(config(8080.into(), "").port, 8080);
        assert_eq!(config(" 9000 ".into(), "").port, 9000);
        assert_eq!(config(80.into(), "").port, LOWEST_PORT);
        assert_eq!(config(1e9.into(), "").port, u16::MAX);
        assert_eq!(config("soon".into(), "").port, DEFAULT_PORT);
        assert_eq!(config(Value::Null, "").port, DEFAULT_PORT);
    }

    #[test]
    fn blank_token_means_none() {
        assert_eq!(config(8080.into(), "  ").token, None);
        assert_eq!(config(8080.into(), " s3cret ").token.as_deref(), Some("s3cret"));
    }
}
