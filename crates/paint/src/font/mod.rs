//! Puts the system UI fonts into egui and names the sizes the panels use.

mod system;

use std::sync::Arc;

use egui::epaint::text::{FontTweak, VariationCoords};
use egui::{FontData, FontDefinitions, FontFamily, FontId};

use system::FontSource;

const SEMIBOLD_FAMILY: &str = "semibold";

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, semibold_family())
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn headline() -> FontId {
    semibold(13.0)
}

pub fn body() -> FontId {
    regular(13.0)
}

pub fn callout() -> FontId {
    regular(12.0)
}

pub fn subheadline() -> FontId {
    regular(11.0)
}

pub fn section() -> FontId {
    semibold(11.0)
}

pub fn caption() -> FontId {
    regular(10.0)
}

pub fn value() -> FontId {
    mono(12.0)
}

/// Loads the system fonts, with egui's own fonts kept behind them as fallbacks.
pub fn install(ctx: &egui::Context) {
    let fonts = system::system_fonts();
    let mut defs = FontDefinitions::default();
    let text_fallbacks = family_members(&defs, &FontFamily::Proportional);
    let mono_fallbacks = family_members(&defs, &FontFamily::Monospace);

    let regular = register(&mut defs, "ui", &fonts.regular);
    let semibold = register(&mut defs, "ui-semibold", &fonts.semibold).or_else(|| regular.clone());
    let mono = register(&mut defs, "ui-mono", &fonts.mono);

    defs.families.insert(FontFamily::Proportional, with_fallbacks(regular, &text_fallbacks));
    defs.families.insert(semibold_family(), with_fallbacks(semibold, &text_fallbacks));
    defs.families.insert(FontFamily::Monospace, with_fallbacks(mono, &mono_fallbacks));
    ctx.set_fonts(defs);
}

fn semibold_family() -> FontFamily {
    FontFamily::Name(SEMIBOLD_FAMILY.into())
}

fn family_members(defs: &FontDefinitions, family: &FontFamily) -> Vec<String> {
    defs.families.get(family).cloned().unwrap_or_default()
}

fn register(defs: &mut FontDefinitions, name: &str, sources: &[FontSource]) -> Option<String> {
    let data = sources.iter().find_map(load)?;
    defs.font_data.insert(name.to_owned(), Arc::new(data));
    Some(name.to_owned())
}

fn with_fallbacks(primary: Option<String>, fallbacks: &[String]) -> Vec<String> {
    primary.into_iter().chain(fallbacks.iter().cloned()).collect()
}

fn load(source: &FontSource) -> Option<FontData> {
    let bytes = std::fs::read(&source.path).ok()?;
    let mut data = FontData::from_owned(bytes);
    data.index = source.index;
    if let Some(weight) = source.weight {
        data = data.tweak(FontTweak { coords: VariationCoords::new([(b"wght", weight)]), ..Default::default() });
    }
    Some(data)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const NOT_A_FONT: &[u8] = b"not really a font";

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("notchbase-paint-font-{}-{name}", std::process::id()))
    }

    #[test]
    fn named_sizes_keep_families() {
        let semibold = FontFamily::Name(SEMIBOLD_FAMILY.into());
        assert_eq!(body(), FontId::new(13.0, FontFamily::Proportional));
        assert_eq!(headline(), FontId::new(13.0, semibold.clone()));
        assert_eq!(section(), FontId::new(11.0, semibold));
        assert_eq!(value(), FontId::new(12.0, FontFamily::Monospace));
        assert_eq!([callout().size, subheadline().size, caption().size], [12.0, 11.0, 10.0]);
        assert_eq!(regular(9.5).family, FontFamily::Proportional);
    }

    #[test]
    fn fallbacks_follow_first_choice() {
        let fallbacks = ["a".to_owned(), "b".to_owned()];
        assert_eq!(with_fallbacks(Some("ui".to_owned()), &fallbacks), ["ui", "a", "b"]);
        assert_eq!(with_fallbacks(None, &fallbacks), ["a", "b"]);
        assert!(with_fallbacks(None, &[]).is_empty());
    }

    #[test]
    fn unknown_family_is_empty() {
        let defs = FontDefinitions::default();
        assert!(!family_members(&defs, &FontFamily::Proportional).is_empty());
        assert!(family_members(&defs, &semibold_family()).is_empty());
    }

    #[test]
    fn missing_file_registers_nothing() {
        let mut defs = FontDefinitions::default();
        let before = defs.font_data.len();
        let missing = [FontSource::new(temp("missing.ttf"))];
        assert_eq!(register(&mut defs, "ui", &missing), None);
        assert_eq!(register(&mut defs, "ui", &[]), None);
        assert_eq!(defs.font_data.len(), before);
        assert_eq!(load(&missing[0]), None);
    }

    #[test]
    fn first_readable_source_wins() {
        let path = temp("face.ttf");
        std::fs::write(&path, NOT_A_FONT).unwrap();
        let mut weighted = FontSource::new(&path).weight(590.0);
        weighted.index = 2;
        let mut defs = FontDefinitions::default();
        let sources = [FontSource::new(temp("absent.ttf")), weighted];
        assert_eq!(register(&mut defs, "ui", &sources), Some("ui".to_owned()));
        let data = &defs.font_data["ui"];
        assert_eq!((data.font.as_ref(), data.index), (NOT_A_FONT, 2));
        assert_ne!(data.tweak, FontTweak::default());
        let plain = load(&FontSource::new(&path)).expect("a readable file");
        assert_eq!((plain.index, plain.tweak), (0, FontTweak::default()));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn install_fills_every_family() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let families = ui.ctx().fonts(|fonts| fonts.definitions().families.clone());
            for family in [FontFamily::Proportional, FontFamily::Monospace, semibold_family()] {
                assert!(families.get(&family).is_some_and(|members| !members.is_empty()), "{family:?}");
            }
        });
        output.textures_delta.clear();
    }
}
