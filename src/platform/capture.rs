//! A captured screen as RGBA, and crops of it that repeat the border past the edge. Macs capture another way.

const BYTES_PER_PIXEL: usize = 4;
const OPAQUE: u8 = 255;

pub struct Capture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Capture {
    pub fn pixel(&self, x: i64, y: i64) -> [u8; 3] {
        let x = x.clamp(0, self.width.saturating_sub(1) as i64) as usize;
        let y = y.clamp(0, self.height.saturating_sub(1) as i64) as usize;
        let i = (y * self.width + x) * BYTES_PER_PIXEL;
        self.rgba.get(i..i + 3).map_or([0; 3], |px| [px[0], px[1], px[2]])
    }

    #[cfg(target_os = "windows")]
    pub fn srgb(&self, x: i64, y: i64) -> egui::Color32 {
        let [r, g, b] = self.pixel(x, y);
        egui::Color32::from_rgb(r, g, b)
    }

    fn contains(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        let inside = x + width <= self.width && y + height <= self.height;
        inside && self.rgba.len() >= self.width * self.height * BYTES_PER_PIXEL
    }
}

pub fn to_srgb_image(capture: &Capture, x: usize, y: usize, width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(width * height * BYTES_PER_PIXEL);
    if capture.contains(x, y, width, height) {
        for row in y..y + height {
            let start = (row * capture.width + x) * BYTES_PER_PIXEL;
            out.extend_from_slice(&capture.rgba[start..start + width * BYTES_PER_PIXEL]);
        }
        return out;
    }
    for row in 0..height {
        for col in 0..width {
            let [r, g, b] = capture.pixel((x + col) as i64, (y + row) as i64);
            out.extend_from_slice(&[r, g, b, OPAQUE]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: usize, height: usize) -> Capture {
        let rgba = (0..width * height).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        Capture { width, height, rgba }
    }

    #[test]
    fn inside_crops_copy_rows() {
        let capture = gradient(4, 3);
        let reds: Vec<u8> = to_srgb_image(&capture, 1, 1, 2, 2).chunks_exact(4).map(|px| px[0]).collect();
        assert_eq!(reds, vec![5, 6, 9, 10]);
    }

    #[test]
    fn edge_crops_repeat_the_border() {
        let capture = gradient(2, 2);
        let reds: Vec<u8> = to_srgb_image(&capture, 1, 1, 2, 1).chunks_exact(4).map(|px| px[0]).collect();
        assert_eq!(reds, vec![3, 3]);
    }

    #[test]
    fn empty_capture_reads_black() {
        let empty = Capture { width: 0, height: 0, rgba: Vec::new() };
        assert_eq!(empty.pixel(3, -2), [0, 0, 0]);
        assert_eq!(to_srgb_image(&empty, 0, 0, 1, 1), vec![0, 0, 0, OPAQUE]);
    }
}
