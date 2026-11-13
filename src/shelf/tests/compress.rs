use std::fs;

use image::{DynamicImage, GenericImageView, ImageBuffer, LumaA, Rgb, Rgba, RgbaImage};
use lopdf::Document;

use super::fixtures::{
    PdfFixture, TempDir, half_transparent, image_source, noise, output_names, page_ids, page_text, pdf_source, run,
    save_image, save_jpeg, single_output, uncompressed_16_bit_gray_png,
};
use crate::shelf::names::file_name;
use crate::shelf::{Notice, Op, png_opt};

#[test]
fn png_compression_is_lossless() {
    let dir = TempDir::new();
    let source = uncompressed_16_bit_gray_png(dir.path("bloated.png"));
    let outcome = run(Op::Compress, &[image_source(&source)]);
    let output = single_output(&outcome);
    assert_eq!(file_name(output), "bloated compressed.png");
    let text = &outcome.notice.text;
    assert!(text.starts_with("bloated compressed.png · ") && text.ends_with("% smaller"), "{text}");
    let (before, after) = (fs::metadata(&source).unwrap().len(), fs::metadata(output).unwrap().len());
    assert!(after * 10 < before, "{before} → {after}");
    assert_eq!(image::open(&source).unwrap().to_rgba16(), image::open(output).unwrap().to_rgba16());

    let again = run(Op::Compress, &[image_source(output)]);
    assert_eq!(again.notice, Notice::ok("Already compact"));
    assert!(again.outputs.is_empty());
}

#[test]
fn png_optimizer_keeps_every_pixel() {
    let few_colors = DynamicImage::ImageRgba8(RgbaImage::from_fn(33, 17, |x, y| {
        let alpha = if (x + y) % 5 == 0 { 90 } else { 255 };
        Rgba([(x % 3 * 100) as u8, (y % 2 * 200) as u8, 50, alpha])
    }));
    let gray_alpha =
        DynamicImage::ImageLumaA8(ImageBuffer::from_fn(20, 9, |x, y| LumaA([(x * 12) as u8, (y * 28) as u8])));
    let colorful = DynamicImage::ImageRgb8(noise(31, 7));
    let deep =
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(9, 5, |x, y| Rgb([x as u16 * 7001, y as u16 * 13_001, 12_345])));
    for image in [few_colors, gray_alpha, colorful, deep] {
        let encoded = png_opt::encode_image(&image, None).unwrap();
        let decoded = image::load_from_memory(&encoded).unwrap();
        assert_eq!(decoded.to_rgba16(), image.to_rgba16(), "{:?}", image.color());
    }
}

#[test]
fn jpeg_and_bmp_compress_together() {
    let dir = TempDir::new();
    let jpeg = save_jpeg(&noise(96, 64), 100, dir.path("photo.jpg"));
    let bmp = save_image(&half_transparent(), dir.path("drawing.bmp"));
    let outcome = run(Op::Compress, &[image_source(&jpeg), image_source(&bmp)]);
    assert!(outcome.notice.ok);
    assert!(outcome.notice.text.starts_with("Compressed 2 files · "), "{}", outcome.notice.text);
    assert_eq!(output_names(&outcome), vec!["photo compressed.jpg", "drawing compressed.png"]);
    assert_eq!(image::open(&outcome.outputs[0]).unwrap().dimensions(), (96, 64));
    assert_eq!(image::open(&outcome.outputs[1]).unwrap().to_rgba8(), image::open(&bmp).unwrap().to_rgba8());
}

#[test]
fn pdf_compresses_and_keeps_its_pages() {
    let dir = TempDir::new();
    let source = PdfFixture::pages(3).padded(20_000).save(dir.path("report.pdf"));
    let outcome = run(Op::Compress, &[pdf_source(&source)]);
    let output = single_output(&outcome);
    assert_eq!(file_name(output), "report compressed.pdf");
    assert!(fs::metadata(output).unwrap().len() * 5 < fs::metadata(&source).unwrap().len());
    let doc = Document::load(output).unwrap();
    let pages = page_ids(&doc);
    assert_eq!(pages.len(), 3);
    assert!(page_text(&doc, pages[1]).contains("(Page 2)"));

    let again = run(Op::Compress, &[pdf_source(output)]);
    assert_eq!(again.notice, Notice::ok("Already compact"));
}
