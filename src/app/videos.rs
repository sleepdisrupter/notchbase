//! Video downloads: a browser tab dragged onto the notch is worked out to a page, then its download is followed.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::Pos2;

use crate::bridge::{Activity, VideoLive, VideoStage};
use crate::filedrag::{self, Dropped, Sensed};
use crate::platform::{self, Feel, KnownDir};
use crate::poll::Wake;
use crate::webvideo::browser::{self, Browser};
use crate::webvideo::{self, Download, Ending, Kind, Options, Page, Stage, Videos};

use super::App;
use super::preview::previewing;

const THREAD: &str = "notchbase-tab";
const TRAVEL: f32 = 40.0;
const RISE: f32 = 16.0;
const AT_ONCE: &[u64] = &[0];
const AFTER_THE_DROP: &[u64] = &[60, 250, 600];
const SETTING_UP: &str = "Getting the downloader, just this once";
const JOINING: &str = "Finishing";
const SAVED: &str = "In Downloads";
const MEGABYTE: f64 = 1_000_000.0;
const FILES: [&str; 28] = [
    "png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "bmp", "tiff", "svg", "ico", "pdf", "zip", "dmg", "pkg",
    "exe", "msi", "7z", "rar", "gz", "mp4", "mov", "mkv", "webm", "mp3", "m4a", "wav", "flac",
];

struct Answer {
    press: u64,
    page: Option<Page>,
    cookies: Option<String>,
}

type Answers = Arc<Mutex<Vec<Answer>>>;

pub(super) struct Grabbing {
    service: Videos,
    seen: u64,
    shown: Vec<VideoLive>,
    expiry: Option<Instant>,
    answers: Answers,
    wake: Wake,
    press: u64,
    held: bool,
    pressed_at: Option<Pos2>,
    browser: Option<Browser>,
    asked: bool,
    waiting: usize,
    dropped: bool,
    carrying: bool,
    over: bool,
    near: bool,
    arrived: bool,
    aim: Option<Kind>,
    picking: bool,
    tab: Option<(Page, Option<String>)>,
    payload: Option<Page>,
}

impl Grabbing {
    pub(super) fn new(wake: Wake) -> Self {
        Self {
            service: Videos::new(wake.clone()),
            seen: 0,
            shown: Vec::new(),
            expiry: None,
            answers: Answers::default(),
            wake,
            press: 0,
            held: false,
            pressed_at: None,
            browser: None,
            asked: false,
            waiting: 0,
            dropped: false,
            carrying: false,
            over: false,
            near: false,
            arrived: false,
            aim: None,
            picking: false,
            tab: None,
            payload: None,
        }
    }

    fn begin_press(&mut self, at: Option<Pos2>) {
        self.press += 1;
        self.pressed_at = at;
        self.browser = None;
        (self.asked, self.waiting, self.dropped, self.carrying) = (false, 0, false, false);
        (self.over, self.near, self.arrived, self.aim, self.picking) = (false, false, false, None, false);
        (self.tab, self.payload) = (None, None);
    }

    fn travelled(&self, now: Option<Pos2>) -> f32 {
        match (self.pressed_at, now) {
            (Some(from), Some(to)) => from.distance(to),
            _ => f32::INFINITY,
        }
    }

    fn risen(&self, now: Option<Pos2>) -> bool {
        match (self.pressed_at, now) {
            (Some(from), Some(to)) => from.y - to.y >= RISE,
            _ => true,
        }
    }

    fn offered(&self) -> Option<&Page> {
        self.payload.as_ref().or_else(|| self.tab.as_ref().filter(|_| !self.carrying).map(|(page, _)| page))
    }

    fn ask(&mut self, waits_ms: &'static [u64]) {
        let Some(browser) = self.browser.clone() else { return };
        let (press, answers, wake) = (self.press, self.answers.clone(), self.wake.clone());
        let look = move || {
            let mut page = None;
            for wait in waits_ms {
                std::thread::sleep(Duration::from_millis(*wait));
                let found = browser::addresses(&browser, &|address| webvideo::recognize_any(address).is_some());
                page = found.iter().find_map(|address| webvideo::recognize_any(address));
                if page.is_some() {
                    break;
                }
            }
            let answer = Answer { press, page, cookies: browser.cookies };
            answers.lock().unwrap_or_else(PoisonError::into_inner).push(answer);
            wake();
        };
        if std::thread::Builder::new().name(THREAD.into()).spawn(look).is_ok() {
            self.waiting += 1;
        }
    }
}

pub(super) fn wanted(link: &str) -> bool {
    crate::config::current().videos.enabled && webvideo::recognize(link).is_some()
}

fn page_in(link: &str) -> Option<Page> {
    webvideo::recognize(link).or_else(|| webvideo::recognize_any(link).filter(|_| !names_a_file(link)))
}

fn names_a_file(link: &str) -> bool {
    let path = link.split(['?', '#']).next().unwrap_or_default();
    let name = path.rsplit('/').next().unwrap_or_default();
    name.rsplit_once('.').is_some_and(|(_, extension)| FILES.contains(&extension.to_ascii_lowercase().as_str()))
}

impl App {
    pub(super) fn offer_videos(&mut self, sensed: Sensed, within_reach: bool, caught: bool) -> bool {
        let enabled = self.config.config().videos.enabled && !previewing();
        let held = sensed != Sensed::Idle;
        if !held {
            if std::mem::take(&mut self.videos.held) {
                self.video_released(enabled);
            }
            return false;
        }
        let at = crate::snap::cursor();
        if !self.videos.held {
            self.videos.begin_press(at);
            filedrag::catch_everything(false);
        }
        let ours = self.drops.busy();
        let carrying = matches!(sensed, Sensed::Files(_)) || self.drops.carrying();
        let grab = &mut self.videos;
        grab.held = true;
        grab.carrying |= carrying;
        let came = caught || grab.travelled(at) >= TRAVEL;
        grab.over = enabled && within_reach && came && !ours;
        if grab.over && !grab.asked && !grab.carrying {
            grab.asked = true;
            grab.browser = browser::frontmost();
            grab.ask(AT_ONCE);
        }
        let Some(nw) = self.notches.active_mut() else { return false };
        let cursor = nw.cursor();
        let in_zone = grab.over && cursor.is_some_and(|at| nw.notch.tab_zone().contains(at));
        grab.arrived |= in_zone && grab.risen(at);
        grab.near = in_zone && grab.arrived;
        let shown = if grab.payload.is_some() { grab.over } else { grab.near };
        let site = grab.offered().filter(|_| shown).map(|page| page.site);
        let from_tab = site.is_some() && grab.payload.is_none();
        filedrag::catch_everything(from_tab);
        let changed = nw.notch.set_drop_video(site);
        grab.picking = nw.notch.is_picking_layout();
        let aim = cursor.and_then(|at| nw.notch.takes_video_at(at));
        if site.is_some() {
            grab.aim = aim;
        }
        if nw.notch.aim_video(aim) | changed {
            nw.redraw();
        }
        if site.is_none() && changed {
            self.update_notch(|notch| {
                notch.set_drop_video(None);
            });
        }
        from_tab
    }

    fn video_released(&mut self, enabled: bool) {
        self.update_notch(|notch| {
            notch.set_drop_video(None);
            notch.aim_video(None);
        });
        let grab = &mut self.videos;
        let near = std::mem::take(&mut grab.near);
        grab.over = false;
        if !enabled || !near || grab.carrying || grab.payload.is_some() {
            return;
        }
        match (grab.tab.clone(), grab.aim) {
            (Some((page, cookies)), Some(kind)) => self.start_video(page, kind, cookies),
            (Some(_), None) => {}
            (None, _) if grab.asked && !grab.picking => {
                grab.dropped = true;
                grab.ask(AFTER_THE_DROP);
            }
            (None, _) => {}
        }
    }

    pub(super) fn video_offered(&mut self, link: &str) {
        if self.config.config().videos.enabled {
            self.videos.payload = page_in(link);
        }
    }

    pub(super) fn take_videos(&mut self, items: Vec<Dropped>) -> Vec<Dropped> {
        if !self.config.config().videos.enabled {
            return items;
        }
        let page_of = |item: &Dropped| match item {
            Dropped::Page(link) => page_in(link),
            Dropped::Link(link) => webvideo::recognize(link),
            Dropped::File(_) | Dropped::Data { .. } => None,
        };
        let (videos, rest): (Vec<Dropped>, Vec<Dropped>) = items.into_iter().partition(|item| page_of(item).is_some());
        let cookies = (!videos.is_empty()).then(browser::frontmost).flatten().and_then(|browser| browser.cookies);
        let kind = self.videos.aim.unwrap_or(Kind::Video);
        for page in videos.iter().filter_map(page_of) {
            self.start_video(page, kind, cookies.clone());
        }
        self.videos.payload = None;
        rest
    }

    fn start_video(&mut self, page: Page, kind: Kind, cookies: Option<String>) {
        let config = &self.config.config().videos;
        let options = Options {
            folder: platform::known_dir(KnownDir::Downloads),
            kind,
            tallest: config.tallest(),
            audio: config.audio_format().to_owned(),
            cookies: cookies.filter(|_| config.sign_in),
        };
        self.videos.service.start(page, options);
        platform::haptic(Feel::Snap);
        self.show_videos(Instant::now());
    }

    pub(super) fn cancel_video(&mut self, id: u64) {
        self.videos.service.cancel(id);
    }

    pub(super) fn sync_videos(&mut self, now: Instant) {
        self.take_tab_answers();
        for ending in self.videos.service.take_endings() {
            match ending {
                Ending::Saved { path, .. } => {
                    self.panel.shelf.add(vec![path]);
                    platform::haptic(Feel::Flip);
                }
                Ending::Failed(problem) => self.notify(&problem),
            }
        }
        let version = self.videos.service.version();
        let expired = self.videos.expiry.is_some_and(|at| now >= at);
        if version != self.videos.seen || expired {
            self.videos.seen = version;
            self.show_videos(now);
        }
    }

    fn show_videos(&mut self, now: Instant) {
        let shown: Vec<VideoLive> = self.videos.service.downloads(now).iter().map(live).collect();
        self.videos.expiry = self.videos.service.next_expiry();
        if shown != self.videos.shown {
            self.videos.shown = shown;
            self.show_activities();
        }
    }

    fn take_tab_answers(&mut self) {
        let answers = std::mem::take(&mut *self.videos.answers.lock().unwrap_or_else(PoisonError::into_inner));
        let press = self.videos.press;
        for answer in answers.into_iter().filter(|answer| answer.press == press) {
            let grab = &mut self.videos;
            grab.waiting = grab.waiting.saturating_sub(1);
            if let Some(page) = answer.page {
                grab.tab = Some((page, answer.cookies));
            }
            let found = grab.tab.clone().filter(|_| grab.dropped);
            if found.is_some() || grab.waiting == 0 {
                grab.dropped = false;
            }
            if let Some((page, cookies)) = found {
                self.start_video(page, Kind::Video, cookies);
            }
        }
    }

    pub(super) fn video_activities(&self) -> impl Iterator<Item = Activity> + '_ {
        self.videos.shown.iter().cloned().map(Activity::Video)
    }

    pub(super) fn videos_wakeup(&self) -> Option<Instant> {
        self.videos.expiry
    }
}

fn live(download: &Download) -> VideoLive {
    let (stage, detail) = match &download.stage {
        Stage::Preparing => (VideoStage::Preparing, SETTING_UP.to_owned()),
        Stage::Finding => (VideoStage::Finding, download.page.site.name().to_owned()),
        Stage::Downloading => (VideoStage::Downloading, pace(download)),
        Stage::Joining => (VideoStage::Joining, JOINING.to_owned()),
        Stage::Saved(_) => (VideoStage::Saved, SAVED.to_owned()),
        Stage::Failed(problem) => (VideoStage::Failed, problem.clone()),
    };
    VideoLive {
        id: download.id,
        site: download.page.site,
        audio: download.kind == Kind::Audio,
        stage,
        share: download.share,
        title: download.title.clone().unwrap_or_default(),
        detail,
        cover: download.cover.clone(),
    }
}

fn pace(download: &Download) -> String {
    let speed = download.speed.map(|speed| match speed / MEGABYTE {
        fast if fast >= 10.0 => format!("{fast:.0} MB/s"),
        megabytes if megabytes >= 0.1 => format!("{megabytes:.1} MB/s"),
        _ => format!("{:.0} KB/s", speed / 1000.0),
    });
    let left = download.eta.filter(|eta| *eta >= 1.0).map(|eta| {
        let seconds = eta.round() as u64;
        format!("{}:{:02} left", seconds / 60, seconds % 60)
    });
    [speed, left].into_iter().flatten().collect::<Vec<_>>().join(" \u{b7} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grabbing() -> Grabbing {
        Grabbing::new(Arc::new(|| {}))
    }

    fn page(address: &str) -> Page {
        webvideo::recognize(address).expect("a video page")
    }

    fn download(stage: Stage) -> Download {
        Download {
            id: 3,
            page: page("https://youtu.be/jNQXAC9IVRw"),
            kind: Kind::Video,
            title: Some("Me at the zoo".into()),
            stage,
            share: 0.42,
            received: 311_000,
            speed: Some(3_140_000.0),
            eta: Some(72.4),
            since_ms: 0,
            cover: None,
        }
    }

    #[test]
    fn drag_beats_tab() {
        let mut grab = grabbing();
        let (tab, link) = (page("https://youtu.be/jNQXAC9IVRw"), page("https://www.instagram.com/reel/DAbc/"));
        grab.tab = Some((tab.clone(), None));
        assert_eq!(grab.offered(), Some(&tab));
        grab.payload = Some(link.clone());
        assert_eq!(grab.offered(), Some(&link), "a dragged link is what was meant");
        grab.payload = None;
        grab.carrying = true;
        assert_eq!(grab.offered(), None, "a picture dragged off a video page is just a picture");
    }

    #[test]
    fn press_starts_fresh() {
        let mut grab = grabbing();
        grab.tab = Some((page("https://youtu.be/jNQXAC9IVRw"), Some("firefox".into())));
        (grab.asked, grab.dropped, grab.carrying, grab.aim) = (true, true, true, Some(Kind::Audio));
        let before = grab.press;
        grab.begin_press(Some(Pos2::new(10.0, 10.0)));
        assert_eq!(grab.press, before + 1, "answers to the last drag no longer match");
        assert!(grab.tab.is_none() && !grab.asked && !grab.dropped && !grab.carrying && grab.aim.is_none());
    }

    #[test]
    fn drag_arrives_after_travel() {
        let mut grab = grabbing();
        grab.begin_press(Some(Pos2::new(700.0, 60.0)));
        assert!(grab.travelled(Some(Pos2::new(712.0, 52.0))) < TRAVEL, "a click on a tab under the notch");
        assert!(grab.travelled(Some(Pos2::new(735.0, 20.0))) >= TRAVEL);
        grab.begin_press(None);
        assert!(grab.travelled(Some(Pos2::ZERO)) >= TRAVEL, "an unknown start counts as far away");
    }

    #[test]
    fn sideways_reorders_lifted_comes() {
        let mut grab = grabbing();
        grab.begin_press(Some(Pos2::new(500.0, 58.0)));
        assert!(!grab.risen(Some(Pos2::new(760.0, 55.0))), "along the tab strip, under the notch");
        assert!(!grab.risen(Some(Pos2::new(760.0, 70.0))), "or sagging below it");
        assert!(grab.risen(Some(Pos2::new(740.0, 40.0))), "lifted towards the notch");
        grab.begin_press(Some(Pos2::new(700.0, 400.0)));
        assert!(grab.risen(Some(Pos2::new(700.0, 100.0))), "a tab from lower down has risen long before it arrives");
        grab.begin_press(None);
        assert!(grab.risen(Some(Pos2::ZERO)));
        assert!(!grab.arrived, "and every press has to arrive again");
    }

    #[test]
    fn address_is_page_unless_file() {
        assert_eq!(page_in("https://vimeo.com/76979871").map(|page| page.site), Some(webvideo::Site::Vimeo));
        assert_eq!(page_in("https://example.com/watch/some-clip").map(|page| page.site), Some(webvideo::Site::Other));
        assert_eq!(page_in("https://example.com/post.html").map(|page| page.site), Some(webvideo::Site::Other));
        for file in
            ["https://example.com/cat.PNG", "https://example.com/a/report.pdf?dl=1", "https://cdn.example.com/clip.mp4"]
        {
            assert_eq!(page_in(file), None, "{file} is saved as the file it is");
        }
        assert_eq!(page_in("https://www.youtube.com/feed/subscriptions"), None);
        let audio = live(&Download { kind: Kind::Audio, ..download(Stage::Downloading) });
        assert!(audio.audio && !live(&download(Stage::Downloading)).audio);
    }

    #[test]
    fn no_browser_asks_nobody() {
        let mut grab = grabbing();
        grab.ask(AT_ONCE);
        assert_eq!(grab.waiting, 0);
    }

    #[test]
    fn download_shows_stage_and_pace() {
        let moving = live(&download(Stage::Downloading));
        assert_eq!(
            (moving.stage, moving.share, moving.title.as_str()),
            (VideoStage::Downloading, 0.42, "Me at the zoo")
        );
        assert_eq!(moving.detail, "3.1 MB/s \u{b7} 1:12 left");
        assert_eq!(live(&download(Stage::Saved("/d/zoo.mp4".into()))).detail, SAVED);
        let failed = live(&download(Stage::Failed("Can't reach YouTube".into())));
        assert_eq!((failed.stage, failed.detail.as_str()), (VideoStage::Failed, "Can't reach YouTube"));
        assert_eq!(live(&download(Stage::Finding)).detail, "YouTube");
    }

    #[test]
    fn pace_reads_at_any_speed() {
        let at = |speed: Option<f64>, eta: Option<f64>| pace(&Download { speed, eta, ..download(Stage::Downloading) });
        assert_eq!(at(Some(42_600_000.0), Some(3.0)), "43 MB/s \u{b7} 0:03 left");
        assert_eq!(at(Some(48_000.0), None), "48 KB/s");
        assert_eq!(at(None, Some(0.2)), "");
        assert_eq!(at(None, Some(600.0)), "10:00 left");
    }
}
