use notchbase_sdk::protocol::{Node, Request};
use notchbase_sdk::testing::Harness;

use super::*;

fn movie(received: u64, total: Option<u64>) -> Download {
    Download { id: "movie.mp4.crdownload".into(), name: "movie.mp4".into(), received, total }
}

fn progress(download: Download) -> DownloadChange {
    DownloadChange::Progress { download }
}

fn finished(received: u64) -> DownloadChange {
    DownloadChange::Finished { download: movie(received, Some(received)), path: "/Downloads/movie.mp4".into() }
}

#[test]
fn known_size_fills_a_ring() {
    let mut harness = Harness::<Downloads>::start();
    assert!(harness.live().is_none());
    harness.download(progress(movie(25_000_000, Some(100_000_000))));
    let live = harness.live().expect("a live activity");
    assert!(matches!(live.leading, Node::Ring { value, tone: Tone::Blue, .. } if value == 0.25));
    assert!(harness.shows("movie.mp4") && harness.shows("25.0 MB"), "the ring is the share, the text the amount");
}

#[test]
fn unknown_size_counts_bytes() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(progress(movie(12_400_000, None)));
    let live = harness.live().expect("a live activity");
    assert_eq!(live.leading, Node::Icon { icon: Icon::Download, size: GLYPH, tone: Tone::Blue });
    assert!(harness.shows("12.4 MB"));
}

#[test]
fn several_show_first_and_count() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(progress(movie(1_000_000, None)));
    harness.download(progress(Download { id: "b.zip.part".into(), name: "b.zip".into(), received: 5, total: None }));
    assert!(harness.shows("movie.mp4") && harness.shows("1.0 MB +1"));
    harness.download(DownloadChange::Cancelled { download: movie(1_000_000, None) });
    assert!(harness.shows("b.zip") && !harness.shows("movie.mp4"));
}

#[test]
fn finished_lands_on_shelf() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(progress(movie(10, None)));
    harness.download(finished(48_000_000));
    assert!(matches!(&harness.requests()[0], Request::Shelve { paths } if paths == &["/Downloads/movie.mp4"]));
    assert!(matches!(&harness.requests()[1], Request::Notice { notice } if notice.banner));
    assert_eq!(harness.noticed(), ["movie.mp4", "Downloaded \u{b7} 48.0 MB"]);
    assert!(harness.live().is_none());
}

#[test]
fn shelf_and_notice_can_be_off() {
    let mut harness = Harness::<Downloads>::start_with([(SHELVE, false)]);
    assert!(matches!(harness.download(finished(5)), [Request::Notice { .. }]));
    harness.change_settings([(SHELVE, true), (ANNOUNCE, false)]);
    assert!(matches!(harness.download(finished(5)), [Request::Shelve { .. }]));
    harness.change_settings([(SHELVE, false)]);
    assert!(harness.download(finished(5)).is_empty());
}

#[test]
fn hidden_stays_out_until_done() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(progress(movie(10, None)));
    harness.click_icon(Icon::Xmark, 0);
    assert!(harness.live().is_none());
    harness.download(progress(movie(20, None)));
    assert!(harness.live().is_none());
    harness.download(finished(30));
    assert!(matches!(harness.requests(), [Request::Shelve { .. }, Request::Notice { .. }]));
    harness.download(progress(movie(5, None)));
    assert!(harness.live().is_some(), "downloading the same file again shows again");
}

#[test]
fn long_file_names_are_cut_short() {
    assert_eq!(shortened("clip.mp4"), "clip.mp4");
    assert_eq!(shortened("Screen Recording 2026-10-02 at 09.41.12.mov"), "Screen Recording 202\u{2026}");
}

#[test]
fn focus_keeps_it_quiet() {
    let mut harness = Harness::<Downloads>::start();
    harness.focus(Focus { active: true, name: None });
    assert!(matches!(harness.download(finished(5)), [Request::Shelve { .. }]));
}

fn landed_zip(path: &str) -> DownloadChange {
    let download = Download { id: "fonts.zip.crdownload".into(), name: "fonts.zip".into(), received: 5, total: None };
    DownloadChange::Finished { download, path: path.into() }
}

#[test]
fn finished_zip_offers_unzip() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(finished(5));
    assert!(harness.live().is_none(), "only zips are offered");
    harness.download(landed_zip("/Downloads/fonts.zip"));
    let live = harness.live().expect("the offer");
    assert!(live.controls.is_some());
    assert!(harness.shows("fonts.zip") && harness.shows("Unzip?"));
    harness.click_icon(Icon::Xmark, 0);
    assert!(harness.live().is_none(), "no thanks puts it away");
}

#[test]
fn offer_times_out() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(landed_zip("/Downloads/fonts.zip"));
    harness.send(Msg::Expire("/Downloads/other.zip".into()));
    assert!(harness.live().is_some(), "an older offer's timer leaves a newer one alone");
    harness.send(Msg::Expire("/Downloads/fonts.zip".into()));
    assert!(harness.live().is_none());
}

#[test]
fn offer_can_be_off() {
    let mut harness = Harness::<Downloads>::start_with([(OFFER_UNZIP, false)]);
    harness.download(landed_zip("/Downloads/fonts.zip"));
    assert!(harness.live().is_none());
}

#[test]
fn progress_comes_before_offer() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(landed_zip("/Downloads/fonts.zip"));
    harness.download(progress(movie(10, None)));
    assert!(harness.shows("movie.mp4") && !harness.shows("Unzip?"));
    harness.download(DownloadChange::Cancelled { download: movie(10, None) });
    assert!(harness.shows("Unzip?"));
}

#[test]
fn unreadable_zip_says_so() {
    let mut harness = Harness::<Downloads>::start();
    harness.download(landed_zip("/nowhere/fonts.zip"));
    harness.click("Unzip");
    assert!(harness.shows("Unzipping"));
    let requests = harness.wait_for(|downloads| downloads.offer.is_none()).to_vec();
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, Request::Notify { text } if text == "Couldn't unzip fonts.zip"))
    );
}

#[test]
#[cfg(target_os = "macos")]
fn unzip_shelves_the_folder() {
    let dir = std::env::temp_dir().join(format!("notchbase-offer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("source")).unwrap();
    std::fs::write(dir.join("source/a.ttf"), "a").unwrap();
    std::fs::write(dir.join("source/b.ttf"), "b").unwrap();
    let zipped = std::process::Command::new("zip")
        .args(["-qr", "../fonts.zip", "."])
        .current_dir(dir.join("source"))
        .status()
        .unwrap();
    assert!(zipped.success());
    let mut harness = Harness::<Downloads>::start();
    harness.download(landed_zip(dir.join("fonts.zip").to_str().unwrap()));
    harness.click("Unzip");
    let requests = harness.wait_for(|downloads| downloads.offer.is_none()).to_vec();
    let folder = dir.join("fonts").to_string_lossy().into_owned();
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, Request::Shelve { paths } if paths == std::slice::from_ref(&folder)))
    );
    assert!(dir.join("fonts/b.ttf").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
