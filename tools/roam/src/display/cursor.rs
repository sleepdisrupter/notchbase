//! Copied from the recorder (src/record/windows/cursor.rs), which the app crate keeps to itself.
//! Draws the mouse pointer onto a captured frame, since duplicated frames leave it out. Monochrome, color and masked
//! color shapes all work, inverting cursors included.

const BYTES_PER_PIXEL: usize = 4;
const RGB: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Monochrome,
    Color,
    MaskedColor,
}

pub struct Shape {
    pub kind: Kind,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub pixels: Vec<u8>,
}

impl Shape {
    /// Rows the pointer covers. A monochrome shape stacks its AND and XOR masks, so it's twice as tall.
    fn rows(&self) -> usize {
        match self.kind {
            Kind::Monochrome => self.height / 2,
            Kind::Color | Kind::MaskedColor => self.height,
        }
    }

    fn mask_bit(&self, row: usize, column: usize) -> bool {
        self.pixels.get(row * self.pitch + column / 8).is_some_and(|byte| byte & (0x80 >> (column % 8)) != 0)
    }

    fn texel(&self, row: usize, column: usize) -> Option<&[u8]> {
        let at = row * self.pitch + column * BYTES_PER_PIXEL;
        self.pixels.get(at..at + BYTES_PER_PIXEL)
    }

    fn paint(&self, row: usize, column: usize, dst: &mut [u8]) {
        match self.kind {
            Kind::Monochrome => {
                let keep = self.mask_bit(row, column);
                let flip = self.mask_bit(row + self.rows(), column);
                for channel in &mut dst[..RGB] {
                    *channel = (if keep { *channel } else { 0 }) ^ (if flip { 0xFF } else { 0 });
                }
            }
            Kind::Color => {
                let Some(src) = self.texel(row, column) else { return };
                let alpha = u32::from(src[3]);
                for (channel, &value) in dst[..RGB].iter_mut().zip(&src[..RGB]) {
                    *channel = ((u32::from(value) * alpha + u32::from(*channel) * (255 - alpha) + 127) / 255) as u8;
                }
            }
            Kind::MaskedColor => {
                let Some(src) = self.texel(row, column) else { return };
                let xor = src[3] != 0;
                for (channel, &value) in dst[..RGB].iter_mut().zip(&src[..RGB]) {
                    *channel = if xor { *channel ^ value } else { value };
                }
            }
        }
    }
}

#[derive(Default)]
pub struct Pointer {
    pub shape: Option<Shape>,
    pub at: [i32; 2],
    pub visible: bool,
}

impl Pointer {
    pub fn draw(&self, frame: &mut [u8], size: [usize; 2], offset: [i32; 2]) {
        let Some(shape) = self.shape.as_ref().filter(|_| self.visible) else { return };
        let [left, top] = [0, 1].map(|axis| i64::from(self.at[axis]) - i64::from(offset[axis]));
        for row in 0..shape.rows() {
            let y = top + row as i64;
            if y < 0 || y >= size[1] as i64 {
                continue;
            }
            for column in 0..shape.width {
                let x = left + column as i64;
                if x < 0 || x >= size[0] as i64 {
                    continue;
                }
                let at = (y as usize * size[0] + x as usize) * BYTES_PER_PIXEL;
                if let Some(dst) = frame.get_mut(at..at + BYTES_PER_PIXEL) {
                    shape.paint(row, column, dst);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat(width * height)
    }

    fn pixel(frame: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let at = (y * width + x) * 4;
        [frame[at], frame[at + 1], frame[at + 2]]
    }

    fn pointer(shape: Shape, at: [i32; 2]) -> Pointer {
        Pointer { shape: Some(shape), at, visible: true }
    }

    #[test]
    fn color_cursors_blend_by_their_alpha() {
        let pixels = [[0, 0, 255, 255], [255, 255, 255, 0], [0, 0, 0, 128], [9, 9, 9, 255]].concat();
        let shape = Shape { kind: Kind::Color, width: 2, height: 2, pitch: 8, pixels };
        let mut canvas = frame(4, 4, [200, 100, 50, 255]);
        pointer(shape, [11, 21]).draw(&mut canvas, [4, 4], [10, 20]);
        assert_eq!(pixel(&canvas, 4, 1, 1), [0, 0, 255]);
        assert_eq!(pixel(&canvas, 4, 2, 1), [200, 100, 50]);
        assert_eq!(pixel(&canvas, 4, 1, 2), [100, 50, 25]);
        assert_eq!(pixel(&canvas, 4, 0, 0), [200, 100, 50]);
    }

    #[test]
    fn monochrome_cursors_keep_or_invert() {
        let pixels = vec![0b1000_0000, 0b0000_0000, 0b1000_0000, 0b1100_0000];
        let shape = Shape { kind: Kind::Monochrome, width: 2, height: 4, pitch: 1, pixels };
        let mut canvas = frame(2, 2, [10, 20, 30, 255]);
        pointer(shape, [0, 0]).draw(&mut canvas, [2, 2], [0, 0]);
        assert_eq!(pixel(&canvas, 2, 0, 0), [245, 235, 225]);
        assert_eq!(pixel(&canvas, 2, 1, 0), [0, 0, 0]);
        assert_eq!(pixel(&canvas, 2, 0, 1), [255, 255, 255]);
        assert_eq!(pixel(&canvas, 2, 1, 1), [255, 255, 255]);
    }

    #[test]
    fn masked_cursors_replace_or_xor() {
        let pixels = [[1, 2, 3, 0], [0xFF, 0, 0, 0xFF]].concat();
        let shape = Shape { kind: Kind::MaskedColor, width: 2, height: 1, pitch: 8, pixels };
        let mut canvas = frame(2, 1, [0x0F, 0x0F, 0x0F, 255]);
        pointer(shape, [0, 0]).draw(&mut canvas, [2, 1], [0, 0]);
        assert_eq!(pixel(&canvas, 2, 0, 0), [1, 2, 3]);
        assert_eq!(pixel(&canvas, 2, 1, 0), [0xF0, 0x0F, 0x0F]);
    }

    #[test]
    fn unseen_cursors_leave_frame_alone() {
        let shape = || Shape { kind: Kind::Color, width: 1, height: 1, pitch: 4, pixels: vec![0, 0, 0, 255] };
        let mut canvas = frame(2, 2, [50, 50, 50, 255]);
        Pointer { visible: false, ..pointer(shape(), [0, 0]) }.draw(&mut canvas, [2, 2], [0, 0]);
        pointer(shape(), [-5, 1]).draw(&mut canvas, [2, 2], [0, 0]);
        pointer(shape(), [7, 7]).draw(&mut canvas, [2, 2], [0, 0]);
        assert!(canvas.chunks(4).all(|px| px == [50, 50, 50, 255]));
    }
}
