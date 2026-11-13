use std::borrow::Cow;
use std::io::Cursor;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage};
use png::{BitDepth, ColorType, Info};

use super::layout::{Layout, fewest_gray_bits, pack};
use super::pixels::Pixels;
use super::{Error, RenderingChunks, encode_image, lossless_layouts, optimize};
use crate::shelf::tests::fixtures::icc_profile;

struct Header {
    color: ColorType,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
}

fn header(png_file: &[u8]) -> Header {
    let reader = png::Decoder::new(Cursor::new(png_file)).read_info().unwrap();
    let info = reader.info();
    Header {
        color: info.color_type,
        icc: info.icc_profile.as_deref().map(<[u8]>::to_vec),
        exif: info.exif_metadata.as_deref().map(<[u8]>::to_vec),
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn claiming_to_be(side: u32, mut png_file: Vec<u8>) -> Vec<u8> {
    png_file[16..20].copy_from_slice(&side.to_be_bytes());
    png_file[20..24].copy_from_slice(&side.to_be_bytes());
    let crc = crc32(&png_file[12..29]);
    png_file[29..33].copy_from_slice(&crc.to_be_bytes());
    png_file
}

fn uneven_grays() -> DynamicImage {
    DynamicImage::ImageLuma8(GrayImage::from_fn(40, 40, |x, _| Luma([[3, 70, 101, 190, 250][x as usize % 5]])))
}

#[test]
fn packs_rows_on_byte_boundaries() {
    assert_eq!(pack(&[1, 0, 1, 1, 1, 0], 3, 1), vec![0b1010_0000, 0b1100_0000]);
    assert_eq!(pack(&[3, 2, 1, 0, 3], 5, 2), vec![0b1110_0100, 0b1100_0000]);
    assert_eq!(pack(&[15, 1], 2, 4), vec![0xF1]);
    assert_eq!(pack(&[7, 8], 2, 8), vec![7, 8]);
}

#[test]
fn gray_bits_are_exact() {
    assert_eq!(fewest_gray_bits(&[0, 255]), 1);
    assert_eq!(fewest_gray_bits(&[0, 85, 170, 255]), 2);
    assert_eq!(fewest_gray_bits(&[0, 17, 34, 255]), 4);
    assert_eq!(fewest_gray_bits(&[0, 1]), 8);
}

#[test]
fn black_and_white_becomes_1_bit() {
    let mut pixels = Pixels {
        width: 2,
        height: 1,
        channels: 4,
        sample_bytes: 2,
        data: vec![0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255],
    };
    pixels.reduce_losslessly(true);
    assert_eq!((pixels.channels, pixels.sample_bytes), (1, 1));
    assert_eq!(pixels.data, vec![0, 255]);
    let plain = Layout::plain(&pixels);
    assert_eq!((plain.color, plain.depth, plain.packed_rows), (ColorType::Grayscale, 1, vec![0b0100_0000]));
}

#[test]
fn real_16_bit_and_alpha_stay() {
    let mut pixels = Pixels { width: 1, height: 1, channels: 2, sample_bytes: 2, data: vec![0x12, 0x34, 0x80, 0x00] };
    pixels.reduce_losslessly(true);
    assert_eq!((pixels.channels, pixels.sample_bytes), (2, 2));
}

#[test]
fn palette_puts_translucent_first() {
    let pixels = Pixels {
        width: 3,
        height: 1,
        channels: 4,
        sample_bytes: 1,
        data: vec![255, 0, 0, 255, 0, 255, 0, 128, 255, 0, 0, 255],
    };
    let palette = Layout::palette(&pixels).unwrap();
    assert_eq!(palette.palette.unwrap(), vec![0, 255, 0, 255, 0, 0]);
    assert_eq!(palette.transparency.unwrap(), vec![128]);
    assert_eq!(palette.depth, 1);
    assert_eq!(palette.packed_rows, vec![0b1010_0000]);
}

#[test]
fn rgb_profile_keeps_color() {
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(16, 16, |x, y| {
        let level = (x * 16 + y) as u8;
        Rgb([level, level, level])
    }));
    let profile = icc_profile(b"RGB ");
    let encoded = header(&encode_image(&image, Some(profile.clone())).unwrap());
    assert!(matches!(encoded.color, ColorType::Rgb | ColorType::Indexed), "{:?}", encoded.color);
    assert_eq!(encoded.icc, Some(profile));
    assert_eq!(header(&encode_image(&image, None).unwrap()).color, ColorType::Grayscale);
}

fn offers_a_palette(icc: Option<Vec<u8>>) -> bool {
    let mut pixels = Pixels::from_image(&uneven_grays());
    let mut chunks = RenderingChunks { icc, ..RenderingChunks::default() };
    lossless_layouts(&mut pixels, &mut chunks).iter().any(|layout| layout.color == ColorType::Indexed)
}

#[test]
fn gray_profile_skips_palette() {
    let profile = icc_profile(b"GRAY");
    assert!(offers_a_palette(None));
    assert!(!offers_a_palette(Some(profile.clone())));
    let encoded = header(&encode_image(&uneven_grays(), Some(profile.clone())).unwrap());
    assert_eq!((encoded.color, encoded.icc), (ColorType::Grayscale, Some(profile)));
}

#[test]
fn wrong_profile_is_dropped() {
    let encoded = header(&encode_image(&uneven_grays(), Some(icc_profile(b"CMYK"))).unwrap());
    assert_eq!(encoded.icc, None);
}

#[test]
fn exif_metadata_survives_optimizing() {
    let exif = vec![b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 0];
    let mut info = Info::with_size(4, 4);
    info.color_type = ColorType::Rgb;
    info.bit_depth = BitDepth::Eight;
    info.exif_metadata = Some(Cow::Borrowed(&exif));
    let mut original = Vec::new();
    let mut writer = png::Encoder::with_info(&mut original, info).unwrap().write_header().unwrap();
    writer.write_image_data(&[90; 4 * 4 * 3]).unwrap();
    writer.finish().unwrap();
    assert_eq!(header(&optimize(&original).unwrap()).exif, Some(exif));
}

#[test]
fn huge_sizes_refused_early() {
    let tiny = encode_image(&DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([1, 2, 3]))), None).unwrap();
    assert!(optimize(&claiming_to_be(1, tiny.clone())).is_ok());
    assert_eq!(optimize(&claiming_to_be(40_000, tiny)), Err(Error::TooLarge));
}
