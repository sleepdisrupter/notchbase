//! Pixels in a plain buffer, and the lossless reductions: 16-bit samples that are really 8-bit, an alpha that is all
//! opaque, and gray stored as color.

use image::DynamicImage;

pub(super) struct Pixels {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) channels: usize,
    pub(super) sample_bytes: usize,
    pub(super) data: Vec<u8>,
}

impl Pixels {
    pub(super) fn from_image(image: &DynamicImage) -> Self {
        use DynamicImage as D;
        let big_endian = |samples: &[u16]| samples.iter().flat_map(|s| s.to_be_bytes()).collect::<Vec<u8>>();
        let (channels, sample_bytes, data) = match image {
            D::ImageLuma8(buffer) => (1, 1, buffer.as_raw().clone()),
            D::ImageLumaA8(buffer) => (2, 1, buffer.as_raw().clone()),
            D::ImageRgb8(buffer) => (3, 1, buffer.as_raw().clone()),
            D::ImageRgba8(buffer) => (4, 1, buffer.as_raw().clone()),
            D::ImageLuma16(buffer) => (1, 2, big_endian(buffer.as_raw())),
            D::ImageLumaA16(buffer) => (2, 2, big_endian(buffer.as_raw())),
            D::ImageRgb16(buffer) => (3, 2, big_endian(buffer.as_raw())),
            _ => (4, 2, big_endian(image.to_rgba16().as_raw())),
        };
        Self { width: image.width(), height: image.height(), channels, sample_bytes, data }
    }

    pub(super) fn len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub(super) fn pixel_bytes(&self) -> usize {
        self.channels * self.sample_bytes
    }

    pub(super) fn has_color(&self) -> bool {
        self.channels >= 3
    }

    pub(super) fn reduce_losslessly(&mut self, may_drop_color: bool) {
        self.drop_redundant_low_bytes();
        self.drop_opaque_alpha();
        if may_drop_color {
            self.drop_equal_color_channels();
        }
    }

    fn drop_redundant_low_bytes(&mut self) {
        if self.sample_bytes != 2 {
            return;
        }
        let every_sample_is_8_bit_scaled = self.data.as_chunks::<2>().0.iter().all(|sample| sample[0] == sample[1]);
        if every_sample_is_8_bit_scaled {
            self.data = self.data.iter().step_by(2).copied().collect();
            self.sample_bytes = 1;
        }
    }

    fn drop_opaque_alpha(&mut self) {
        let has_alpha = self.channels == 2 || self.channels == 4;
        if !has_alpha {
            return;
        }
        let stride = self.pixel_bytes();
        let color_bytes = stride - self.sample_bytes;
        let opaque = self.data.chunks_exact(stride).all(|pixel| pixel[color_bytes..].iter().all(|&byte| byte == 0xFF));
        if opaque {
            self.data = self.data.chunks_exact(stride).flat_map(|pixel| &pixel[..color_bytes]).copied().collect();
            self.channels -= 1;
        }
    }

    fn drop_equal_color_channels(&mut self) {
        if !self.has_color() {
            return;
        }
        let (stride, sample) = (self.pixel_bytes(), self.sample_bytes);
        let all_gray = self.data.chunks_exact(stride).all(|pixel| {
            let (red, green, blue) = (&pixel[..sample], &pixel[sample..2 * sample], &pixel[2 * sample..3 * sample]);
            red == green && red == blue
        });
        if all_gray {
            self.data = self
                .data
                .chunks_exact(stride)
                .flat_map(|pixel| pixel[..sample].iter().chain(&pixel[3 * sample..]))
                .copied()
                .collect();
            self.channels -= 2;
        }
    }
}
