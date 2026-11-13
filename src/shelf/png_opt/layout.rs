//! Ways to lay out the pixels: plain at the fewest bits that hold them, or a palette with translucent colors first.

use std::collections::HashMap;

use png::ColorType;

use super::pixels::Pixels;

const MAX_PALETTE_COLORS: usize = 256;

pub(super) struct Layout {
    pub(super) color: ColorType,
    pub(super) depth: u8,
    pub(super) packed_rows: Vec<u8>,
    pub(super) palette: Option<Vec<u8>>,
    pub(super) transparency: Option<Vec<u8>>,
}

impl Layout {
    pub(super) fn plain(pixels: &Pixels) -> Layout {
        let color = match pixels.channels {
            1 => ColorType::Grayscale,
            2 => ColorType::GrayscaleAlpha,
            3 => ColorType::Rgb,
            _ => ColorType::Rgba,
        };
        let eight_bit_gray = pixels.channels == 1 && pixels.sample_bytes == 1;
        let depth = if eight_bit_gray { fewest_gray_bits(&pixels.data) } else { 8 * pixels.sample_bytes as u8 };
        let packed_rows = if depth < 8 {
            let step = 255 / ((1u8 << depth) - 1);
            let levels: Vec<u8> = pixels.data.iter().map(|value| value / step).collect();
            pack(&levels, pixels.width as usize, depth)
        } else {
            pixels.data.clone()
        };
        Layout { color, depth, packed_rows, palette: None, transparency: None }
    }

    pub(super) fn palette(pixels: &Pixels) -> Option<Layout> {
        if pixels.sample_bytes != 1 {
            return None;
        }
        let (colors, indices) = distinct_colors(pixels)?;
        let order = translucent_first(&colors);
        let mut new_index = vec![0u8; colors.len()];
        for (new, &old) in order.iter().enumerate() {
            new_index[old] = new as u8;
        }
        let palette: Vec<u8> = order.iter().flat_map(|&i| colors[i][..3].iter().copied()).collect();
        let translucent = order.iter().take_while(|&&i| colors[i][3] != 255).count();
        let transparency = (translucent > 0).then(|| order[..translucent].iter().map(|&i| colors[i][3]).collect());
        let depth = bits_for_palette(colors.len());
        let indices: Vec<u8> = indices.iter().map(|&i| new_index[usize::from(i)]).collect();
        Some(Layout {
            color: ColorType::Indexed,
            depth,
            packed_rows: pack(&indices, pixels.width as usize, depth),
            palette: Some(palette),
            transparency,
        })
    }
}

fn distinct_colors(pixels: &Pixels) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    let mut index_of: HashMap<[u8; 4], u8> = HashMap::new();
    let mut colors: Vec<[u8; 4]> = Vec::new();
    let mut indices = Vec::with_capacity(pixels.len());
    for pixel in pixels.data.chunks_exact(pixels.channels) {
        let color = as_rgba(pixel);
        let index = match index_of.get(&color) {
            Some(&index) => index,
            None if colors.len() == MAX_PALETTE_COLORS => return None,
            None => {
                let index = colors.len() as u8;
                colors.push(color);
                index_of.insert(color, index);
                index
            }
        };
        indices.push(index);
    }
    Some((colors, indices))
}

fn as_rgba(pixel: &[u8]) -> [u8; 4] {
    match *pixel {
        [gray] => [gray, gray, gray, 255],
        [gray, alpha] => [gray, gray, gray, alpha],
        [red, green, blue] => [red, green, blue, 255],
        [red, green, blue, alpha] => [red, green, blue, alpha],
        _ => [0; 4],
    }
}

fn translucent_first(colors: &[[u8; 4]]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..colors.len()).collect();
    order.sort_by_key(|&i| colors[i][3] == 255);
    order
}

fn bits_for_palette(colors: usize) -> u8 {
    match colors {
        0..=2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        _ => 8,
    }
}

pub(super) fn fewest_gray_bits(values: &[u8]) -> u8 {
    for bits in [1u8, 2, 4] {
        let step = 255 / ((1u8 << bits) - 1);
        if values.iter().all(|value| value % step == 0) {
            return bits;
        }
    }
    8
}

pub(super) fn pack(values: &[u8], width: usize, depth: u8) -> Vec<u8> {
    if depth >= 8 {
        return values.to_vec();
    }
    let per_byte = usize::from(8 / depth);
    let row_bytes = width.div_ceil(per_byte);
    let rows = values.len() / width.max(1);
    let mut out = Vec::with_capacity(row_bytes * rows);
    for row in values.chunks(width.max(1)) {
        for group in row.chunks(per_byte) {
            let mut byte = 0u8;
            for (slot, &value) in group.iter().enumerate() {
                byte |= value << (8 - usize::from(depth) * (slot + 1));
            }
            out.push(byte);
        }
    }
    out
}
