//! The updates section: whether updates install by themselves, and the last version whose notes were seen.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    pub automatic: bool,
    pub seen: String,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { automatic: true, seen: String::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Config;

    #[test]
    fn old_file_reads_as_never_seen() {
        let old: Config = serde_json::from_str(r#"{ "updates": { "automatic": false } }"#).unwrap();
        assert_eq!((old.updates.automatic, old.updates.seen.as_str()), (false, ""));
        let older: Config = serde_json::from_str(r#"{ "notch": { "hover_delay_ms": 90 } }"#).unwrap();
        assert_eq!((older.updates.automatic, older.updates.seen.as_str()), (true, ""));
        assert_eq!(Config::default().updates.seen, "");
    }

    #[test]
    fn seen_marker_survives_save_and_load() {
        let mut config = Config::default();
        config.updates.seen = "0.14.0".to_owned();
        let saved = serde_json::to_string_pretty(&config).unwrap();
        assert!(saved.contains("\"updates\": {\n    \"automatic\": true,\n    \"seen\": \"0.14.0\"\n  }"), "{saved}");
        assert_eq!(serde_json::from_str::<Config>(&saved).unwrap(), config);
    }
}
