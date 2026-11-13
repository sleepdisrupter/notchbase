use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn version(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

fn updater(status: Status) -> (Updater, Receiver<Order>) {
    let (orders, inbox) = mpsc::channel();
    (Updater { status: Arc::new(Mutex::new(status)), orders: Some(orders) }, inbox)
}

fn worker(manifest: &str) -> (Worker, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&wakes);
    let worker = Worker {
        server: Server::at(manifest, USER_AGENT),
        staging: std::env::temp_dir().join(format!("notchbase-update-tests-{}", std::process::id())),
        status: Arc::new(Mutex::new(Status::Waiting)),
        wake: Arc::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        }),
        found: None,
    };
    (worker, wakes)
}

#[test]
fn running_version_is_the_apps_own() {
    assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    assert!(current_version() > Version::default());
}

#[test]
fn update_requests_name_this_app() {
    assert_eq!(USER_AGENT, format!("notchbase/{}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn fetch_starts_only_when_available() {
    let (updater, inbox) = updater(Status::Available(version("9.0.0")));
    updater.fetch();
    assert_eq!(updater.status(), Status::Downloading(version("9.0.0"), 0.0));
    assert!(matches!(inbox.try_recv(), Ok(Order::Fetch)));
    updater.fetch();
    assert!(inbox.try_recv().is_err(), "a download already under way is not ordered twice");
}

#[test]
fn fetch_with_nothing_changes_nothing() {
    for status in [Status::Waiting, Status::Current, Status::Unavailable, Status::Failed("x".into())] {
        let (updater, inbox) = updater(status.clone());
        updater.fetch();
        assert_eq!(updater.status(), status);
        assert!(inbox.try_recv().is_err());
    }
}

#[test]
fn checking_soon_orders_a_check() {
    let (updater, inbox) = updater(Status::Current);
    updater.check_soon();
    assert!(matches!(inbox.try_recv(), Ok(Order::Check)));
}

#[test]
fn outside_failure_is_shown() {
    let (updater, _inbox) = updater(Status::Current);
    updater.failed("Couldn't install".into());
    assert_eq!(updater.status(), Status::Failed("Couldn't install".into()));
}

#[test]
fn workerless_updater_ignores_orders() {
    let updater = Updater { status: Arc::new(Mutex::new(Status::Unavailable)), orders: None };
    updater.check_soon();
    updater.fetch();
    assert_eq!(updater.status(), Status::Unavailable);
}

#[test]
fn wakes_only_when_status_changes() {
    let (worker, wakes) = worker("https://example.invalid/latest.json");
    worker.set(Status::Current);
    worker.set(Status::Current);
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    worker.set(Status::Available(version("9.0.0")));
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
    assert_eq!(worker.status(), Status::Available(version("9.0.0")));
}

#[test]
fn early_fetch_goes_back_to_waiting() {
    let (mut worker, _) = worker("https://example.invalid/latest.json");
    worker.set(Status::Current);
    assert!(worker.fetch());
    assert_eq!(worker.status(), Status::Waiting);
}

#[test]
fn unreachable_server_fails_and_retries() {
    let (mut worker, _) = worker("http://example.invalid/latest.json");
    assert!(!worker.check());
    assert_eq!(worker.status(), Status::Failed("Couldn't check for updates".into()));
    assert_eq!(worker.found, None);
}

#[test]
fn download_named_after_last_part() {
    assert_eq!(download_name("https://x/releases/0.16.2/notchbase-0.16.2.zip"), "notchbase-0.16.2.zip");
    assert_eq!(download_name("notchbase.exe"), "notchbase.exe");
}

#[test]
fn trailing_slash_gets_stand_in_name() {
    assert_eq!(download_name("https://x/releases/"), UNNAMED_DOWNLOAD);
    assert_eq!(download_name(""), UNNAMED_DOWNLOAD);
}
