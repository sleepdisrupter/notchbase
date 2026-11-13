use std::fs;

use lopdf::Document;

use super::fixtures::{
    PdfFixture, TempDir, count_typed, half_transparent, image_source, media_box, noise, page_ids, page_image,
    page_rotation, page_text, pdf_source, run, save_image, save_jpeg, single_output,
};
use crate::shelf::Op;
use crate::shelf::names::file_name;

#[test]
fn combines_images_and_pdfs_in_order() {
    let dir = TempDir::new();
    let jpeg = save_jpeg(&noise(64, 48), 90, dir.path("one.jpg"));
    let png = save_image(&half_transparent(), dir.path("two.png"));
    let pdf = PdfFixture::pages(3).save(dir.path("three.pdf"));
    let outcome = run(Op::Combine, &[image_source(&jpeg), image_source(&png), pdf_source(&pdf)]);
    let output = single_output(&outcome);
    assert_eq!(file_name(output), "Combined.pdf");
    assert_eq!(outcome.notice.text, "Saved Combined.pdf · 5 pages");

    let doc = Document::load(output).unwrap();
    let pages = page_ids(&doc);
    assert_eq!(pages.len(), 5);

    let jpeg_image = page_image(&doc, pages[0]);
    assert_eq!(jpeg_image.dict.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
    assert_eq!(jpeg_image.content, fs::read(&jpeg).unwrap());
    assert_eq!(media_box(&doc, pages[0]), vec![0.0, 0.0, 48.0, 36.0]);

    let png_image = page_image(&doc, pages[1]);
    assert_eq!(png_image.dict.get(b"Filter").unwrap().as_name().unwrap(), b"FlateDecode");
    assert!(png_image.dict.get(b"SMask").is_ok());

    for (index, &page) in pages[2..].iter().enumerate() {
        assert_eq!(media_box(&doc, page), vec![0.0, 0.0, 612.0, 792.0]);
        let dict = doc.get_dictionary(page).unwrap();
        let resources = doc.dereference(dict.get(b"Resources").unwrap()).unwrap().1.as_dict().unwrap();
        assert!(resources.get(b"Font").is_ok());
        assert!(page_text(&doc, page).contains(&format!("(Page {})", index + 1)));
    }
    assert_eq!(count_typed(&doc, b"Catalog"), 1);
}

#[test]
fn keeps_inherited_rotation() {
    let dir = TempDir::new();
    let upright = PdfFixture::pages(2).save(dir.path("a.pdf"));
    let upside_down = PdfFixture::pages(3).rotated(180).save(dir.path("b.pdf"));
    let outcome = run(Op::Combine, &[pdf_source(&upside_down), pdf_source(&upright)]);
    let doc = Document::load(single_output(&outcome)).unwrap();
    let pages = page_ids(&doc);
    let rotations: Vec<Option<i64>> = pages.iter().map(|&page| page_rotation(&doc, page)).collect();
    assert_eq!(rotations, vec![Some(180), Some(180), Some(180), None, None]);
    assert!(page_text(&doc, pages[3]).contains("(Page 1)"));
}

#[test]
fn lone_image_names_the_pdf() {
    let dir = TempDir::new();
    let png = save_image(&half_transparent(), dir.path("scan.png"));
    let outcome = run(Op::Combine, &[image_source(&png)]);
    assert_eq!(file_name(single_output(&outcome)), "scan.pdf");
    assert_eq!(outcome.notice.text, "Saved scan.pdf · 1 page");
}
