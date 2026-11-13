//! Writes a color as hex, RGB or HSL.

use notchbase_sdk::prelude::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl Format {
    pub const CHOICES: [(&str, &str); 3] = [("hex", "HEX"), ("rgb", "RGB"), ("hsl", "HSL")];

    pub fn named(name: &str) -> Self {
        match name {
            "rgb" => Format::Rgb,
            "hsl" => Format::Hsl,
            _ => Format::Hex,
        }
    }

    pub fn write(self, color: Color) -> String {
        match self {
            Format::Hex => color.hex(),
            Format::Rgb => format!("rgb({}, {}, {})", color.r, color.g, color.b),
            Format::Hsl => {
                let [h, s, l] = hsl(color);
                format!("hsl({h}, {s}%, {l}%)")
            }
        }
    }
}

fn hsl(color: Color) -> [u32; 3] {
    let [r, g, b] = [color.r, color.g, color.b].map(|channel| f32::from(channel) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return [0, 0, (lightness * 100.0).round() as u32];
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    [hue.round() as u32 % 360, (saturation * 100.0).round() as u32, (lightness * 100.0).round() as u32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_each_format() {
        let blue = Color::rgb(0x3A, 0x7B, 0xFF);
        assert_eq!(Format::Hex.write(blue), "#3A7BFF");
        assert_eq!(Format::Rgb.write(blue), "rgb(58, 123, 255)");
        assert_eq!(Format::Hsl.write(blue), "hsl(220, 100%, 61%)");
    }

    #[test]
    fn grays_land_on_round_numbers() {
        assert_eq!(Format::Hsl.write(Color::rgb(128, 128, 128)), "hsl(0, 0%, 50%)");
        assert_eq!(Format::Hsl.write(Color::rgb(255, 0, 0)), "hsl(0, 100%, 50%)");
        assert_eq!(Format::Hsl.write(Color::rgb(0, 255, 0)), "hsl(120, 100%, 50%)");
        assert_eq!(Format::Hsl.write(Color::rgb(255, 0, 255)), "hsl(300, 100%, 50%)");
    }

    #[test]
    fn unknown_format_is_hex() {
        assert_eq!(Format::named("cmyk"), Format::Hex);
        assert_eq!(Format::named("hsl"), Format::Hsl);
    }
}
