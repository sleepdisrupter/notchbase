use super::mark::START;
use super::*;
use crate::find::model::{flag, with_tier};
use crate::find::rank::named;
use crate::find::tests::corpus::{EPOCH, Noise, Planted, SPAN, grown, plant};
use crate::find::weights::SHORTLIST;

const NOW: u32 = EPOCH + SPAN;
const SYSTEM: u8 = 1;
const EACH: usize = 12;
const MADE_UP: usize = 600;
const SEED: u64 = 31;
const ROUNDING: f32 = 0.01;
const WORDS: [&str; 10] = ["report", "Main", "my", "File", "ab", "a", "b", "invoice", "Summary", "1"];
const JOINTS: [&str; 5] = ["", " ", "-", "_", "."];
const ENDINGS: [&str; 5] = ["", ".txt", ".exe", ".app", ".lnk"];
const ODD_QUESTIONS: [&str; 12] = [
    "ab ab",
    "my file",
    "a b",
    "a b ab",
    "file my",
    "main report my",
    "My File",
    "REPORT main",
    "my-file",
    "summary invoice",
    "apps my",
    "report main",
];
const MENU: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs";
const QUESTIONS: [&str; 18] = [
    "e",
    "re 1",
    "report main",
    "rs",
    "*.md",
    "today",
    "folders",
    "notes/",
    "re/ma",
    "Report",
    "1/",
    "re/1",
    "apps",
    "applications re",
    "reportmain",
    "invoice summary report",
    "main report",
    "start report",
];

fn corpus() -> Live {
    let mut live = grown(13, 8_000);
    let mut put = |path: &str, modified: u32, flags: u8| {
        plant(&mut live, &Planted { path, modified, created: EPOCH, flags });
    };
    for nth in 0..SHORTLIST + EACH {
        put(&format!("/work/summary-invoice-report {nth:03}/"), NOW, 0);
    }
    for nth in 0..EACH {
        put(&format!("/work/tools/x invoice-summary_report {nth}.exe"), EPOCH, 0);
        put(&format!("/work/re_port-main {nth}.txt"), EPOCH, 0);
        put(&format!("/work/reportmain-{nth}.rs"), EPOCH, 0);
        put(&format!("/work/main report {nth}.md"), EPOCH, 0);
        put(&format!("{MENU}/Report Main {nth}.lnk"), EPOCH, with_tier(0, SYSTEM));
        put(&format!("{MENU}/Reports/Main Report {nth}.url"), NOW, with_tier(0, SYSTEM));
        put(&format!("/Applications/Report Main {nth}.app/"), NOW, 0);
        put(&format!("/Applications/Invoice {nth}.app"), EPOCH, flag::LINK);
    }
    for led_by_no_word in ["/work/a-b ab", "/work/m-y file my.txt", "/work/re_port main report.exe"] {
        put(led_by_no_word, EPOCH, 0);
    }
    let mut noise = Noise::seeded(SEED);
    for nth in 0..MADE_UP {
        let mut name = String::new();
        for word in 0..2 + noise.below(3) {
            name.push_str(if word == 0 { "" } else { JOINTS[noise.below(JOINTS.len())] });
            name.push_str(WORDS[noise.below(WORDS.len())]);
        }
        let (ending, changed) = (ENDINGS[noise.below(ENDINGS.len())], EPOCH + noise.below(SPAN as usize) as u32);
        put(&format!("/made/up {nth}/{name}{ending}"), changed, with_tier(0, noise.below(3) as u8));
    }
    live
}

fn marked_as_the_names_read(ground: &Ground<'_>, id: EntryId) -> u8 {
    let name = ground.live.folded(id);
    let leads = ground.query.terms.iter().any(|term| !term.strict && name.starts_with(&term.needle));
    ground.owned(id) & !START | if leads { START } else { 0 }
}

fn weighing_every_entry(ground: &Ground<'_>) -> (Vec<Scored>, usize) {
    let live = ground.live;
    let mut judge = Judge::new(ground);
    let every = (0..live.len() as EntryId).filter_map(|id| {
        let flags = live.flags(id);
        judge.weigh(id, flags, collect::fit(ground, id, flags, &|id| ground.owned(id))?, None)
    });
    let mut slow: Vec<Scored> = every.collect();
    let matched = slow.len();
    slow.sort_by(|one, other| other.cmp(one));
    slow.truncate(SHORTLIST);
    let mut original = String::new();
    let is_one = |id: &EntryId| launcher(live, *id, live.folded(*id), live.flags(*id));
    let named =
        (0..live.len() as EntryId).filter(is_one).filter_map(|id| named(ground, (id, false), None, &mut original));
    (apps::replace(slow, apps::Named { best: named.collect(), veiled: Vec::new() }), matched)
}

#[test]
fn fast_path_keeps_the_same() {
    let live = corpus();
    let (context, latest) = (Context::at(NOW, 0, None), AtomicU64::new(0));
    let stop = Stop { latest: &latest, mine: 0 };
    for text in QUESTIONS {
        let query = parse(text, &context);
        let ground = Ground::new(&live, &query, &context, None, &stop).showing(true);
        let (mut slow, matched) = weighing_every_entry(&ground);
        assert!(matched >= fuzzy::FUZZY_BELOW, "{text} matches {matched} entries");
        slow.sort_by(|one, other| other.cmp(one));
        let kept = |found: &[Scored]| found.iter().map(|found| (found.id, found.score)).collect::<Vec<_>>();
        for most in [mark::allowance(), None] {
            let (mut fast, more) = search(&ground, &mut Scratch::default(), &[], most).unwrap();
            fast.sort_by(|one, other| other.cmp(one));
            assert_eq!((kept(&fast), more), (kept(&slow), false), "{text}, marking at most {most:?}");
        }
    }
}

#[test]
fn corpus_needs_squeezed_phrase() {
    let live = corpus();
    let (context, latest) = (Context::at(NOW, 0, None), AtomicU64::new(0));
    let stop = Stop { latest: &latest, mine: 0 };
    let found = |text: &str| {
        let query = parse(text, &context);
        let ground = Ground::new(&live, &query, &context, None, &stop).showing(true);
        search(&ground, &mut Scratch::default(), &[], None).unwrap().0
    };
    let apps = |text: &str| found(text).iter().filter(|found| found.fit.app).count();
    assert_eq!((apps("report main"), apps("apps"), apps("invoice")), (3 * EACH, 4 * EACH, EACH));
    assert_eq!(apps("start report"), 0, "a word of a folder above is not in the shown name");
    assert_eq!(found("start report").len(), 2 * EACH + 1, "the pipeline still finds them, and one folder");
    let made_up = live.resolve(std::path::Path::new("/made")).expect("planted last");
    let planted = |found: &&Scored| found.id < made_up;
    let squeezed = found("re_portmain");
    let tight = squeezed.iter().filter(planted).filter(|found| found.fit.squeezed).count();
    assert_eq!(tight, 4 * EACH + 1, "the files, the apps and the name that no word leads");
    assert!(squeezed.iter().any(|found| found.id > made_up && found.fit.squeezed), "and made-up names beside them");
    let hopeful = found("invoice summary report");
    assert_eq!(hopeful.len(), SHORTLIST, "a full shortlist");
    assert_eq!(hopeful.iter().filter(planted).filter(|found| live.flags(found.id) & flag::DIR == 0).count(), EACH);
}

#[test]
fn floor_never_drops_equal() {
    let live = corpus();
    let (context, latest) = (Context::at(NOW, 0, None), AtomicU64::new(0));
    let stop = Stop { latest: &latest, mine: 0 };
    let mut weighed = 0;
    for text in QUESTIONS.iter().chain(&ODD_QUESTIONS) {
        let query = parse(text, &context);
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let mut judge = Judge::new(&ground);
        for id in 0..live.len() as EntryId {
            let flags = live.flags(id);
            let Some(fit) = collect::fit(&ground, id, flags, &|id| marked_as_the_names_read(&ground, id)) else {
                continue;
            };
            let open = judge.weigh(id, flags, fit, None).expect("with no floor everything is weighed");
            let floor = Some(open.score - ROUNDING);
            let name = String::from_utf8_lossy(live.folded(id));
            assert!(!judge.hopeless(id, flags, &fit, floor), "{text}: {name} looked hopeless at {}", open.score);
            assert_eq!(judge.weigh(id, flags, fit, floor), Some(open), "{text}: {name}");
            weighed += 1;
        }
    }
    assert!(weighed > 30 * MADE_UP, "{weighed} entries were weighed");
}
