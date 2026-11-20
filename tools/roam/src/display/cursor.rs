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

impl Shape {
    /// The pointer as straight RGBA, for a lending computer that draws it itself, with its
    /// size. Pixels that invert what's under them can't be drawn that way: they come out
    /// black with a white edge, which shows on any background (Windows' text cursor is all
    /// of them).
    pub fn rgba(&self) -> ([u32; 2], Vec<u8>) {
        let [w, h] = [self.width, self.rows()];
        let mut out = vec![0u8; w * h * BYTES_PER_PIXEL];
        let mut inverts = vec![false; w * h];
        for row in 0..h {
            for column in 0..w {
                let at = row * w + column;
                let (pixel, invert) = self.straight(row, column);
                out[at * BYTES_PER_PIXEL..][..BYTES_PER_PIXEL].copy_from_slice(&pixel);
                inverts[at] = invert;
            }
        }
        for row in 0..h {
            for column in 0..w {
                let at = row * w + column;
                let pixel = &mut out[at * BYTES_PER_PIXEL..][..BYTES_PER_PIXEL];
                if inverts[at] {
                    pixel.copy_from_slice(&[0, 0, 0, 255]);
                } else if pixel[3] == 0 && near_invert(&inverts, w, h, row, column) {
                    pixel.copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        ([w as u32, h as u32], out)
    }

    /// One pixel as RGBA, or that it inverts what's under it.
    fn straight(&self, row: usize, column: usize) -> ([u8; 4], bool) {
        const CLEAR: ([u8; 4], bool) = ([0, 0, 0, 0], false);
        match self.kind {
            Kind::Monochrome => match (self.mask_bit(row, column), self.mask_bit(row + self.rows(), column)) {
                (false, false) => ([0, 0, 0, 255], false),
                (false, true) => ([255, 255, 255, 255], false),
                (true, false) => CLEAR,
                (true, true) => (CLEAR.0, true),
            },
            Kind::Color => match self.texel(row, column) {
                Some(&[b, g, r, a]) => ([r, g, b, a], false),
                _ => CLEAR,
            },
            Kind::MaskedColor => match self.texel(row, column) {
                Some(&[b, g, r, 0]) => ([r, g, b, 255], false),
                Some(&[0, 0, 0, _]) => CLEAR,
                Some(_) => (CLEAR.0, true),
                None => CLEAR,
            },
        }
    }
}

/// Whether a pixel touches one that inverts, across or corner to corner.
fn near_invert(inverts: &[bool], w: usize, h: usize, row: usize, column: usize) -> bool {
    (row.saturating_sub(1)..(row + 2).min(h))
        .any(|y| (column.saturating_sub(1)..(column + 2).min(w)).any(|x| inverts[y * w + x]))
}

#[derive(Default)]
pub struct Pointer {
    pub shape: Option<Shape>,
    pub at: [i32; 2],
    pub visible: bool,
}

impl Pointer {
    /// The part of a `size` frame at `offset` the pointer covers, as left, top, width and
    /// height: drawing it on the graphics card copies just this part off and back.
    pub fn area(&self, size: [usize; 2], offset: [i32; 2]) -> Option<[usize; 4]> {
        let shape = self.shape.as_ref().filter(|_| self.visible)?;
        let [left, top] = [0, 1].map(|axis| i64::from(self.at[axis]) - i64::from(offset[axis]));
        let [right, bottom] = [left + shape.width as i64, top + shape.rows() as i64];
        let [x0, y0] = [left.max(0), top.max(0)];
        let [x1, y1] = [right.min(size[0] as i64), bottom.min(size[1] as i64)];
        (x1 > x0 && y1 > y0).then(|| [x0 as usize, y0 as usize, (x1 - x0) as usize, (y1 - y0) as usize])
    }

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

    #[test]
    fn the_pointer_covers_only_what_it_overlaps() {
        let shape = Shape { kind: Kind::Monochrome, width: 32, height: 64, pitch: 4, pixels: vec![0; 256] };
        let mut pointer = Pointer { shape: Some(shape), at: [110, 210], visible: true };
        // A monochrome shape is half as tall as its masks; the frame starts at (100, 200).
        assert_eq!(pointer.area([1920, 1080], [100, 200]), Some([10, 10, 32, 32]));
        pointer.at = [90, 1270];
        assert_eq!(pointer.area([1920, 1080], [100, 200]), Some([0, 1070, 22, 10]), "cut at the edges");
        pointer.at = [5000, 210];
        assert_eq!(pointer.area([1920, 1080], [100, 200]), None, "off the frame");
        pointer.at = [110, 210];
        pointer.visible = false;
        assert_eq!(pointer.area([1920, 1080], [100, 200]), None, "hidden");
    }

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

    /// RGBA laid over a BGRA background, as the lending computer draws it.
    fn over(rgba: &[u8], background: [u8; 4]) -> [u8; 3] {
        let alpha = u32::from(rgba[3]);
        let blend =
            |top: u8, under: u8| ((u32::from(top) * alpha + u32::from(under) * (255 - alpha) + 127) / 255) as u8;
        [blend(rgba[2], background[0]), blend(rgba[1], background[1]), blend(rgba[0], background[2])]
    }

    #[test]
    fn drawn_apart_a_pointer_looks_as_drawn_in() {
        let shapes = [
            Shape {
                kind: Kind::Color,
                width: 2,
                height: 2,
                pitch: 8,
                pixels: [[0, 0, 255, 255], [255, 255, 255, 0], [0, 0, 0, 128], [9, 8, 7, 255]].concat(),
            },
            // Black, white and clear: everything but inverting.
            Shape { kind: Kind::Monochrome, width: 3, height: 2, pitch: 1, pixels: vec![0b0010_0000, 0b0100_0000] },
            Shape {
                kind: Kind::MaskedColor,
                width: 2,
                height: 1,
                pitch: 8,
                pixels: [[1, 2, 3, 0], [0, 0, 0, 0xFF]].concat(),
            },
        ];
        for background in [[10, 20, 30, 255], [250, 240, 230, 255]] {
            for shape in &shapes {
                let ([w, h], rgba) = shape.rgba();
                let (w, h) = (w as usize, h as usize);
                assert_eq!(rgba.len(), w * h * 4);
                let mut canvas = frame(w, h, background);
                pointer(Shape { pixels: shape.pixels.clone(), ..*shape }, [0, 0]).draw(&mut canvas, [w, h], [0, 0]);
                for y in 0..h {
                    for x in 0..w {
                        let at = (y * w + x) * 4;
                        assert_eq!(
                            over(&rgba[at..at + 4], background),
                            pixel(&canvas, w, x, y),
                            "{:?} at {x},{y}",
                            shape.kind
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn inverting_pixels_come_out_black_edged_in_white() {
        // A text cursor: one column of inverting pixels in the middle of three.
        let and = [0b1110_0000; 3];
        let xor = [0b0100_0000; 3];
        let shape =
            Shape { kind: Kind::Monochrome, width: 3, height: 6, pitch: 1, pixels: [and, xor].concat().to_vec() };
        let ([w, h], rgba) = shape.rgba();
        assert_eq!([w, h], [3, 3]);
        for row in rgba.chunks(12) {
            assert_eq!(row, [[255, 255, 255, 255], [0, 0, 0, 255], [255, 255, 255, 255]].concat());
        }
        let masked =
            Shape { kind: Kind::MaskedColor, width: 1, height: 1, pitch: 4, pixels: vec![0xFF, 0xFF, 0xFF, 0xFF] };
        assert_eq!(masked.rgba().1, [0, 0, 0, 255]);
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
