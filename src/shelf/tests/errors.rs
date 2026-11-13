use std::fs;

use super::fixtures::{TempDir, half_transparent, image_source, noise, pdf_source, run, save_image, save_jpeg};
use crate::shelf::ops::Outcome;
use crate::shelf::{Format, Notice, Op};

fn assert_failed(outcome: Outcome, text: &str) {
    assert_eq!(outcome.notice, Notice::failed(text));
    assert!(outcome.outputs.is_empty());
}

fn gif_with_a_huge_canvas() -> Vec<u8> {
    let side = 60_000u16.to_le_bytes();
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[side[0], side[1], side[0], side[1], 0, 0, 0]);
    gif.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0x80, 0, 0, 0, 0xFF, 0xFF, 0xFF]);
    gif.extend_from_slice(&[2, 2, 0x44, 1, 0, 0x3B]);
    gif
}

#[test]
fn unreadable_files_become_notices() {
    let dir = TempDir::new();
    let bad_png = dir.path("bad.png");
    let bad_pdf = dir.path("bad.pdf");
    fs::write(&bad_png, b"\x89PNG\r\n\x1a\nnot really").unwrap();
    fs::write(&bad_pdf, b"%PDF-1.7\ngarbage").unwrap();
    let good = save_image(&half_transparent(), dir.path("good.png"));

    assert_failed(run(Op::Convert(Format::Jpeg), &[image_source(&bad_png)]), "Couldn't read bad.png");
    assert_failed(run(Op::Compress, &[image_source(&bad_png)]), "Couldn't read bad.png");
    assert_failed(run(Op::Compress, &[pdf_source(&bad_pdf)]), "Couldn't read bad.pdf");
    assert_failed(run(Op::Split, &[pdf_source(&bad_pdf)]), "Couldn't read bad.pdf");
    assert_failed(run(Op::Combine, &[image_source(&good), pdf_source(&bad_pdf)]), "Couldn't read bad.pdf");
    assert_failed(run(Op::Convert(Format::Png), &[image_source(&dir.path("gone.png"))]), "Couldn't read gone.png");
    assert_failed(
        run(Op::Convert(Format::Png), &[image_source(&bad_png), image_source(&bad_png)]),
        "Couldn't read bad.png (+1 more)",
    );
    assert!(!dir.path("bad pages").exists());
}

#[test]
fn partial_success_saves_what_it_can() {
    let dir = TempDir::new();
    let good = save_image(&half_transparent(), dir.path("good.png"));
    let bad = dir.path("bad.png");
    fs::write(&bad, b"nope").unwrap();
    let outcome = run(Op::Convert(Format::Jpeg), &[image_source(&good), image_source(&bad)]);
    assert_eq!(outcome.notice, Notice::ok("Saved good.jpg · 1 failed"));
    assert_eq!(outcome.outputs, vec![dir.path("good.jpg")]);
}

#[test]
fn truncated_images_never_panic() {
    let dir = TempDir::new();
    let full = fs::read(save_jpeg(&noise(64, 64), 90, dir.path("full.jpg"))).unwrap();
    let cut = dir.path("cut.jpg");
    fs::write(&cut, &full[..full.len() / 3]).unwrap();
    for op in [Op::Combine, Op::Compress, Op::Convert(Format::Png)] {
        let _outcome_either_way = run(op, &[image_source(&cut)]);
    }
}

#[test]
fn oversized_images_are_refused_by_name() {
    let dir = TempDir::new();
    let huge = dir.path("huge.gif");
    fs::write(&huge, gif_with_a_huge_canvas()).unwrap();
    assert_failed(run(Op::Convert(Format::Png), &[image_source(&huge)]), "huge.gif is too large");
}
