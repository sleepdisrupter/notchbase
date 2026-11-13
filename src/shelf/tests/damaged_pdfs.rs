use std::fs;

use lopdf::{Document, Object, ObjectId};

use super::fixtures::{PdfFixture, TempDir, image_source, noise, page_ids, pdf_source, run, save_jpeg, single_output};
use crate::shelf::Op;

fn thumbnail_is_a_reference(doc: &Document, page: ObjectId) -> bool {
    matches!(doc.get_dictionary(page).unwrap().get(b"Thumb"), Ok(Object::Reference(_)))
}

#[test]
fn dangling_refs_stay_dangling() {
    let dir = TempDir::new();
    let broken = PdfFixture::pages(2).with_dangling_thumbnails().save(dir.path("broken.pdf"));
    let photo = save_jpeg(&noise(8, 8), 90, dir.path("photo.jpg"));

    let combined = run(Op::Combine, &[pdf_source(&broken), image_source(&photo)]);
    let doc = Document::load(single_output(&combined)).unwrap();
    let pages = page_ids(&doc);
    assert_eq!(pages.len(), 3);
    assert!(!thumbnail_is_a_reference(&doc, pages[0]));

    let split = run(Op::Split, &[pdf_source(&broken)]);
    let first = Document::load(single_output(&split).join("broken 1.pdf")).unwrap();
    assert!(!thumbnail_is_a_reference(&first, page_ids(&first)[0]));
}

#[test]
fn page_listed_twice_counts_once() {
    let dir = TempDir::new();
    let source = PdfFixture::pages(2).listing_the_first_page_twice().save(dir.path("twice.pdf"));

    let split = run(Op::Split, &[pdf_source(&source)]);
    assert_eq!(split.notice.text, "Split into 2 pages");
    assert_eq!(fs::read_dir(single_output(&split)).unwrap().count(), 2);

    let combined = run(Op::Combine, &[pdf_source(&source), pdf_source(&source)]);
    assert_eq!(combined.notice.text, "Saved Combined.pdf · 4 pages");
}
