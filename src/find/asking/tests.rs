use std::path::Path;
use std::sync::mpsc;

use super::*;
use crate::find::apps::{Read, Sources};
use crate::find::context::Context;
use crate::find::rank::row;
use crate::find::tests::corpus::{Planted, from_paths, plant};
use crate::find::usage::Usage;

fn question(generation: u64, text: &str) -> Ask {
    Ask { generation, text: Arc::from(text), context: Context::default() }
}

fn asker() -> Asker {
    let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
    *shared.writing() = from_paths(&["/Users/john/Cargo.toml", "/Users/john/Car.png"]);
    shared.visible.store(true, Ordering::Release);
    Asker::new(shared, None)
}

fn posted(asker: &Asker) -> Option<(u64, Vec<String>)> {
    let answer = lock(&asker.shared.answer).take()?;
    Some((answer.generation, answer.rows.iter().map(|row| row.name.to_string()).collect()))
}

fn version(asker: &Asker) -> u64 {
    asker.shared.version.load(Ordering::Acquire)
}

#[test]
fn newest_of_burst_answered() {
    let (asks, inbox) = mpsc::channel();
    let asker = asker();
    let shared = asker.shared.clone();
    shared.latest.store(3, Ordering::Release);
    for (generation, text) in [(1, "c"), (2, "ca"), (3, "cargo")] {
        asks.send(Asked::Query(question(generation, text))).unwrap();
    }
    drop(asks);
    asker.run(inbox);
    let answer = lock(&shared.answer).take().expect("the last question is answered");
    assert_eq!((answer.generation, &*answer.query, answer.rows.len()), (3, "cargo", 1));
    assert_eq!(shared.version.load(Ordering::Acquire), 1, "one answer, one wake");
}

#[test]
fn answer_posted_and_app_woken() {
    let mut asker = asker();
    asker.shared.latest.store(1, Ordering::Release);
    asker.answer(&question(1, "cargo"));
    assert_eq!(posted(&asker), Some((1, vec!["Cargo.toml".to_owned()])));
    assert_eq!(version(&asker), 1);
    assert!(!asker.shared.searching.load(Ordering::Acquire));
}

#[test]
fn overtaken_question_posts_nothing() {
    let mut asker = asker();
    asker.shared.latest.store(5, Ordering::Release);
    asker.answer(&question(4, "car"));
    assert_eq!((posted(&asker), version(&asker)), (None, 0));
}

#[test]
fn empty_list_frozen_per_visit() {
    let (asks, inbox) = mpsc::channel();
    let mut asker = asker();
    asker.answer(&question(0, " "));
    assert_eq!((posted(&asker), asker.frozen.as_ref().map(Vec::len)), (Some((0, Vec::new())), Some(0)));
    let cargo = {
        let live = asker.shared.reading();
        let id = live.resolve(Path::new("/Users/john/Cargo.toml")).unwrap();
        FindRow { predicted: true, ..row(&live, id, Vec::new(), None, false, None) }
    };
    asker.frozen = Some(vec![cargo.clone()]);
    asker.answer(&question(0, ""));
    assert_eq!(posted(&asker), Some((0, vec!["Cargo.toml".to_owned()])), "the list of this visit, not a new one");
    asks.send(Asked::Shown).unwrap();
    let sifted = asker.sift(Asked::Refresh, &inbox);
    assert!(sifted.ask.is_none() && sifted.refresh && !sifted.apps && !sifted.stop);
    assert_eq!(asker.frozen, None);
    asker.answer(&question(0, ""));
    assert_eq!(posted(&asker), Some((0, Vec::new())));
    assert!(lock(&asker.shared.predicted).is_empty(), "what was predicted is what the fresh list predicted");
}

#[test]
fn wordless_question_is_empty_field() {
    let mut asker = asker();
    let cargo = {
        let live = asker.shared.reading();
        row(&live, live.resolve(Path::new("/Users/john/Cargo.toml")).unwrap(), Vec::new(), None, false, None)
    };
    asker.frozen = Some(vec![cargo]);
    for wordless in ["", "  ", "/", " // / ", "\u{301}"] {
        asker.answer(&question(0, wordless));
        let answer = lock(&asker.shared.answer).take().expect("it is answered");
        assert_eq!((&*answer.query, answer.rows.len()), (wordless, 1), "the list of this visit, under what was typed");
        assert!(asker.typed.is_none() && asker.shown.is_empty(), "{wordless:?} leaves nothing to run again");
    }
    asker.refresh();
    assert_eq!(posted(&asker), None);
    asker.answer(&question(0, "/car"));
    assert_eq!(posted(&asker).map(|(_, names)| names.len()), Some(2), "a word after the slash is a question");
    assert!(asker.typed.is_some());
}

#[test]
fn refresh_posts_only_changes() {
    let mut asker = asker();
    asker.refresh();
    asker.answer(&question(0, "car"));
    assert_eq!(posted(&asker).map(|(_, names)| names.len()), Some(2));
    asker.refresh();
    assert_eq!((posted(&asker), version(&asker)), (None, 1), "nothing changed, nobody is woken");
    let added = Planted { path: "/Users/john/car-keys.txt", modified: 1, created: 1, flags: 0 };
    plant(&mut asker.shared.writing(), &added);
    asker.shared.visible.store(false, Ordering::Release);
    asker.refresh();
    assert_eq!(posted(&asker), None, "a hidden page is not refreshed");
    asker.shared.visible.store(true, Ordering::Release);
    asker.refresh();
    assert_eq!(posted(&asker).map(|(generation, names)| (generation, names.len())), Some((0, 3)));
    asker.answer(&question(0, ""));
    assert_eq!(posted(&asker), Some((0, Vec::new())));
    plant(&mut asker.shared.writing(), &Planted { path: "/Users/john/car.txt", ..added });
    asker.refresh();
    assert_eq!(posted(&asker), None, "the empty list stays as it was");
}

#[test]
fn refreshes_half_a_second_apart() {
    let mut asker = asker();
    let now = Instant::now();
    assert_eq!(asker.refresh_due(now), now);
    asker.refreshed = Some(now);
    assert_eq!(asker.refresh_due(now + Duration::from_millis(100)), now + REFRESH_EVERY);
    assert_eq!(asker.refresh_due(now + Duration::from_secs(2)), now + Duration::from_secs(2));
}

#[test]
fn thread_ends_without_askers() {
    let (asks, inbox) = mpsc::channel();
    let asker = asker();
    let shared = asker.shared.clone();
    shared.latest.store(1, Ordering::Release);
    asks.send(Asked::Query(question(1, "png"))).unwrap();
    asks.send(Asked::Refresh).unwrap();
    drop(asks);
    asker.run(inbox);
    assert_eq!(lock(&shared.answer).as_ref().map(|answer| answer.rows.len()), Some(1));
}

#[test]
fn apps_met_again_each_visit() {
    let (asks, inbox) = mpsc::channel();
    let mut asker = asker();
    plant(&mut asker.shared.writing(), &Planted { path: "/Applications/Maps.app/", modified: 1, created: 1, flags: 0 });
    asker.answer(&question(0, "maps"));
    assert_eq!(asker.scratch.launchers.known().len(), 1);
    let answer = lock(&asker.shared.answer).take().expect("the app answers to its name");
    let shown: Vec<(&str, bool)> = answer.rows.iter().map(|row| (&*row.name, row.app)).collect();
    assert_eq!(shown, [("Maps", true)], "and is posted as an app, by the name it shows");
    asks.send(Asked::Shown).unwrap();
    asker.sift(Asked::Refresh, &inbox);
    assert!(asker.scratch.launchers.known().is_empty(), "nothing is trusted from the visit before");
    asker.answer(&question(0, ""));
    assert_eq!(asker.scratch.launchers.known().len(), 1, "so the first letter typed finds the apps listed");
}

#[test]
fn apps_handed_to_librarian() {
    let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
    *shared.writing() = from_paths(&["/System/Applications/FindMy.app/", "/Users/john/find-my-keys.txt"]);
    shared.visible.store(true, Ordering::Release);
    let (asks, inbox) = mpsc::channel();
    let sources = Sources {
        settle: Box::new(|| {}),
        read: Box::new(|app| Read {
            title: app.path.ends_with("FindMy.app").then(|| "Find My".to_owned()),
            ..Read::default()
        }),
        used: Box::new(|| None),
        every: Box::new(Vec::new),
        adopt: Box::new(|_| {}),
    };
    let librarian = Librarian::reading(shared.clone(), asks, sources);
    let mut asker = Asker::new(shared, None).cataloguing(librarian);
    asker.shared.latest.store(1, Ordering::Release);
    asker.answer(&question(1, "find my"));
    let rows = |app: &str| Some((1, vec![app.to_owned(), "find-my-keys.txt".to_owned()]));
    assert_eq!(posted(&asker), rows("FindMy"), "until the book is read, the file names the app");
    asker.catalogue();
    assert!(matches!(inbox.recv_timeout(Duration::from_secs(5)), Ok(Asked::Refresh)), "the book was shelved");
    asker.answer(&question(1, "find my"));
    assert_eq!(posted(&asker), rows("Find My"));
    asker.catalogue();
    assert!(inbox.recv_timeout(Duration::from_millis(50)).is_err(), "the same apps are not handed over twice");
}

#[test]
fn apps_met_before_first_open() {
    let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
    *shared.writing() = from_paths(&["/Applications/Maps.app/"]);
    let (asks, inbox) = mpsc::channel();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = asked.clone();
    let sources = Sources {
        settle: Box::new(|| {}),
        read: Box::new(|_| Read { title: Some("Maps".to_owned()), backstage: false, unlisted: false }),
        used: Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
            None
        }),
        every: Box::new(Vec::new),
        adopt: Box::new(|_| {}),
    };
    let librarian = Librarian::reading(shared.clone(), asks.clone(), sources);
    let mut asker = Asker::new(shared.clone(), None).cataloguing(librarian);
    asker.meet_the_apps();
    asker.catalogue();
    assert!(matches!(inbox.recv_timeout(Duration::from_secs(5)), Ok(Asked::Refresh)));
    assert_eq!(shared.book().len(), 1, "the closed Find already knows its apps");
    let opened = asker.sift(Asked::Shown, &inbox);
    assert!(opened.ask.is_none() && !opened.refresh && !opened.apps && !opened.stop);
    asker.catalogue();
    assert!(inbox.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(shared.book().len(), 1, "a visit whose apps are not gathered yet empties nothing");
    asker.meet_the_apps();
    asker.catalogue();
    assert!(inbox.recv_timeout(Duration::from_millis(50)).is_err(), "the same book is not shelved twice");
    assert_eq!(asked.load(Ordering::Relaxed), 1, "use is not asked for again within the minute");
}

#[test]
fn gone_service_stops_query_thread() {
    let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
    let (asks, inbox) = mpsc::channel();
    let sources = Sources {
        settle: Box::new(|| {}),
        read: Box::new(|_| Read::default()),
        used: Box::new(|| None),
        every: Box::new(Vec::new),
        adopt: Box::new(|_| {}),
    };
    let librarian = Librarian::reading(shared.clone(), asks.clone(), sources);
    let asker = Asker::new(shared, None).cataloguing(librarian);
    let running = std::thread::spawn(move || asker.run(inbox));
    asks.send(Asked::Stop).unwrap();
    running.join().expect("the query thread ends on the word, not on a line that never closes");
}
