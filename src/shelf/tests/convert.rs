use std::fs;

use image::{DynamicImage, GenericImageView, ImageBuffer, LumaA};
use lopdf::Document;

use super::fixtures::{
    TempDir, embedded_icc, half_transparent, icc_profile, image_source, jpeg_bytes, media_box, noise, page_ids,
    page_image, page_rotation, run, save_image, save_png_with_icc, single_output, with_exif_orientation_6,
};
use crate::shelf::names::file_name;
use crate::shelf::{Format, Op, images};

#[test]
fn converts_to_every_format() {
    let dir = TempDir::new();
    let source = save_image(&half_transparent(), dir.path("photo.png"));
    for format in Format::ALL {
        let outcome = run(Op::Convert(format), &[image_source(&source)]);
        let output = single_output(&outcome);
        let expected = match format {
            Format::Png => "photo 2.png".to_owned(),
            _ => format!("photo.{}", format.extension()),
        };
        assert_eq!(file_name(output), expected);
        assert_eq!(outcome.notice.text, format!("Saved {expected}"));
        if format == Format::Pdf {
            let doc = Document::load(output).unwrap();
            assert_eq!(media_box(&doc, page_ids(&doc)[0]), vec![0.0, 0.0, 48.0, 36.0]);
            continue;
        }
        let decoded = image::open(output).unwrap_or_else(|err| panic!("{format:?}: {err}"));
        assert_eq!(decoded.dimensions(), (64, 48), "{format:?}");
    }
}

#[test]
fn jpeg_flattens_alpha_onto_white() {
    let dir = TempDir::new();
    let source = save_image(&half_transparent(), dir.path("clear.png"));
    let outcome = run(Op::Convert(Format::Jpeg), &[image_source(&source)]);
    let jpeg = image::open(single_output(&outcome)).unwrap().to_rgb8();
    let was_transparent = jpeg.get_pixel(8, 20);
    assert!(was_transparent.0.iter().all(|&channel| channel >= 240), "{was_transparent:?}");
    let was_opaque = jpeg.get_pixel(48, 20);
    let original = [192u8, 100, 200];
    assert!(was_opaque.0.iter().zip(original).all(|(&got, want)| got.abs_diff(want) <= 12), "{was_opaque:?}");
}

#[test]
fn converting_never_overwrites() {
    let dir = TempDir::new();
    let source = save_image(&half_transparent(), dir.path("pic.png"));
    fs::write(dir.path("pic.webp"), b"precious").unwrap();
    let original = fs::read(&source).unwrap();
    let first = run(Op::Convert(Format::Webp), &[image_source(&source)]);
    let second = run(Op::Convert(Format::Webp), &[image_source(&source)]);
    assert_eq!(file_name(single_output(&first)), "pic 2.webp");
    assert_eq!(file_name(single_output(&second)), "pic 3.webp");
    assert_eq!(fs::read(dir.path("pic.webp")).unwrap(), b"precious");
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn several_files_sum_up() {
    let dir = TempDir::new();
    let a = save_image(&half_transparent(), dir.path("a.png"));
    let b = save_image(&half_transparent(), dir.path("b.bmp"));
    let outcome = run(Op::Convert(Format::Gif), &[image_source(&a), image_source(&b)]);
    assert_eq!(outcome.notice.text, "Saved 2 files");
    assert_eq!(outcome.outputs, vec![dir.path("a.gif"), dir.path("b.gif")]);
}

#[test]
fn icons_fit_256() {
    let dir = TempDir::new();
    let source = save_image(&DynamicImage::ImageRgb8(noise(600, 300)), dir.path("wide.png"));
    let outcome = run(Op::Convert(Format::Ico), &[image_source(&source)]);
    let icon = image::open(single_output(&outcome)).unwrap();
    assert_eq!(icon.dimensions(), (256, 128));
}

#[test]
fn exif_orientation_is_honoured() {
    let dir = TempDir::new();
    let source = dir.path("turned.jpg");
    fs::write(&source, with_exif_orientation_6(&jpeg_bytes(&noise(40, 20), 90))).unwrap();
    assert_eq!(images::dimensions(&source), Some([20, 40]));

    let png = run(Op::Convert(Format::Png), &[image_source(&source)]);
    assert_eq!(image::open(single_output(&png)).unwrap().dimensions(), (20, 40));

    let pdf = run(Op::Convert(Format::Pdf), &[image_source(&source)]);
    let doc = Document::load(single_output(&pdf)).unwrap();
    let page = page_ids(&doc)[0];
    assert_eq!(page_rotation(&doc, page), Some(90));
    assert_eq!(media_box(&doc, page), vec![0.0, 0.0, 30.0, 15.0]);
    assert_eq!(page_image(&doc, page).dict.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
}

#[test]
fn profiles_kept_only_when_valid() {
    let dir = TempDir::new();
    let gray = DynamicImage::ImageLumaA8(ImageBuffer::from_fn(24, 16, |x, y| LumaA([(x * 10) as u8, (y * 16) as u8])));
    let profile = icc_profile(b"GRAY");
    let source = save_png_with_icc(&gray, profile.clone(), dir.path("gray.png"));
    let converted = |format| embedded_icc(single_output(&run(Op::Convert(format), &[image_source(&source)])));
    assert_eq!(converted(Format::Jpeg), Some(profile));
    assert_eq!(converted(Format::Webp), None);
    assert_eq!(converted(Format::Tiff), None);
}
