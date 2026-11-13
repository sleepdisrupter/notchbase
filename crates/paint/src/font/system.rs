//! Where each platform keeps its UI fonts: San Francisco on a Mac, Segoe UI on Windows, DejaVu elsewhere.

use std::path::PathBuf;

#[cfg(windows)]
const WINDOWS_FONTS: &str = r"C:\Windows\Fonts";
#[cfg(target_os = "macos")]
const MACOS_FONTS: &str = "/System/Library/Fonts";

#[derive(Clone, Debug, PartialEq)]
pub struct FontSource {
    pub path: PathBuf,
    pub index: u32,
    pub weight: Option<f32>,
}

impl FontSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), index: 0, weight: None }
    }

    #[cfg_attr(not(any(target_os = "macos", windows, test)), expect(dead_code))]
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemFonts {
    pub regular: Vec<FontSource>,
    pub semibold: Vec<FontSource>,
    pub mono: Vec<FontSource>,
}

#[cfg(windows)]
pub fn system_fonts() -> SystemFonts {
    let fonts = std::env::var_os("WINDIR")
        .map(|windir| PathBuf::from(windir).join("Fonts"))
        .unwrap_or_else(|| PathBuf::from(WINDOWS_FONTS));
    windows_fonts(fonts)
}

#[cfg(target_os = "macos")]
pub fn system_fonts() -> SystemFonts {
    macos_fonts(PathBuf::from(MACOS_FONTS))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn system_fonts() -> SystemFonts {
    other_fonts()
}

#[cfg(any(windows, test))]
fn windows_fonts(fonts: PathBuf) -> SystemFonts {
    SystemFonts {
        regular: vec![FontSource::new(fonts.join("segoeui.ttf"))],
        semibold: vec![FontSource::new(fonts.join("seguisb.ttf"))],
        mono: vec![
            FontSource::new(fonts.join("CascadiaMono.ttf")).weight(600.0),
            FontSource::new(fonts.join("consolab.ttf")),
            FontSource::new(fonts.join("consola.ttf")),
        ],
    }
}

#[cfg(any(target_os = "macos", test))]
fn macos_fonts(fonts: PathBuf) -> SystemFonts {
    SystemFonts {
        regular: vec![
            FontSource::new(fonts.join("SFNS.ttf")).weight(400.0),
            FontSource::new(fonts.join("Helvetica.ttc")),
        ],
        semibold: vec![
            FontSource::new(fonts.join("SFNS.ttf")).weight(590.0),
            FontSource::new(fonts.join("Helvetica.ttc")),
        ],
        mono: vec![FontSource::new(fonts.join("SFNSMono.ttf")).weight(560.0), FontSource::new(fonts.join("Menlo.ttc"))],
    }
}

#[cfg(any(not(any(windows, target_os = "macos")), test))]
fn other_fonts() -> SystemFonts {
    SystemFonts {
        regular: vec![FontSource::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")],
        semibold: vec![FontSource::new("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")],
        mono: vec![FontSource::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")],
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn names(sources: &[FontSource]) -> Vec<(String, Option<f32>)> {
        let name = |source: &FontSource| source.path.file_name().unwrap().to_string_lossy().into_owned();
        sources.iter().map(|source| (name(source), source.weight)).collect()
    }

    #[test]
    fn new_source_is_first_face() {
        let source = FontSource::new("/fonts/a.ttf");
        assert_eq!(source, FontSource { path: PathBuf::from("/fonts/a.ttf"), index: 0, weight: None });
        assert_eq!(source.weight(590.0).weight, Some(590.0));
    }

    #[test]
    fn windows_uses_segoe_and_cascadia() {
        let fonts = windows_fonts(PathBuf::from("W"));
        assert_eq!(fonts.regular[0].path, Path::new("W").join("segoeui.ttf"));
        assert_eq!(names(&fonts.semibold), [("seguisb.ttf".to_owned(), None)]);
        assert_eq!(
            names(&fonts.mono),
            [
                ("CascadiaMono.ttf".to_owned(), Some(600.0)),
                ("consolab.ttf".to_owned(), None),
                ("consola.ttf".to_owned(), None)
            ]
        );
    }

    #[test]
    fn mac_uses_sf_with_fallbacks() {
        let fonts = macos_fonts(PathBuf::from("/System/Library/Fonts"));
        assert_eq!(names(&fonts.regular), [("SFNS.ttf".to_owned(), Some(400.0)), ("Helvetica.ttc".to_owned(), None)]);
        assert_eq!(names(&fonts.semibold), [("SFNS.ttf".to_owned(), Some(590.0)), ("Helvetica.ttc".to_owned(), None)]);
        assert_eq!(names(&fonts.mono), [("SFNSMono.ttf".to_owned(), Some(560.0)), ("Menlo.ttc".to_owned(), None)]);
        assert!(fonts.mono[1].path.starts_with("/System/Library/Fonts"));
    }

    #[test]
    fn other_systems_use_dejavu() {
        let fonts = other_fonts();
        for family in [&fonts.regular, &fonts.semibold, &fonts.mono] {
            assert_eq!(family.len(), 1);
            assert!(family[0].path.starts_with("/usr/share/fonts/truetype/dejavu"));
        }
        assert_eq!(names(&fonts.semibold), [("DejaVuSans-Bold.ttf".to_owned(), None)]);
    }

    #[test]
    fn every_family_has_a_first_choice() {
        let fonts = system_fonts();
        assert!(!fonts.regular.is_empty() && !fonts.semibold.is_empty() && !fonts.mono.is_empty());
        assert!(fonts.regular.iter().chain(&fonts.semibold).chain(&fonts.mono).all(|source| source.index == 0));
    }
}
