use std::fs;

use lopdf::Document;

use super::fixtures::{
    PdfFixture, TempDir, count_typed, media_box, page_ids, page_text, pdf_source, run, single_output,
};
use crate::shelf::Op;

#[test]
fn splits_into_one_pdf_per_page() {
    let dir = TempDir::new();
    let source = PdfFixture::pages(3).save(dir.path("report.pdf"));
    let outcome = run(Op::Split, &[pdf_source(&source)]);
    let folder = single_output(&outcome);
    assert_eq!(folder, dir.path("report pages"));
    assert_eq!(outcome.notice.text, "Split into 3 pages");

    let mut files: Vec<String> =
        fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, vec!["report 1.pdf", "report 2.pdf", "report 3.pdf"]);

    for number in 1..=3 {
        let doc = Document::load(folder.join(format!("report {number}.pdf"))).unwrap();
        let pages = page_ids(&doc);
        assert_eq!(pages.len(), 1);
        assert_eq!(media_box(&doc, pages[0]), vec![0.0, 0.0, 612.0, 792.0]);
        assert!(page_text(&doc, pages[0]).contains(&format!("(Page {number})")));
        assert_eq!(count_typed(&doc, b"Page"), 1);
    }

    let again = run(Op::Split, &[pdf_source(&source)]);
    assert_eq!(single_output(&again), dir.path("report pages 2"));
}
