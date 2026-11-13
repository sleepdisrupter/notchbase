use super::*;
use crate::find::store::meta;
use crate::find::view::Access;

fn with_documents(name: &str) -> (Rig, PathBuf) {
    let mut rig = Rig::new(name);
    let documents = rig.at("Documents");
    rig.world.state().guarded = vec![documents.clone()];
    rig.writer.launch();
    let jobs = rig.scanner.take();
    let root = rig.root();
    rig.listed(vec![
        listing(&root, &[folder("Documents"), file("notes.txt", 1)], jobs[0].ticket),
        listing(&documents, &[file("taxes.pdf", 1)], jobs[0].ticket),
    ]);
    rig.finish(&jobs);
    rig.scanner.take();
    (rig, documents)
}

fn access(rig: &Rig) -> Access {
    lock(&rig.writer.shared.status).access
}

fn dirs(rig: &Rig) -> Vec<PathBuf> {
    rig.scanner.take().into_iter().map(|job| job.dir).collect()
}

#[test]
fn limited_launch_buries_guarded() {
    let (mut rig, documents) = with_documents("guarded-launch");
    rig.writer.save(rig.world.now());
    rig.world.state().full = false;
    let rig = {
        let mut rig = rig.again();
        rig.writer.launch();
        rig
    };
    assert_eq!(access(&rig), Access::Limited);
    assert!(rig.find("Documents/taxes.pdf").is_none(), "gone from results, suggestions and the newest list");
    assert!(rig.find("Documents").is_some() && rig.find("notes.txt").is_some(), "the rest stays");
    assert!(!rig.writer.shared.reachable(&documents.join("taxes.pdf")) && rig.writer.shared.reachable(&rig.root()));
}

#[test]
fn lost_access_noticed_first() {
    let (mut rig, documents) = with_documents("guarded-lost");
    rig.world.state().full = false;
    rig.events(vec![(Change::Touched(documents.join("taxes.pdf")), 7)]);
    assert_eq!(access(&rig), Access::Limited);
    assert!(rig.find("Documents/taxes.pdf").is_none());
    rig.settle();
    assert!(dirs(&rig).is_empty(), "nothing in the guarded folder is looked at");
}

#[test]
fn change_dropped_after_lost_access() {
    let (mut rig, documents) = with_documents("guarded-held");
    rig.events(vec![(Change::Touched(documents.join("new.pdf")), 7)]);
    rig.world.state().full = false;
    rig.settle();
    assert_eq!(access(&rig), Access::Limited, "the writer asks again before it acts");
    assert!(dirs(&rig).is_empty());
}

#[test]
fn change_dropped_after_exclusion() {
    let (mut rig, _) = with_documents("excluded-held");
    rig.listed(vec![listing(&rig.root(), &[folder("Documents"), folder("Secret")], 1)]);
    rig.events(vec![(Change::Touched(rig.at("Secret/diary.txt")), 7)]);
    rig.writer.obey(Order::Follow(FindConfig { excluded: vec!["Secret".into()], ..FindConfig::default() }));
    rig.scanner.take();
    rig.settle();
    assert!(dirs(&rig).is_empty());
}

#[test]
fn guarded_reports_ignored() {
    let (mut rig, documents) = with_documents("guarded-gone");
    rig.world.state().full = false;
    rig.writer.obey(Order::Access);
    rig.writer.obey(Order::Gone(documents.join("taxes.pdf")));
    assert!(dirs(&rig).is_empty());
    rig.listed(vec![listing(&documents, &[file("late.pdf", 1)], 1)]);
    assert!(rig.find("Documents/late.pdf").is_none());
}

#[test]
fn cut_guarded_pass_not_done() {
    let mut rig = Rig::new("guarded-cut");
    rig.world.state().guarded = vec![rig.at("Documents")];
    rig.world.state().full = false;
    rig.writer.launch();
    let scan = rig.scanner.take();
    rig.finish(&scan);
    rig.world.state().full = true;
    rig.writer.obey(Order::Access);
    let pass = rig.scanner.take();
    assert_eq!(pass.len(), 1);
    rig.world.state().full = false;
    rig.writer.obey(Order::Access);
    rig.finish(&pass);
    assert_eq!(rig.writer.meta.flags & meta::GUARDED_DONE, 0, "the next grant goes through the folders again");
}

#[test]
fn limited_until_first_probe() {
    let rig = Rig::new("unprobed");
    let documents = rig.at("Documents");
    rig.world.state().guarded = vec![documents.clone()];
    let rig = rig.again();
    assert_eq!(access(&rig), Access::Limited);
    assert!(!rig.writer.shared.reachable(&documents.join("taxes.pdf")), "unknown counts as limited");
}
