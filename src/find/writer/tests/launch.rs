use super::*;
use crate::find::store::{mark, meta};
use crate::find::view::{Access, Phase};

fn second_root(rig: &Rig, volume: &Scratch, flags: u8) -> PathBuf {
    let path = volume.dir().to_path_buf();
    rig.world.state().roots.push(Root { path: path.clone(), volume: [3; 16], devices: [0, 0], flags });
    path
}

fn scans(rig: &Rig) -> Vec<String> {
    rig.log().into_iter().filter(|line| line.starts_with("scan")).collect()
}

fn complete(rig: &mut Rig, jobs: &[Job]) {
    rig.finish(jobs);
    rig.writer.tick();
}

#[test]
fn first_run_seeds_the_cursor() {
    let mut rig = Rig::new("first-run");
    rig.world.state().cursor = 777;
    rig.writer.launch();
    let log = rig.log();
    assert_eq!(log[0], format!("watch [{:?}] since Some(777)", rig.root().display().to_string()));
    assert_eq!(log[1], format!("scan {} deep true", rig.root().display()));
    assert_eq!(rig.cursor(), 777);
    assert!(rig.writer.shared.reading().resolve(&rig.root()).is_some(), "the root has its row before any listing");
    assert_eq!(lock(&rig.writer.shared.status).phase, Phase::Scanning);
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    assert_eq!(lock(&rig.writer.shared.status).phase, Phase::Watching);
    assert!(rig.store.file("index.bin").exists(), "a finished root is saved at once");
}

#[test]
fn unfinished_root_resumes() {
    let mut rig = Rig::new("mid-scan");
    let volume = Scratch::new("mid-scan-volume");
    let other = second_root(&rig, &volume, 0);
    rig.world.state().cursor = 500;
    rig.writer.launch();
    let jobs = rig.scanner.take();
    let home = jobs.iter().find(|job| job.dir == rig.root()).cloned().expect("the home root is scanned");
    assert_eq!(jobs[0].dir, rig.root(), "the root holding home goes first");
    complete(&mut rig, &[home]);
    rig.world.state().cursor = 900;
    let mut rig = rig.again();
    rig.writer.launch();
    assert_eq!(scans(&rig), [format!("scan {} deep true", other.display())]);
    assert!(rig.log()[0].ends_with("since Some(500)"), "replayed from the saved cursor, not from now");
}

#[test]
fn app_root_scans_first() {
    let mut rig = Rig::new("apps-first");
    let (plain, holding) = (Scratch::new("apps-first-plain"), Scratch::new("apps-first-holding"));
    let (plain, holding) = (second_root(&rig, &plain, 0), second_root(&rig, &holding, 0));
    rig.world.state().apps = vec![holding.join("Applications")];
    rig.writer.launch();
    let scanned: Vec<PathBuf> = rig.scanner.take().into_iter().map(|job| job.dir).collect();
    assert_eq!(scanned, [rig.root(), holding, plain], "home, then the applications, then the rest");
}

#[test]
fn app_places_relisted_at_launch() {
    let mut rig = Rig::new("apps-again");
    rig.world.state().apps = vec![rig.at("Applications")];
    rig.writer.launch();
    assert_eq!(scans(&rig), [format!("scan {} deep true", rig.root().display())], "a first scan finds them anyway");
    assert!(rig.asks.try_iter().any(|asked| matches!(asked, Asked::Apps)), "the loaded index may hold apps");
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    assert!(rig.asks.try_iter().any(|asked| matches!(asked, Asked::Apps)), "and so may a finished scan");
    let mut rig = rig.again();
    rig.writer.launch();
    assert_eq!(scans(&rig), [format!("scan {} deep false", rig.root().display())], "the nearest folder it knows");
    let jobs = rig.scanner.take();
    assert!(jobs.iter().all(|job| job.live), "ahead of any background work");
}

#[test]
fn cut_grant_resumes() {
    let mut rig = Rig::new("grant");
    let documents = rig.at("Documents");
    (rig.world.state().full, rig.world.state().guarded) = (false, vec![documents.clone()]);
    rig.writer.launch();
    assert_eq!(lock(&rig.writer.shared.status).access, Access::Limited);
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    rig.world.state().full = true;
    rig.writer.obey(Order::Access);
    assert_eq!(
        rig.scanner.take().iter().map(|job| job.dir.clone()).collect::<Vec<_>>(),
        std::slice::from_ref(&documents)
    );
    rig.writer.save(rig.world.now());
    let mut rig = rig.again();
    rig.writer.launch();
    let resumed = rig.scanner.take();
    assert_eq!(resumed.iter().map(|job| job.dir.clone()).collect::<Vec<_>>(), [documents], "the quit cut it short");
    complete(&mut rig, &resumed);
    assert_eq!(rig.writer.meta.flags & meta::GUARDED_DONE, meta::GUARDED_DONE);
    let mut rig = rig.again();
    rig.writer.launch();
    assert!(rig.scanner.take().is_empty(), "nothing left to resume");
}

#[test]
fn changed_volume_is_swept_again() {
    let mut rig = Rig::new("volumes");
    let volume = Scratch::new("volumes-other");
    let other = second_root(&rig, &volume, 0);
    rig.writer.launch();
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    rig.listed(vec![listing(&other, &[file("photo.jpg", 1)], 1)]);
    rig.writer.save(rig.world.now());
    {
        let mut state = rig.world.state();
        state.roots.retain(|found| found.path != other);
        state.roots[0].volume = [8; 16];
    }
    let mut rig = rig.again();
    rig.writer.launch();
    assert_eq!(scans(&rig), [format!("scan {} deep true", rig.root().display())]);
    assert!(rig.writer.shared.reading().resolve(&other.join("photo.jpg")).is_none());
}

#[test]
fn limited_skips_removable_volumes() {
    let mut rig = Rig::new("limited");
    let stick = Scratch::new("limited-stick");
    second_root(&rig, &stick, root::REMOVABLE);
    rig.world.state().full = false;
    rig.writer.launch();
    let root = rig.root().display().to_string();
    assert_eq!(rig.log()[0], format!("watch [{root:?}] since Some(0)"));
    assert_eq!(scans(&rig), [format!("scan {root} deep true")]);
}

#[test]
fn no_cursor_reconciles_gently() {
    let mut rig = Rig::new("no-cursor");
    rig.world.state().keeps = false;
    rig.writer.launch();
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    let mut rig = rig.again();
    rig.writer.launch();
    assert!(rig.log()[0].ends_with("since None"));
    rig.writer.tick();
    let reconcile = rig.scanner.take();
    assert_eq!(reconcile.iter().map(|job| (job.deep, job.reach, job.live)).collect::<Vec<_>>(), [(true, 1, false)]);
    assert_eq!(lock(&rig.scanner.paces).last(), Some(&Pace::Gentle));
    complete(&mut rig, &reconcile);
    assert_eq!(rig.writer.mark_of(&rig.root()).unwrap().flags & mark::COMPLETE, mark::COMPLETE);
}

#[test]
fn off_and_on_replays_missed() {
    let mut rig = Rig::new("off");
    rig.writer.launch();
    let jobs = rig.scanner.take();
    complete(&mut rig, &jobs);
    rig.events(vec![(Change::Touched(rig.at("a.txt")), 61)]);
    rig.settle();
    rig.finish(&rig.scanner.take());
    let off = FindConfig { enabled: false, ..FindConfig::default() };
    rig.writer.obey(Order::Follow(off));
    assert_eq!(rig.log().last().map(String::as_str), Some("unwatch"));
    assert_eq!(lock(&rig.writer.shared.status).phase, Phase::Off);
    rig.writer.obey(Order::Follow(FindConfig::default()));
    assert!(rig.log().iter().any(|line| line.ends_with("since Some(61)")));
    let deep = scans(&rig).into_iter().filter(|line| line.ends_with("deep true")).count();
    assert_eq!(deep, 1, "the complete root is not scanned again");
}
