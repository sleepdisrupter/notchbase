use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::find::scan::Fences;
use crate::find::tests::Scratch;

const ANSWER_WITHIN: Duration = Duration::from_secs(5);

type Asked = Arc<Mutex<Vec<PathBuf>>>;

fn recording() -> (Asked, Load) {
    let asked = Asked::default();
    let log = asked.clone();
    let load: Load = Box::new(move |path: &Path, px| {
        log.lock().unwrap().push(path.to_path_buf());
        assert!(path.exists(), "a probe exists while it is asked about");
        Some(ColorImage::filled([px as usize; 2], egui::Color32::WHITE))
    });
    (asked, load)
}

fn row(path: &Path, kind: Kind, folder: bool) -> FindRow {
    FindRow {
        key: 0,
        path: Arc::from(path),
        name: path.file_name().unwrap_or_default().to_string_lossy().into(),
        place: Arc::from("~"),
        marks: Vec::new(),
        kind,
        app: false,
        folder,
        cloud: false,
        modified: 0,
        why: None,
        predicted: false,
        icon: None,
    }
}

fn artist(scratch: &Scratch, home: &Path, load: Load) -> Artist {
    let fences = Fences { guarded: vec![home.join("Documents")], ..Fences::default() };
    let plan = Plan::fenced(&FindConfig::default(), &[], Some(home), None, false, fences);
    Artist { scratch: Some(scratch.file("icons")), home: Some(home.to_path_buf()), plan: Some(plan), load }
}

fn drawn(artist: &mut Artist, row: &FindRow, limited: bool) -> bool {
    artist.draw(&Look::of(row), limited).is_ok_and(|image| image.is_some())
}

#[test]
fn file_icon_from_type_probe() {
    let (scratch, home) = (Scratch::new("icons-type"), Scratch::new("icons-home"));
    let (asked, load) = recording();
    let mut artist = artist(&scratch, home.dir(), load);
    let report = row(&home.file("Documents/Report.PDF"), Kind::Pdf, false);
    let notes = row(&home.file("notes"), Kind::Other, false);
    assert!(drawn(&mut artist, &report, true) && drawn(&mut artist, &notes, false));
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked, [scratch.file("icons/probe.pdf"), scratch.file("icons/probe")]);
    assert!(asked.iter().all(|path| !path.exists()), "the probes are removed again");
}

#[test]
fn folders_share_one_icon() {
    let (scratch, home) = (Scratch::new("icons-folder"), Scratch::new("icons-home"));
    let (asked, load) = recording();
    let mut artist = artist(&scratch, home.dir(), load);
    let folder = row(&home.file("Documents/Taxes"), Kind::Folder, true);
    assert_eq!(Look::of(&folder), Look::of(&row(&home.file("Music"), Kind::Folder, true)));
    assert!(drawn(&mut artist, &folder, true));
    assert_eq!(*asked.lock().unwrap(), [scratch.file("icons")]);
}

#[test]
fn guarded_app_gets_no_icon() {
    let (scratch, home) = (Scratch::new("icons-app"), Scratch::new("icons-home"));
    let (asked, load) = recording();
    let mut artist = artist(&scratch, home.dir(), load);
    for app in ["Documents/Tool.app", "Pictures/Photos Library.photoslibrary/Edit.app", "Apps/Free.app"] {
        fs::create_dir_all(home.file(app)).unwrap();
    }
    let guarded = row(&home.file("Documents/Tool.app"), Kind::App, true);
    let packaged = row(&home.file("Pictures/Photos Library.photoslibrary/Edit.app"), Kind::App, true);
    let free = row(&home.file("Apps/Free.app"), Kind::App, true);
    assert!(artist.draw(&Look::of(&guarded), true).is_err());
    assert!(artist.draw(&Look::of(&packaged), true).is_err());
    assert!(drawn(&mut artist, &free, true));
    assert_eq!(*asked.lock().unwrap(), [home.file("Apps/Free.app")], "nothing guarded was asked");
    assert!(drawn(&mut artist, &guarded, false), "once access is full it is asked like any app");
}

#[test]
fn app_shortcut_gets_own_icon() {
    let (scratch, home) = (Scratch::new("icons-shortcut"), Scratch::new("icons-home"));
    let (asked, load) = recording();
    let mut artist = artist(&scratch, home.dir(), load);
    let shortcut = home.file("AppData/Roaming/Microsoft/Windows/Start Menu/Programs/TreeSize Free.lnk");
    fs::create_dir_all(shortcut.parent().unwrap()).unwrap();
    fs::write(&shortcut, []).unwrap();
    let app = FindRow { app: true, name: "TreeSize Free".into(), ..row(&shortcut, Kind::App, false) };
    let link = row(&home.file("Desktop/TreeSize Free.lnk"), Kind::Link, false);
    assert_eq!(Look::of(&app), Look::App(shortcut.clone()));
    assert_eq!(Look::of(&FindRow { kind: Kind::Link, ..app.clone() }), Look::App(shortcut.clone()), "by what it is");
    assert_eq!(Look::of(&link), Look::Type("lnk".into()));
    assert!(drawn(&mut artist, &app, false) && drawn(&mut artist, &link, false));
    assert_eq!(*asked.lock().unwrap(), [shortcut, scratch.file("icons/probe.lnk")]);
    let bundle = FindRow { app: true, name: "Safari".into(), ..row(&home.file("Apps/Safari.app"), Kind::App, true) };
    assert_eq!(Look::of(&bundle), Look::App(home.file("Apps/Safari.app")), "a bundle is not drawn as a folder");
}

#[test]
fn odd_endings_ask_plain_file() {
    assert_eq!(ending("Archive.TAR"), "tar");
    assert_eq!(ending(".zshrc"), "");
    assert_eq!(ending("weird.na/me"), "");
    assert_eq!(ending("long.abcdefghijklmnopq"), "");
    assert_eq!(ending("README"), "");
}

#[test]
fn draws_each_type_once() {
    let (scratch, home) = (Scratch::new("icons-thread"), Scratch::new("icons-home"));
    let (asked, load) = recording();
    let (woke, wakes) = mpsc::channel();
    let wake: Wake = Arc::new(move || {
        let _ = woke.send(());
    });
    let icons = Icons::run(wake, artist(&scratch, home.dir(), load));
    let rows = [
        row(&home.file("a.txt"), Kind::Text, false),
        row(&home.file("b.TXT"), Kind::Text, false),
        row(&home.file("Documents/Tool.app"), Kind::App, true),
    ];
    assert!(icons.of(&rows[0]).is_none());
    icons.want(&rows, true);
    wakes.recv_timeout(ANSWER_WITHIN).expect("a wake once the icons are drawn");
    assert_eq!(icons.version(), 1);
    assert!(icons.of(&rows[0]).is_some() && icons.of(&rows[1]).is_some());
    assert!(icons.of(&rows[2]).is_none());
    icons.want(&rows[..2], true);
    assert!(wakes.recv_timeout(Duration::from_millis(200)).is_err(), "kept icons are not drawn again");
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn icons_off_draw_nothing() {
    let icons = Icons::start(Arc::new(|| {}), false, None);
    let folder = row(Path::new("/tmp"), Kind::Folder, true);
    icons.want(std::slice::from_ref(&folder), false);
    assert_eq!((icons.of(&folder), icons.version()), (None, 0));
}

#[test]
fn probe_removed_after_panic() {
    let (scratch, home) = (Scratch::new("icons-panic"), Scratch::new("icons-panic-home"));
    let load: Load = Box::new(|path: &Path, _| panic!("the loader fell over on {}", path.display()));
    let mut artist = artist(&scratch, home.dir(), load);
    let report = row(&home.file("Report.pdf"), Kind::Pdf, false);
    let fell = catch_unwind(AssertUnwindSafe(|| artist.draw(&Look::of(&report), false)));
    assert!(fell.is_err());
    assert!(!scratch.file("icons/probe.pdf").exists(), "nothing is left in Find's folder");
}
