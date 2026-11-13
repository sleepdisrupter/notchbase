use std::fs;

use super::Outputs;
use super::compress::worth_keeping;
use super::tally::saving_percent;
use super::unique::write_new_file;
use crate::shelf::names::counted;
use crate::shelf::tests::fixtures::TempDir;

#[test]
fn names_count_up_and_never_overwrite() {
    let scratch = TempDir::new();
    let dir = &scratch.0;
    fs::write(dir.join("a.png"), b"original").unwrap();
    fs::create_dir(dir.join("a 2.png")).unwrap();
    let first = write_new_file(dir, "a", "png", b"one").unwrap();
    let second = write_new_file(dir, "a", "png", b"two").unwrap();
    assert_eq!(first, dir.join("a 3.png"));
    assert_eq!(second, dir.join("a 4.png"));
    assert_eq!(fs::read(dir.join("a.png")).unwrap(), b"original");
    assert_eq!(fs::read(&second).unwrap(), b"two");

    let outputs = Outputs::new(None);
    let near = dir.join("report.pdf");
    assert_eq!(outputs.create_dir(&near, "report pages").unwrap(), dir.join("report pages"));
    assert_eq!(outputs.create_dir(&near, "report pages").unwrap(), dir.join("report pages 2"));
}

#[test]
fn unusable_folder_falls_back() {
    let scratch = TempDir::new();
    let fallback = scratch.path("fallback");
    let near = scratch.path("gone").join("photo.png");
    let outputs = Outputs::new(Some(fallback.clone()));
    assert_eq!(outputs.write(&near, "photo", "jpg", b"x").unwrap(), fallback.join("photo.jpg"));
    assert_eq!(outputs.write(&near, "photo", "jpg", b"x").unwrap(), fallback.join("photo 2.jpg"));
    assert_eq!(Outputs::new(None).write(&near, "photo", "jpg", b"x"), Err("Couldn't save photo.jpg".to_owned()));
}

#[test]
fn savings_and_counts() {
    assert!(worth_keeping(100, 97));
    assert!(!worth_keeping(100, 98));
    assert!(!worth_keeping(0, 0));
    assert_eq!(saving_percent(1000, 380), 62);
    assert_eq!(saving_percent(1000, 1200), 0);
    assert_eq!(counted(1, "page"), "1 page");
    assert_eq!(counted(3, "page"), "3 pages");
}
