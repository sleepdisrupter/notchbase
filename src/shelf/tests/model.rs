use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::fixtures::{TempDir, no_wake};
use crate::shelf::{Clip, Format, Item, ItemId, Kind, Op, Origin, Shelf, available_ops, store};

#[test]
fn kinds_and_badges() {
    let dir = TempDir::new();
    let names = ["a.png", "b.JPG", "c.pdf", "d.txt", "e.heic", "noext", "f.png"];
    for name in &names[..6] {
        fs::write(dir.path(name), b"x").unwrap();
    }
    fs::create_dir(dir.path("f.png")).unwrap();
    let mut shelf = Shelf::with_locations(no_wake(), None, None);
    assert_eq!(shelf.add(names.iter().map(|name| dir.path(name)).collect()), 7);
    let got: Vec<(Kind, &str, Option<u64>)> = shelf.items().iter().map(|i| (i.kind, i.ext.as_str(), i.bytes)).collect();
    let heic = if cfg!(target_os = "macos") { Kind::Image } else { Kind::Other };
    assert_eq!(
        got,
        vec![
            (Kind::Image, "PNG", Some(1)),
            (Kind::Image, "JPG", Some(1)),
            (Kind::Pdf, "PDF", Some(1)),
            (Kind::Other, "TXT", Some(1)),
            (heic, "HEIC", Some(1)),
            (Kind::Other, "", Some(1)),
            (Kind::Folder, "", None),
        ]
    );
    assert_eq!(shelf.items()[1].name, "b.JPG");
}

#[test]
fn dedupes_by_path() {
    let dir = TempDir::new();
    fs::create_dir(dir.path("sub")).unwrap();
    let a = dir.path("a.png");
    fs::write(&a, b"x").unwrap();
    let mut shelf = Shelf::with_locations(no_wake(), None, None);
    assert_eq!(shelf.add(vec![a.clone(), a.clone()]), 1);
    assert_eq!(shelf.add(vec![a.clone()]), 0);
    assert_eq!(shelf.add(vec![dir.0.join("sub").join("..").join("a.png")]), 0);
    if cfg!(any(windows, target_os = "macos")) {
        assert_eq!(shelf.add(vec![dir.path("A.PNG")]), 0);
    }
    assert_eq!(shelf.add(vec![dir.path("missing.png")]), 0);
    assert_eq!(shelf.items().len(), 1);
}

#[test]
fn persistence_round_trips() {
    let dir = TempDir::new();
    let store_file = dir.path("data").join("shelf.json");
    let files: Vec<PathBuf> = ["b.png", "a.pdf", "c.txt"].iter().map(|name| dir.path(name)).collect();
    for file in &files {
        fs::write(file, b"x").unwrap();
    }
    fs::create_dir(dir.path("folder")).unwrap();
    {
        let mut shelf = Shelf::with_locations(no_wake(), Some(store_file.clone()), None);
        let mut all = files.clone();
        all.push(dir.path("folder"));
        assert_eq!(shelf.add(all), 4);
        let first = shelf.items()[0].id;
        shelf.remove(first);
    }
    let reloaded = Shelf::with_locations(no_wake(), Some(store_file.clone()), None);
    let paths: Vec<&Path> = reloaded.items().iter().map(|item| item.path.as_path()).collect();
    assert_eq!(paths, vec![files[1].as_path(), files[2].as_path(), dir.path("folder").as_path()]);
    assert_eq!(reloaded.items()[2].kind, Kind::Folder);
    drop(reloaded);

    fs::remove_file(&files[1]).unwrap();
    let mut shelf = Shelf::with_locations(no_wake(), Some(store_file.clone()), None);
    assert_eq!(shelf.items().len(), 2);
    assert_eq!(store::load(&store_file).len(), 2);
    shelf.clear();
    assert!(shelf.items().is_empty());
    assert!(store::load(&store_file).is_empty());

    fs::write(&store_file, b"{ not json").unwrap();
    assert!(Shelf::with_locations(no_wake(), Some(store_file), None).items().is_empty());
}

#[test]
fn prune_and_remove() {
    let dir = TempDir::new();
    let (a, b) = (dir.path("a.txt"), dir.path("b.txt"));
    fs::write(&a, b"x").unwrap();
    fs::write(&b, b"x").unwrap();
    let mut shelf = Shelf::with_locations(no_wake(), None, None);
    shelf.add(vec![a.clone(), b.clone()]);
    assert!(!shelf.prune_missing());
    fs::remove_file(&a).unwrap();
    assert!(shelf.prune_missing());
    assert_eq!(shelf.items().len(), 1);
    assert_eq!(shelf.items()[0].path, b);
    fs::write(&a, b"x").unwrap();
    assert_eq!(shelf.add(vec![a.clone()]), 1);
    let id = shelf.items()[1].id;
    shelf.remove(id);
    assert_eq!(shelf.items().len(), 1);
    assert_eq!(shelf.add(vec![a]), 1);
}

#[test]
fn shelf_new_in_tests_is_not_persisted() {
    let shelf = Shelf::new(no_wake());
    assert!(shelf.items().is_empty());
}

fn item(kind: Kind, ext: &str, pages: Option<u32>) -> Item {
    Item {
        id: ItemId(1),
        path: PathBuf::from(format!("x.{ext}")),
        name: format!("x.{ext}"),
        kind,
        ext: ext.to_uppercase(),
        bytes: None,
        pages,
        seconds: None,
        dims: None,
        thumb: None,
        thumb_rev: 0,
        added: Instant::now(),
        origin: Origin::Dropped,
    }
}

#[test]
fn ops_offered_for_a_selection() {
    let png = item(Kind::Image, "png", None);
    let gif = item(Kind::Image, "gif", None);
    let webp = item(Kind::Image, "webp", None);
    let pdf = item(Kind::Pdf, "pdf", Some(3));
    let one_page = item(Kind::Pdf, "pdf", Some(1));
    let folder = item(Kind::Folder, "", None);
    let convert = Op::Convert(Format::Png);
    assert_eq!(available_ops(&[]), vec![]);
    assert_eq!(available_ops(&[&png]), vec![convert, Op::Compress]);
    assert_eq!(available_ops(&[&gif]), vec![convert]);
    assert_eq!(available_ops(&[&webp]), vec![convert]);
    assert_eq!(available_ops(&[&png, &gif]), vec![convert, Op::Combine]);
    assert_eq!(available_ops(&[&pdf]), vec![Op::Compress, Op::Split]);
    assert_eq!(available_ops(&[&one_page]), vec![Op::Compress]);
    assert_eq!(available_ops(&[&png, &pdf]), vec![Op::Compress, Op::Combine]);
    assert_eq!(available_ops(&[&pdf, &one_page]), vec![Op::Compress, Op::Combine]);
    assert_eq!(available_ops(&[&png, &folder]), vec![]);
    let song = item(Kind::Audio, "mp3", None);
    let memo = item(Kind::Audio, "m4a", None);
    assert_eq!(available_ops(&[&song, &memo]), vec![Op::Encode(Clip::M4a)]);
    assert_eq!(available_ops(&[&song, &png]), vec![]);
}
