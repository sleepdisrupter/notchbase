//! App names and icons, loaded once per app and then kept.

use std::collections::HashMap;
use std::sync::Arc;

pub struct AppLabel {
    pub name: String,
    pub icon: Option<Arc<egui::ColorImage>>,
}

#[derive(Default)]
pub struct AppLabels(HashMap<String, AppLabel>);

impl AppLabels {
    pub fn get_or_load(&mut self, key: &str, load: impl FnOnce() -> AppLabel) -> &AppLabel {
        self.0.entry(key.to_owned()).or_insert_with(load)
    }
}

pub fn capitalized(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalizes_the_first_character_only() {
        assert_eq!(capitalized("éclair bar").as_deref(), Some("Éclair bar"));
        assert_eq!(capitalized("x"), Some("X".to_owned()));
        assert_eq!(capitalized(""), None);
    }

    #[test]
    fn labels_load_once_per_key() {
        let mut labels = AppLabels::default();
        labels.get_or_load("a", || AppLabel { name: "First".into(), icon: None });
        let cached = labels.get_or_load("a", || AppLabel { name: "Second".into(), icon: None });
        assert_eq!(cached.name, "First");
    }
}
