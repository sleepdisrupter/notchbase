//! The brush: a colour and a size from 2 to 32 points, which also sets how tall letters are. The config remembers it.

use egui::Color32;

use crate::config::AnnotateConfig;
use crate::ui::color;

const LETTER_BASE: f32 = 10.0;
const LETTER_PER_POINT: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    color: Color32,
    size: f32,
}

impl Brush {
    pub const THINNEST: f32 = AnnotateConfig::THINNEST;
    pub const THICKEST: f32 = AnnotateConfig::THICKEST;
    pub const DEFAULT_SIZE: f32 = AnnotateConfig::DEFAULT_SIZE;
    pub const PALETTE: [Color32; 5] = [color::RED, color::YELLOW, color::GREEN, color::PEARL, color::INK];
    pub const DEFAULT_COLOR: Color32 = Self::PALETTE[0];

    pub fn new(color: Color32, size: f32) -> Self {
        Self { color: Self::DEFAULT_COLOR, size: Self::DEFAULT_SIZE }.with_color(color).with_size(size)
    }

    pub fn color(self) -> Color32 {
        self.color
    }

    pub fn size(self) -> f32 {
        self.size
    }

    pub fn letter_size(self) -> f32 {
        LETTER_BASE + LETTER_PER_POINT * self.size
    }

    pub fn with_color(self, color: Color32) -> Self {
        Self { color: Color32::from_rgb(color.r(), color.g(), color.b()), ..self }
    }

    pub fn with_size(self, size: f32) -> Self {
        let size = if size.is_finite() { size.clamp(Self::THINNEST, Self::THICKEST) } else { self.size };
        Self { size, ..self }
    }
}

impl Default for Brush {
    fn default() -> Self {
        Self { color: Self::DEFAULT_COLOR, size: Self::DEFAULT_SIZE }
    }
}

impl From<&AnnotateConfig> for Brush {
    fn from(saved: &AnnotateConfig) -> Self {
        let [r, g, b] = saved.rgb();
        Self::new(Color32::from_rgb(r, g, b), saved.brush_size())
    }
}

impl From<Brush> for AnnotateConfig {
    fn from(brush: Brush) -> Self {
        Self::remembering([brush.color.r(), brush.color.g(), brush.color.b()], brush.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(color: Color32) -> String {
        AnnotateConfig::from(Brush::default().with_color(color)).color
    }

    #[test]
    fn five_opaque_swatches_garnet_first() {
        let swatches: Vec<String> = Brush::PALETTE.into_iter().map(hex).collect();
        assert_eq!(swatches, ["#EC545A", "#F2C14E", "#3FC48A", "#F4F3F0", "#0E0E0F"]);
        assert_eq!(Brush::PALETTE, [color::RED, color::YELLOW, color::GREEN, color::PEARL, color::INK]);
        assert!(Brush::PALETTE.iter().all(|swatch| swatch.is_opaque()));
    }

    #[test]
    fn fresh_brush_is_garnet_size_five() {
        let brush = Brush::default();
        assert_eq!((brush.color(), brush.size()), (color::RED, 5.0));
        assert_eq!(brush, Brush::new(Brush::DEFAULT_COLOR, Brush::DEFAULT_SIZE));
        assert!((Brush::THINNEST..=Brush::THICKEST).contains(&Brush::DEFAULT_SIZE));
        assert_eq!((Brush::THINNEST, Brush::THICKEST), (2.0, 32.0));
    }

    #[test]
    fn new_config_holds_fresh_brush() {
        assert_eq!(Brush::from(&AnnotateConfig::default()), Brush::default());
        assert_eq!(AnnotateConfig::from(Brush::default()), AnnotateConfig::default());
    }

    #[test]
    fn stray_size_is_pulled_back_in() {
        let brush = Brush::default();
        assert_eq!(brush.with_size(0.5).size(), Brush::THINNEST);
        assert_eq!(brush.with_size(400.0).size(), Brush::THICKEST);
        assert_eq!(brush.with_size(11.25).size(), 11.25);
        assert_eq!(brush.with_size(f32::NAN).size(), Brush::DEFAULT_SIZE);
        assert_eq!(Brush::new(color::INK, -1.0).size(), Brush::THINNEST);
    }

    #[test]
    fn letters_grow_from_14_to_74() {
        let letters = |size: f32| Brush::default().with_size(size).letter_size();
        assert_eq!([Brush::THINNEST, Brush::DEFAULT_SIZE, Brush::THICKEST].map(letters), [14.0, 20.0, 74.0]);
        assert_eq!((letters(9.5), letters(400.0), letters(f32::NAN)), (29.0, 74.0, 20.0));
        assert_eq!(Brush::default().with_color(color::INK).letter_size(), Brush::default().letter_size());
    }

    #[test]
    fn brush_colour_is_opaque() {
        let glassy = Color32::from_rgba_premultiplied(40, 30, 20, 60);
        assert_eq!(Brush::default().with_color(glassy).color(), Color32::from_rgb(40, 30, 20));
        assert_eq!(Brush::default().with_color(color::GREEN).size(), Brush::DEFAULT_SIZE);
    }

    #[test]
    fn brush_round_trips_through_config() {
        for (swatch, size) in Brush::PALETTE.into_iter().zip([2.0, 5.0, 9.5, 20.0, 32.0]) {
            let brush = Brush::new(swatch, size);
            let saved = AnnotateConfig::from(brush);
            assert_eq!(saved.color, hex(swatch));
            assert_eq!(Brush::from(&saved), brush);
        }
    }

    #[test]
    fn damaged_config_gives_usable_brush() {
        let damaged: AnnotateConfig = serde_json::from_str(r#"{ "color": "mauve", "size": 1.0e9 }"#).expect("a config");
        assert_eq!(Brush::from(&damaged), Brush::new(Brush::DEFAULT_COLOR, Brush::THICKEST));
    }
}
