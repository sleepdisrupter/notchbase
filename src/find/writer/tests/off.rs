use super::*;
use crate::find::context::Context;
use crate::find::store::mark;
use crate::find::view::{How, Pick};

const SECOND: Duration = Duration::from_secs(1);

fn off(config: FindConfig) -> FindConfig {
    FindConfig { enabled: false, ..config }
}

fn complete(rig: &Rig) -> bool {
    rig.writer.meta.roots.iter().all(|found| found.flags & mark::COMPLETE != 0)
}

fn deep_scans(rig: &Rig) -> usize {
    rig.scanner.take().iter().filter(|job| job.dir == rig.root() && job.deep).count()
}

fn with_secret(name: &str) -> Rig {
    let mut rig = Rig::new(name);
    rig.writer.launch();
    let jobs = rig.scanner.take();
    let root = rig.root();
    rig.listed(vec![
        listing(&root, &[folder("Secret"), file("notes.txt", 1)], jobs[0].ticket),
        listing(&rig.at("Secret"), &[file("diary.txt", 1)], jobs[0].ticket),
    ]);
    rig.finish(&jobs);
    rig.scanner.take();
    rig
}

#[test]
fn off_mid_scan_leaves_root_open() {
    let mut rig = Rig::new("off-mid-scan");
    rig.writer.launch();
    let early = rig.scanner.take();
    rig.writer.obey(Order::Follow(off(FindConfig::default())));
    let steering = lock(&rig.scanner.plans).last().cloned().unwrap();
    assert!(!steering.wanted(&rig.root()), "the scanner is told to drop what it still holds");
    rig.finish(&early);
    assert!(!complete(&rig));
    assert_eq!(rig.writer.wait(), Duration::MAX, "an idle writer while off");
    rig.writer.obey(Order::Follow(FindConfig::default()));
    let again = rig.scanner.take();
    assert_eq!(again.iter().filter(|job| job.dir == rig.root()).count(), 1, "the root is scanned again");
    rig.finish(&early);
    assert!(!complete(&rig), "a ticket from before the off lands nothing");
    rig.finish(&again);
    assert!(complete(&rig));
}

#[test]
fn off_drops_deferred_rescans() {
    let mut rig = with_secret("off-deferred");
    rig.events(vec![(Change::Overflow(rig.root()), 3)]);
    rig.settle();
    rig.events(vec![(Change::Overflow(rig.root()), 4)]);
    rig.settle();
    assert!(!rig.writer.deferred.is_empty());
    rig.writer.obey(Order::Follow(off(FindConfig::default())));
    rig.world.pass(Duration::from_secs(11 * 60));
    rig.writer.tick();
    assert!(rig.writer.deferred.is_empty() && rig.writer.wait() > SECOND);
}

#[test]
fn learning_off_forgets_at_once() {
    let mut rig = with_secret("off-learn");
    let pick = Pick { path: rig.at("notes.txt"), typed: "no".into(), rank: 0, how: How::Open, predicted: false };
    rig.writer.obey(Order::Picked { pick, context: Context::default() });
    assert!(!rig.writer.shared.usage.read(|model| model.is_empty()));
    rig.writer.obey(Order::Follow(off(FindConfig::default())));
    rig.writer.obey(Order::Follow(off(FindConfig { learn: false, ..FindConfig::default() })));
    assert!(rig.writer.shared.usage.read(|model| model.is_empty()));
}

#[test]
fn excluded_while_off_goes_at_once() {
    let mut rig = with_secret("off-exclude");
    rig.writer.obey(Order::Follow(off(FindConfig::default())));
    let excluded = FindConfig { excluded: vec!["Secret".into()], ..FindConfig::default() };
    rig.writer.obey(Order::Follow(off(excluded)));
    assert!(rig.find("Secret/diary.txt").is_none() && rig.find("notes.txt").is_some());
    assert!(rig.scanner.take().is_empty(), "nothing is scanned while off");
}

#[test]
fn excluded_while_quit_goes_at_launch() {
    let mut rig = with_secret("quit-exclude");
    rig.writer.save(rig.world.now());
    rig.writer.config.excluded = vec!["Secret".into()];
    let mut rig = rig.again();
    rig.writer.launch();
    assert!(rig.find("Secret/diary.txt").is_none() && rig.find("notes.txt").is_some());
}

#[test]
fn freed_folder_scanned_on_return() {
    let mut rig = with_secret("off-include");
    let excluded = FindConfig { excluded: vec!["Secret".into()], ..FindConfig::default() };
    rig.writer.obey(Order::Follow(excluded.clone()));
    rig.writer.obey(Order::Follow(off(excluded)));
    rig.scanner.take();
    rig.writer.obey(Order::Follow(off(FindConfig::default())));
    rig.writer.obey(Order::Follow(FindConfig::default()));
    assert_eq!(deep_scans(&rig), 1);
}

#[test]
fn relists_remembered_briefly() {
    let mut rig = with_secret("relisted");
    rig.events(vec![(Change::Relist(rig.root()), 1), (Change::Relist(rig.at("Secret")), 2)]);
    rig.settle();
    assert_eq!(rig.writer.relisted.len(), 2);
    rig.world.pass(Duration::from_secs(6));
    rig.events(vec![(Change::Relist(rig.root()), 3)]);
    rig.settle();
    assert_eq!(rig.writer.relisted.len(), 1);
}

#[test]
fn no_data_folder_no_ticking() {
    let mut rig = Rig::new("no-folder");
    rig.writer.dir = None;
    rig.writer.launch();
    let jobs = rig.scanner.take();
    rig.finish(&jobs);
    rig.writer.tick();
    assert!(!rig.writer.timers.save_soon && rig.writer.wait() > SECOND);
}
