use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use crate::find::os::{Listed, Raw};

use super::super::lanes::{Lanes, Next};
use super::super::worker::classify;
use super::super::{Job, Verdict};
use super::verdicts::{EVERYONES_MENU, OWN_MENU, drive, mac, windows};
use super::{DIR, deep};

const SYSTEM: u8 = 1;
const SHALLOW: usize = 2;

fn way(below: &str) -> Vec<PathBuf> {
    let steps: Vec<&str> = below.split('\\').collect();
    (0..=steps.len()).map(|taken| drive('C', &steps[..taken].join("\\"))).collect()
}

fn scanning(dir: PathBuf, ticket: u32) -> Job {
    Job { dir, ..deep("", ticket) }
}

#[test]
fn way_to_start_menu_keeps_tier() {
    let plan = windows();
    let everyone = way(EVERYONES_MENU);
    assert!(everyone.iter().all(|dir| plan.leads_to_apps(dir) && plan.ahead(dir)));
    let ranks: Vec<usize> = everyone.iter().map(|dir| plan.rank(dir)).collect();
    assert_eq!(ranks, [0; 6], "the drive and every folder below it lead to the apps, and apps come first");
    let tiers: Vec<u8> = everyone.iter().map(|dir| plan.inside(dir)).collect();
    assert_eq!(tiers, [0, 1, 1, 1, 1, 1], "what ProgramData holds is system data, as it was");
    let own = way(OWN_MENU);
    assert!(own.iter().all(|dir| plan.ahead(dir) && plan.rank(dir) == 0), "the user's own menu leads as well");
    assert_eq!(own.iter().map(|dir| plan.inside(dir)).collect::<Vec<u8>>(), [0, 0, 0, 1, 1, 1, 1, 1, 1]);
    let vendor = drive('C', &format!("{EVERYONES_MENU}\\TreeSize Free"));
    assert!(plan.ahead(&vendor) && !plan.leads_to_apps(&vendor), "a folder of shortcuts is listed with its menu");
    assert_eq!((plan.rank(&vendor), plan.inside(&vendor)), (0, SYSTEM));
    let beside = [
        ("ProgramData\\Adobe", 2),
        ("ProgramData\\Microsoft\\Windows\\Start Menu\\Places", 2),
        ("Program Files", 2),
        ("Users\\me\\AppData\\Local", 1),
    ];
    for (below, rank) in beside {
        let dir = drive('C', below);
        assert!(!plan.ahead(&dir) && !plan.leads_to_apps(&dir), "{below}");
        assert_eq!((plan.rank(&dir), plan.inside(&dir)), (rank, SYSTEM), "{below}");
    }
    assert!(!plan.ahead(&drive('D', "ProgramData")), "the start menu is on one drive");
}

#[test]
fn mac_apps_lead_bundles_wait() {
    let plan = mac(true);
    let ahead = |dir: &str| plan.ahead(Path::new(dir));
    for dir in ["/", "/System", "/System/Applications", "/Applications", "/Applications/Utilities", "/Users/me"] {
        assert!(ahead(dir) && plan.inside(Path::new(dir)) == 0, "{dir} is user content already");
    }
    for dir in ["/Applications/Xcode.app", "/Applications/Xcode.app/Contents", "/Library", "/Users/me/Music"] {
        assert!(!ahead(dir), "{dir}");
    }
    assert_eq!(plan.verdict(Path::new("/Applications"), 0, "Xcode.app", DIR, SHALLOW), Verdict::Descend(SYSTEM));
    assert_eq!(plan.rank(Path::new("/Applications/Xcode.app/Contents")), 0, "first among the system data");
    assert_eq!(plan.rank(Path::new("/Users/me/Music")), 1, "home follows the apps");
    assert!(plan.leads_to_apps(Path::new("/Users")) && !plan.leads_to_apps(Path::new("/Applications/Utilities")));
}

#[test]
fn start_menu_then_home_then_rest() {
    let (sink, _inbox) = mpsc::channel();
    let lanes = Lanes::new(sink);
    lanes.follow(Arc::new(windows()), 0);
    let vendor = format!("{EVERYONES_MENU}\\TreeSize Free");
    let submitted = [
        ('D', "Archive"),
        ('C', "Program Files"),
        ('C', "ProgramData"),
        ('C', "ProgramData\\Adobe"),
        ('C', vendor.as_str()),
        ('C', "Users\\me\\Music"),
    ];
    let jobs = submitted.iter().zip(0..).map(|((letter, below), ticket)| scanning(drive(*letter, below), ticket));
    let epoch = lanes.submit(jobs.collect()).expect("a pool to start");
    let taken = || match lanes.next(0, epoch, false) {
        Next::Job(taken) => Some((taken.job.dir.clone(), taken.job.tier)),
        Next::Idle | Next::Done => None,
    };
    let ran: Vec<(PathBuf, u8)> = std::iter::from_fn(taken).collect();
    let expected = [
        ('C', "ProgramData", SYSTEM),
        ('C', vendor.as_str(), SYSTEM),
        ('C', "Users\\me\\Music", 0),
        ('D', "Archive", 0),
        ('C', "Program Files", SYSTEM),
        ('C', "ProgramData\\Adobe", SYSTEM),
    ];
    assert_eq!(ran, expected.map(|(letter, below, tier)| (drive(letter, below), tier)), "each keeps its own tier");
}

#[test]
fn only_next_step_goes_ahead() {
    let plan = windows();
    let job = Job { tier: SYSTEM, ..scanning(drive('C', "ProgramData"), 4) };
    let folder = Raw { bits: DIR, ..Raw::default() };
    let listed =
        Listed { names: b"Microsoft\0Adobe\0notes.txt\0".to_vec(), rows: vec![folder, folder, Raw::default()] };
    let (listing, children) = classify(&job, &plan, 0, &listed, true);
    assert_eq!((listing.tier, listing.rows.len()), (SYSTEM, 3), "its rows are system data, as before");
    let queued: Vec<(PathBuf, u8, usize, bool)> =
        children.iter().map(|child| (child.job.dir.clone(), child.job.tier, child.rank, child.ahead)).collect();
    let (on_the_way, beside) = (drive('C', "ProgramData\\Microsoft"), drive('C', "ProgramData\\Adobe"));
    assert_eq!(queued, [(on_the_way, SYSTEM, 0, true), (beside, SYSTEM, 2, false)]);
}
