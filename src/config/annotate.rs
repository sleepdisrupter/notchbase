//! The annotator's brush, remembered between windows: a colour as `#RRGGBB` and a width in points from 2 to 32.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const GARNET: [u8; 3] = [236, 84, 90];
const HEX_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnnotateConfig {
    pub color: String,
    #[serde(deserialize_with = "read_size", serialize_with = "write_size")]
    size: f32,
}

impl AnnotateConfig {
    pub const THINNEST: f32 = 2.0;
    pub const THICKEST: f32 = 32.0;
    pub const DEFAULT_SIZE: f32 = 5.0;
    const SIZE_STEP: f32 = 0.5;

    /// The brush to save, its size kept in range and rounded to half points.
    pub fn remembering(rgb: [u8; 3], size: f32) -> Self {
        let stepped = (within_limits(size) / Self::SIZE_STEP).round() * Self::SIZE_STEP;
        Self { color: Self::hex(rgb), size: stepped }
    }

    /// The colour, or the default garnet if the file's isn't six hex digits.
    pub fn rgb(&self) -> [u8; 3] {
        Self::parse_hex(&self.color).unwrap_or(GARNET)
    }

    pub fn brush_size(&self) -> f32 {
        within_limits(self.size)
    }

    pub fn hex([r, g, b]: [u8; 3]) -> String {
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
        let text = text.trim();
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() != HEX_DIGITS || !digits.bytes().all(|digit| digit.is_ascii_hexdigit()) {
            return None;
        }
        let [_, r, g, b] = u32::from_str_radix(digits, HEX_RADIX).ok()?.to_be_bytes();
        Some([r, g, b])
    }
}

impl Default for AnnotateConfig {
    fn default() -> Self {
        Self { color: Self::hex(GARNET), size: Self::DEFAULT_SIZE }
    }
}

fn read_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    f32::deserialize(deserializer).map(within_limits)
}

fn write_size<S: Serializer>(size: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f32(within_limits(*size))
}

/// Keeps a size in range. One too large to be a number, like `1e39`, is the default, since it couldn't be saved.
fn within_limits(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(AnnotateConfig::THINNEST, AnnotateConfig::THICKEST)
    } else {
        AnnotateConfig::DEFAULT_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigStore};

    const JADE: [u8; 3] = [63, 196, 138];

    #[test]
    fn old_file_loads_with_garnet_brush() {
        let old: Config = serde_json::from_str(r#"{ "videos": { "quality": 720 } }"#).unwrap();
        assert_eq!(old.annotate, AnnotateConfig::default());
        assert_eq!(old.annotate.color, "#EC545A");
        assert_eq!((old.annotate.rgb(), old.annotate.brush_size()), (GARNET, AnnotateConfig::DEFAULT_SIZE));
        let half: Config = serde_json::from_str(r##"{ "annotate": { "color": "#0E0E0F" } }"##).unwrap();
        assert_eq!((half.annotate.rgb(), half.annotate.size), ([14, 14, 15], AnnotateConfig::DEFAULT_SIZE));
    }

    #[test]
    fn brush_survives_save_and_load() {
        let config = Config { annotate: AnnotateConfig::remembering(JADE, 12.0), ..Config::default() };
        let saved = serde_json::to_string_pretty(&config).unwrap();
        assert!(saved.contains("\"annotate\": {\n    \"color\": \"#3FC48A\",\n    \"size\": 12.0\n  }"), "{saved}");
        let loaded: Config = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded, config);
        assert_eq!((loaded.annotate.rgb(), loaded.annotate.brush_size()), (JADE, 12.0));
    }

    #[test]
    fn brush_is_written_after_videos() {
        let saved = serde_json::to_string_pretty(&Config::default()).unwrap();
        let at = |key: &str| saved.find(key).unwrap_or_else(|| panic!("{key} in {saved}"));
        assert!(at("\"videos\"") < at("\"annotate\"") && at("\"annotate\"") < at("\"permissions\""));
    }

    #[test]
    fn nonsense_colour_becomes_garnet() {
        for nonsense in ["", "#", "red", "#12345", "#1234567", "#GGGGGG", "+12345", "#12 345", "ёжик"] {
            let config = AnnotateConfig { color: nonsense.to_owned(), ..AnnotateConfig::default() };
            assert_eq!(config.rgb(), GARNET, "{nonsense:?}");
            assert_eq!(AnnotateConfig::parse_hex(nonsense), None, "{nonsense:?}");
        }
    }

    #[test]
    fn colour_reads_with_or_without_hash() {
        for spelling in ["#F2C14E", "f2c14e", "  #f2C14e ", "F2C14E"] {
            assert_eq!(AnnotateConfig::parse_hex(spelling), Some([242, 193, 78]), "{spelling:?}");
        }
        assert_eq!(AnnotateConfig::hex([242, 193, 78]), "#F2C14E");
        assert_eq!(AnnotateConfig::hex([0, 10, 255]), "#000AFF");
    }

    #[test]
    fn size_is_clamped_to_the_brush() {
        let sized = |size| AnnotateConfig { size, ..AnnotateConfig::default() }.brush_size();
        assert_eq!(sized(0.0), AnnotateConfig::THINNEST);
        assert_eq!(sized(-4.0), AnnotateConfig::THINNEST);
        assert_eq!(sized(900.0), AnnotateConfig::THICKEST);
        assert_eq!(sized(7.5), 7.5);
        assert_eq!(sized(f32::NAN), AnnotateConfig::DEFAULT_SIZE);
        assert_eq!(sized(f32::INFINITY), AnnotateConfig::DEFAULT_SIZE);
    }

    #[test]
    fn impossible_size_is_tidied_on_read() {
        let read =
            |size: &str| -> AnnotateConfig { serde_json::from_str(&format!(r#"{{ "size": {size} }}"#)).unwrap() };
        assert_eq!(read("1e39").size, AnnotateConfig::DEFAULT_SIZE, "too large for the file to hold");
        assert_eq!(read("-1e39").size, AnnotateConfig::DEFAULT_SIZE);
        assert_eq!((read("1e9").size, read("-3").size, read("0").size), (32.0, 2.0, 2.0));
        assert_eq!((read("7.5").size, read("32").size), (7.5, 32.0));
    }

    #[test]
    fn unwritable_size_never_reaches_file() {
        for unwritable in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e9] {
            let held = AnnotateConfig { size: unwritable, ..AnnotateConfig::default() };
            let saved = serde_json::to_string(&held).unwrap();
            assert!(!saved.contains("null"), "{saved}");
            let loaded: AnnotateConfig = serde_json::from_str(&saved).unwrap();
            assert_eq!(loaded.size, held.brush_size(), "{saved}");
        }
    }

    #[test]
    fn huge_typed_size_still_saves() {
        let dir = std::env::temp_dir().join(format!("notchbase-annotate-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r##"{ "annotate": { "color": "#3FC48A", "size": 1e39 } }"##).unwrap();
        let (mut store, opened) = ConfigStore::open(Some(path.clone()), None);
        let updated = store.update(|config| config.displays.all = !config.displays.all);
        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let (reopened, reopened_with) = ConfigStore::open(Some(path), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((opened, reopened_with), (None, None), "{written}");
        assert!(updated.is_ok() && !written.contains("null"), "{written}");
        assert_eq!(reopened.config(), store.config());
        assert_eq!(
            reopened.config().annotate,
            AnnotateConfig::remembering(JADE, AnnotateConfig::DEFAULT_SIZE),
            "{written}"
        );
    }

    #[test]
    fn remembered_size_is_tidy_and_fits() {
        assert_eq!(AnnotateConfig::remembering(GARNET, 7.26).size, 7.5);
        assert_eq!(AnnotateConfig::remembering(GARNET, 7.24).size, 7.0);
        assert_eq!(AnnotateConfig::remembering(GARNET, 1000.0).size, AnnotateConfig::THICKEST);
        let unsaveable = AnnotateConfig::remembering(GARNET, f32::NAN);
        assert_eq!(unsaveable.size, AnnotateConfig::DEFAULT_SIZE);
        let saved = serde_json::to_string(&unsaveable).unwrap();
        assert_eq!(serde_json::from_str::<AnnotateConfig>(&saved).unwrap(), unsaveable);
    }

    #[test]
    fn wrong_size_still_loads_the_colour() {
        let config: Config = serde_json::from_str(r#"{ "annotate": { "color": "f4f3f0", "size": -3 } }"#).unwrap();
        assert_eq!((config.annotate.rgb(), config.annotate.brush_size()), ([244, 243, 240], AnnotateConfig::THINNEST));
    }
}
