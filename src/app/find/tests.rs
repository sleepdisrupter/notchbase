use super::*;
use crate::bridge::Kind;

fn row(path: &str) -> FindRow {
    let path: Arc<Path> = Arc::from(Path::new(path));
    let name: Arc<str> = Arc::from(path.file_name().unwrap().to_string_lossy().as_ref());
    let key =
        path.as_os_str().as_encoded_bytes().iter().fold(0, |key: u64, byte| key.wrapping_mul(31) ^ u64::from(*byte));
    FindRow {
        key,
        path,
        name,
        place: Arc::from("~"),
        marks: Vec::new(),
        kind: Kind::Document,
        app: false,
        folder: false,
        cloud: false,
        modified: 0,
        why: None,
        predicted: false,
        icon: None,
    }
}

fn running() -> Finding {
    Finding::resting(Some(Find::idle(Arc::new(|| {}))))
}

fn answer(query: &str, rows: Vec<FindRow>) -> Answer {
    Answer { generation: 1, query: Arc::from(query), rows, reading: None, more: false }
}

fn ready(entries: u32, access: Access) -> Status {
    Status { phase: Phase::Watching, entries, access, ..Status::default() }
}

#[test]
fn unstarted_service_is_off() {
    let mut finding = Finding::resting(None);
    assert!(!finding.synced());
    finding.shown(Some("Xcode".into()));
    finding.left();
    let view = finding.view(&Config::default());
    assert_eq!((view.state, view.answered, view.rows.len(), view.nudge), (FindState::Off, 0, 0, false));
    assert!(!finding.limited());
}

#[test]
fn sync_skips_same_version() {
    let mut finding = running();
    assert!(!finding.synced(), "nothing was published yet");
    let service = finding.service.as_ref().unwrap();
    service.post(Some(answer("car", vec![row("/Users/john/car.txt")])), ready(3, Access::Full));
    assert!(finding.synced());
    let first = finding.view(&Config::default());
    assert_eq!((first.state, first.answered, first.rows.len(), &*first.query), (FindState::Ready, 1, 1, "car"));
    assert!(!finding.synced(), "the same version is one compare and no work");
    assert_eq!(finding.view(&Config::default()), first);
}

#[test]
fn answers_raise_the_count() {
    let mut finding = running();
    for (round, query) in ["c", "c"].into_iter().enumerate() {
        let service = finding.service.as_ref().unwrap();
        service.post(Some(answer(query, vec![row("/a/c.txt")])), ready(3, Access::Full));
        finding.synced();
        assert_eq!(finding.view(&Config::default()).answered, round as u64 + 1, "a refresh counts too");
    }
}

#[test]
fn view_off_when_config_says() {
    let mut finding = running();
    finding.service.as_ref().unwrap().post(None, Status { phase: Phase::Off, ..Status::default() });
    finding.synced();
    let mut config = Config::default();
    assert_eq!(finding.view(&config).state, FindState::Starting, "switched on, the writer not caught up yet");
    config.find.enabled = false;
    finding.service.as_ref().unwrap().post(None, ready(3, Access::Full));
    finding.synced();
    assert_eq!(finding.view(&config).state, FindState::Off);
}

#[test]
fn access_row_until_dismissed() {
    let mut finding = running();
    finding.service.as_ref().unwrap().post(None, ready(3, Access::Limited));
    finding.synced();
    let mut config = Config::default();
    assert!(finding.view(&config).nudge && finding.limited());
    config.permissions.find_access_hidden = true;
    assert!(!finding.view(&config).nudge);
}

#[test]
fn missing_file_reported_once() {
    let mut finding = running();
    let rows = vec![row("/Users/john/gone.txt"), row("/Users/john/here.txt")];
    finding.service.as_ref().unwrap().post(Some(answer("e", rows)), ready(3, Access::Full));
    finding.synced();
    let gone = Path::new("/Users/john/gone.txt");
    let answered = finding.view(&Config::default()).answered;
    assert!(finding.missing(gone), "the first time it is said");
    assert!(!finding.missing(gone), "never twice");
    let view = finding.view(&Config::default());
    let names: Vec<String> = view.rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(names, ["here.txt"]);
    assert_eq!(view.answered, answered + 1, "the page takes the trimmed rows at once");
    assert_eq!(view.gone.len(), 1, "the page drops it even where it keeps rows");
    let again = vec![row("/Users/john/gone.txt"), row("/Users/john/here.txt")];
    finding.service.as_ref().unwrap().post(Some(answer("e", again)), ready(3, Access::Full));
    finding.synced();
    assert_eq!(finding.view(&Config::default()).rows.len(), 1, "a late answer never brings it back");
}

#[test]
fn guarded_pick_refused() {
    let finding = running();
    let documents = PathBuf::from("/Users/john/Documents");
    finding.service.as_ref().unwrap().fence(vec![documents.clone()]);
    assert!(!finding.reachable(&documents.join("taxes.pdf")) && !finding.reachable(&documents));
    assert!(finding.reachable(Path::new("/Users/john/notes.txt")));
    assert!(!Finding::resting(None).reachable(Path::new("/Users/john/notes.txt")), "no service, nothing to pick");
}

#[test]
fn unprobed_access_gates_icons() {
    let mut finding = running();
    assert_eq!(finding.status.access, Access::Limited);
    assert!(!finding.limited() && !finding.view(&Config::default()).nudge);
    finding.service.as_ref().unwrap().post(None, ready(3, Access::Full));
    finding.synced();
    assert!(!finding.limited());
}

#[test]
fn leaving_clears_the_rows() {
    let mut finding = running();
    finding.service.as_ref().unwrap().post(Some(answer("e", vec![row("/a/e.txt")])), ready(3, Access::Full));
    finding.synced();
    finding.left();
    assert!(finding.view(&Config::default()).rows.is_empty());
}
