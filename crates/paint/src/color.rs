//! The palette and the color math the painters share.

use egui::Color32;

pub const PEARL: Color32 = Color32::from_rgb(244, 243, 240);
pub const RED: Color32 = Color32::from_rgb(236, 84, 90);
pub const YELLOW: Color32 = Color32::from_rgb(242, 193, 78);
pub const GREEN: Color32 = Color32::from_rgb(63, 196, 138);
pub const INK: Color32 = Color32::from_rgb(14, 14, 15);
pub const GRAY: Color32 = Color32::from_rgb(140, 139, 136);
pub const SURFACE: Color32 = Color32::from_rgb(24, 24, 25);

// The palette is kept small on purpose, so these names fold onto the colors above.
pub const BLUE: Color32 = PEARL;
pub const ORANGE: Color32 = YELLOW;
pub const PURPLE: Color32 = PEARL;
pub const TEAL: Color32 = GREEN;
pub const ACCENT: Color32 = PEARL;

pub const CLAUDE: Color32 = Color32::from_rgb(217, 119, 87);
pub const CHATGPT: Color32 = Color32::from_rgb(16, 163, 127);

const LIGHT_FROM: f32 = 0.5;

pub fn label() -> Color32 {
    white(0.85)
}

pub fn secondary_label() -> Color32 {
    white(0.55)
}

pub fn tertiary_label() -> Color32 {
    white(0.25)
}

pub fn quaternary_label() -> Color32 {
    white(0.10)
}

pub fn white(alpha: f32) -> Color32 {
    let [r, g, b, _] = PEARL.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn is_light(color: Color32) -> bool {
    let [r, g, b] = [color.r(), color.g(), color.b()].map(|c| f32::from(c) / 255.0);
    // Rec. 709 luma on the sRGB values, close enough to pick an ink.
    0.2126 * r + 0.7152 * g + 0.0722 * b > LIGHT_FROM
}

pub fn ink_on(fill: Color32) -> Color32 {
    if is_light(fill) { INK } else { PEARL }
}

const NEUTRAL_BELOW: f32 = 0.15;
const HUES: [(f32, Color32); 5] = [(20.0, RED), (75.0, YELLOW), (190.0, GREEN), (300.0, PEARL), (360.0, RED)];

/// Snaps a color from outside, like a tool's or a calendar's, to the nearest palette hue so it doesn't clash.
pub fn harmonize(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied().map(|c| f32::from(c) / 255.0);
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let saturation = if hi > 0.0 { (hi - lo) / hi } else { 0.0 };
    let snapped = if saturation < NEUTRAL_BELOW {
        GRAY
    } else {
        let hue = hue_of(r, g, b, hi, hi - lo);
        HUES.iter().find(|(upto, _)| hue < *upto).map_or(RED, |(_, color)| *color)
    };
    let [r, g, b, _] = snapped.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

fn hue_of(r: f32, g: f32, b: f32, hi: f32, span: f32) -> f32 {
    let sixth = if hi == r {
        ((g - b) / span).rem_euclid(6.0)
    } else if hi == g {
        (b - r) / span + 2.0
    } else {
        (r - g) / span + 4.0
    };
    sixth * 60.0
}

/// Source-over blend of two premultiplied colors.
pub fn over(src: Color32, dst: Color32) -> Color32 {
    let k = 255 - src.a() as u32;
    let ch = |s: u8, d: u8| (s as u32 + (d as u32 * k + 127) / 255).min(255) as u8;
    Color32::from_rgba_premultiplied(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        ch(src.a(), dst.a()),
    )
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()), ch(a.a(), b.a()))
}

/// The tint of a picture such as album art: an average that favors bright, saturated pixels,
/// brightened to a fixed level.
pub fn vibrant(pixels: &[Color32]) -> Option<Color32> {
    let (mut sum, mut weight) = ([0.0f32; 3], 0.0f32);
    for p in pixels {
        let [r, g, b] = [p.r(), p.g(), p.b()].map(|c| f32::from(c) / 255.0);
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        let w = (hi - lo) * hi + 0.02;
        sum = [sum[0] + r * w, sum[1] + g * w, sum[2] + b * w];
        weight += w;
    }
    if weight <= 0.0 {
        return None;
    }
    let avg = sum.map(|c| c / weight);
    let lift = 0.78 / avg.into_iter().fold(0.0, f32::max).max(0.05);
    let lifted = avg.map(|c| ((c * lift).min(1.0) * 255.0).round() as u8);
    Some(Color32::from_rgb(lifted[0], lifted[1], lifted[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmonize_snaps_to_nearest_hue() {
        let snap = |r, g, b| harmonize(Color32::from_rgb(r, g, b));
        assert_eq!(snap(0xFF, 0x29, 0x68), RED, "the Calendar app's red");
        assert_eq!(snap(0xFF, 0x95, 0x00), YELLOW);
        assert_eq!(snap(0xFF, 0xCC, 0x00), YELLOW);
        assert_eq!(snap(0x63, 0xDA, 0x38), GREEN);
        assert_eq!(snap(0x30, 0xC8, 0xD0), GREEN);
        assert_eq!(snap(0x1B, 0xAD, 0xF8), PEARL, "blues become the palette's pearl");
        assert_eq!(snap(0xCC, 0x73, 0xE1), PEARL);
        assert_eq!(snap(120, 120, 124), GRAY);
        assert_eq!(harmonize(Color32::from_rgba_unmultiplied(255, 0, 0, 128)).a(), 128, "alpha is kept");
    }

    #[test]
    fn ink_reads_on_every_fill() {
        assert_eq!(ink_on(PEARL), INK);
        assert_eq!(ink_on(YELLOW), INK);
        assert_eq!(ink_on(RED), PEARL);
        assert_eq!(ink_on(Color32::BLACK), PEARL);
    }

    #[test]
    fn vibrant_favors_colorful_pixels() {
        let mut art = vec![Color32::from_gray(20); 90];
        art.extend([Color32::from_rgb(120, 20, 40); 10]);
        let tint = vibrant(&art).expect("a tint");
        assert!(tint.r() > 150 && tint.r() > tint.g() * 3, "{tint:?}");
        let gray = vibrant(&[Color32::from_gray(10); 4]).expect("a tint");
        assert!(gray.r() == gray.g() && gray.g() == gray.b() && gray.r() > 150);
        assert_eq!(vibrant(&[]), None);
    }
}
