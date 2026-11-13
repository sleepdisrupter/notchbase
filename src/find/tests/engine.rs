use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Power;

use super::Scratch;
use crate::config::FindConfig;
use crate::find::asking::Asker;
use crate::find::context::Context;
use crate::find::key::key_of;
use crate::find::os::{Raw, Root, raw, root};
use crate::find::scan::Listing;
use crate::find::shared::{Ask, Asked, Order, lock};
use crate::find::view::{How, Phase, Pick, Status};
use crate::find::writer::tests::{Recording, State, World};
use crate::find::writer::{Inputs, Writer};
use crate::find::{Find, Setup};
use crate::poll::Wake;

const HOME: &str = "/Users/john";
pub(super) const NOW: u32 = 1_760_000_000;
const PATIENCE: Duration = Duration::from_secs(10);

pub(super) fn quiet() -> Wake {
    Arc::new(|| {})
}

fn setup(home: &Path) -> Setup {
    let home = Some(home.to_path_buf());
    Setup { dir: None, home, config: FindConfig::default(), ram_bytes: 16 << 30, writes: false }
}

fn pick(path: &Path, typed: &str) -> Pick {
    Pick { path: path.to_path_buf(), typed: typed.into(), rank: 0, how: How::Open, predicted: false }
}

pub(super) fn listing(dir: &Path, names: &[(&str, u8)], ticket: u32) -> Listing {
    Listing {
        dir: dir.to_path_buf(),
        tier: 0,
        inside_package: false,
        names: names.iter().flat_map(|(name, _)| name.bytes().chain([0])).collect(),
        rows: names.iter().map(|(_, bits)| Raw { modified: 20, created: 10, file_id: 0, bits: *bits }).collect(),
        complete: true,
        deep: true,
        reach: 3,
        ticket,
    }
}

fn world(disk: &Path) -> World {
    let home = Root { path: disk.to_path_buf(), volume: [7; 16], devices: [0, 0], flags: root::BOOT };
    World(Arc::new(Mutex::new(State {
        at: Instant::now(),
        stamp: NOW,
        power: Power { level: Some(80), plugged: true, charging: false },
        roots: vec![home],
        full: true,
        cursor: 0,
        keeps: true,
        watches: true,
        guarded: Vec::new(),
        apps: Vec::new(),
        log: Vec::new(),
    })))
}

pub(super) struct Bench {
    pub(super) find: Find,
    pub(super) writer: Writer,
    inbox: Receiver<Order>,
    pub(super) scanner: Arc<Recording>,
    world: World,
    pub(super) disk: Scratch,
}

pub(super) fn bench(name: &str, wake: Wake, writes: bool) -> Bench {
    let disk = Scratch::new(name);
    let (find, assembled, inbox, _asker, _questions) = Find::assemble(setup(disk.dir()), wake);
    drop(assembled);
    let scanner = Arc::new(Recording::default());
    let world = world(disk.dir());
    let inputs = Inputs {
        scanner: scanner.clone(),
        sink: mpsc::channel().0,
        asks: None,
        dir: None,
        home: Some(disk.dir().to_path_buf()),
        config: FindConfig::default(),
        ram_bytes: 16 << 30,
        writes,
    };
    let writer = Writer::on(find.shared.clone(), inputs, Box::new(world.clone()));
    Bench { find, writer, inbox, scanner, world, disk }
}

fn waits_for(what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < until, "waited too long for {what}");
        std::thread::yield_now();
    }
}

#[test]
fn unstarted_service_is_off() {
    let find = Find::idle(quiet());
    find.shown(Context::default());
    assert_eq!(find.ask("car"), 1);
    find.picked(&pick(Path::new("/a"), "a"), Context::default());
    find.gone(Path::new("/a"));
    find.follow(&FindConfig::default());
    find.forget();
    find.recheck_access();
    find.left();
    assert_eq!((find.version(), find.take(), find.status()), (0, None, Status::default()));
}

#[test]
fn questions_get_newer_numbers() {
    let (find, _writer, _orders, _asker, questions) = Find::assemble(setup(Path::new(HOME)), quiet());
    let at_work = Context::at(1_704_110_400, 3600, Some(Arc::from("Xcode")));
    find.shown(at_work.clone());
    assert_eq!((find.ask("c"), find.ask("ca")), (1, 2));
    let asked: Vec<Asked> = questions.try_iter().collect();
    assert!(matches!(&asked[0], Asked::Shown));
    assert!(matches!(&asked[2], Asked::Query(ask) if ask.generation == 2 && &*ask.text == "ca"));
    assert!(matches!(&asked[2], Asked::Query(ask) if ask.context == at_work));
    assert_eq!(find.status().phase, Phase::Loading);
}

#[test]
fn leaving_reports_picks_once() {
    let (find, _writer, orders, _asker, _questions) = Find::assemble(setup(Path::new(HOME)), quiet());
    find.shown(Context::default());
    let chosen = pick(Path::new("/Users/john/a.txt"), "a");
    find.picked(&chosen, Context::default());
    find.left();
    find.left();
    let sent: Vec<Order> = orders.try_iter().collect();
    assert!(matches!(&sent[0], Order::Picked { pick, .. } if *pick == chosen));
    assert!(matches!(&sent[1], Order::Left { picked: Some(key), .. } if *key == key_of(&chosen.path)));
    assert!(matches!(&sent[2], Order::Left { picked: None, .. }));
}

#[test]
fn landed_listing_is_found() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    let wake: Wake = Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    let mut bench = bench("lands", wake, false);
    let mut asker = Asker::new(bench.find.shared.clone(), Some(bench.disk.dir().to_path_buf()));
    bench.writer.launch();
    let ticket = bench.scanner.take().first().expect("the launch scans the root").ticket;
    let home = bench.disk.dir().to_path_buf();
    let names = [("Cargo.toml", 0), ("Documents", raw::DIR), ("car-keys.txt", 0)];
    bench.writer.obey(Order::Listed(vec![listing(&home, &names, ticket)]));
    bench.writer.obey(Order::Finished(ticket));
    assert_eq!((bench.find.status().entries, bench.find.status().phase), (4, Phase::Watching));

    let before = (bench.find.version(), wakes.load(Ordering::Relaxed));
    bench.find.shown(Context::default());
    let generation = bench.find.ask("car");
    let ask = Ask { generation, text: Arc::from("car"), context: Context::default() };
    asker.answer(&ask);
    let answer = bench.find.take().expect("the question was answered");
    let rows: Vec<(&str, &str, bool)> = answer.rows.iter().map(|row| (&*row.name, &*row.place, row.folder)).collect();
    assert_eq!((answer.generation, &*answer.query), (generation, "car"));
    assert_eq!(rows, [("Cargo.toml", "~", false), ("car-keys.txt", "~", false)]);
    assert_eq!(answer.rows[0].key, key_of(&home.join("Cargo.toml")).0);
    assert_eq!((bench.find.version(), wakes.load(Ordering::Relaxed)), (before.0 + 1, before.1 + 1));
    assert!(bench.find.take().is_none());
    assert!(bench.inbox.try_recv().is_err(), "asking sends the writer nothing");
}

#[test]
fn threads_live_with_service() {
    let bench = bench("threads", quiet(), true);
    let Bench { find, writer, inbox, scanner, world, disk } = bench;
    let shared = find.shared.clone();
    let asker = Asker::new(shared.clone(), Some(disk.dir().to_path_buf()));
    let (asks, questions) = mpsc::channel();
    let listed = Order::Listed(vec![listing(disk.dir(), &[("zebra.txt", 0)], 1)]);
    lock(&find.orders).take().expect("a started service takes orders").send(listed).unwrap();
    drop(find);
    std::thread::spawn(move || writer.run(inbox)).join().expect("the writer ends with its last sender");
    assert!(!scanner.take().is_empty(), "the writer launched over the fake world before it listened");
    assert!(world.state().log.iter().any(|line| line.starts_with("watch")));
    let ask = Ask { generation: 0, text: Arc::from("zebra"), context: Context::default() };
    asks.send(Asked::Query(ask)).unwrap();
    drop(asks);
    std::thread::spawn(move || asker.run(questions)).join().expect("the asker ends with its last sender");
    let answer = lock(&shared.answer).take().expect("the question that was waiting is answered first");
    assert_eq!(answer.rows.iter().map(|row| &*row.name).collect::<Vec<_>>(), ["zebra.txt"]);
}

#[test]
fn pick_reaches_model() {
    let mut bench = bench("pick", quiet(), false);
    bench.writer.launch();
    let file = bench.disk.file("report.pdf");
    std::fs::write(&file, b"q").unwrap();
    bench.find.picked(&pick(&file, "rep"), Context::at(NOW, 0, None));
    bench.inbox.try_iter().for_each(|order| bench.writer.obey(order));
    let remembered = bench.find.shared.usage.read(|model| model.remembered(b"rep", key_of(&file), NOW));
    assert!(remembered > 0.0, "what was typed leads to what was picked");
    assert!(bench.find.shared.usage.read(|model| model.file(key_of(&file)).is_some()));
}

#[test]
fn off_stops_watcher() {
    let bench = bench("off", quiet(), true);
    let Bench { find, writer, inbox, world, .. } = bench;
    let worker = std::thread::spawn(move || writer.run(inbox));
    let watching = || world.state().log.iter().any(|line| line.starts_with("watch"));
    waits_for("the watcher", watching);
    find.follow(&FindConfig { enabled: false, ..FindConfig::default() });
    waits_for("the watcher to stop", || world.state().log.last().is_some_and(|line| line == "unwatch"));
    waits_for("the off state", || find.status().phase == Phase::Off);
    drop(find);
    worker.join().expect("the writer ends with its last sender");
}

#[test]
fn stopped_writer_restarts() {
    let published = Arc::new(AtomicUsize::new(0));
    let counter = published.clone();
    let wake: Wake = Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    let (find, writer, inbox, _asker, _questions) = Find::assemble(setup(Path::new(HOME)), wake);
    drop((writer, inbox));
    find.follow(&FindConfig { enabled: false, ..FindConfig::default() });
    waits_for("a new writer to say Find is off", || published.load(Ordering::Relaxed) > 0);
    assert_eq!(find.status().phase, Phase::Off);
    assert!(lock(&find.orders).as_ref().is_some_and(|orders| orders.send(Order::Forget).is_ok()));
}

#[test]
fn outside_listing_dropped() {
    let mut bench = bench("outside", quiet(), false);
    bench.writer.launch();
    bench.writer.obey(Order::Listed(vec![listing(&PathBuf::from(HOME), &[("notes.txt", 0)], 1)]));
    assert_eq!(bench.find.status().entries, 1, "only the root row");
}
