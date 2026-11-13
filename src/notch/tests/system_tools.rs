use std::time::Instant;

use notchbase_sdk::protocol::{
    Download, DownloadChange, Event, LiveNode, NoticeNode, Power, Request, Sensors, Start, Surface, UiValue,
};
use notchbase_sdk::{Driver, Runtime, Tool, Waker};

use crate::bridge::{Activity, ToolSlot};
use crate::notch::Notch;
use crate::notch::activity::ActivityKind;
use crate::notch::geometry::CANVAS;
use crate::platform::Style;

use super::fixtures::{AWAY, Rig, settle, shortcuts};

const SLOT: ToolSlot = ToolSlot(7);
const FRAMES: usize = 40;
const ROOM: f32 = 60.0;
const WIDEST_WING: f32 = 64.0;
const MACBOOKS: [Option<(f32, f32)>; 3] = [None, Some((185.0, 32.0)), Some((200.0, 38.0))];

fn started<T: Tool>() -> Runtime<T> {
    let mut runtime = Runtime::<T>::new(Waker::noop());
    runtime.start(Start::default());
    runtime
}

fn notices<T: Tool>(runtime: &mut Runtime<T>, events: Vec<Event>) -> Vec<NoticeNode> {
    let requests = runtime.handle(events).requests;
    requests
        .into_iter()
        .filter_map(|request| match request {
            Request::Notice { notice } => Some(notice),
            _ => None,
        })
        .collect()
}

fn power(level: u8, plugged: bool) -> Event {
    Event::Power { power: Power { level: Some(level), plugged, charging: plugged } }
}

fn downloading(received: u64, total: Option<u64>) -> Download {
    let name = "Screen Recording 2026-10-02 at 09.41.12.mov";
    Download { id: format!("{name}.crdownload"), name: name.into(), received, total }
}

fn system_notices() -> Vec<NoticeNode> {
    let mut battery = started::<battery::Battery>();
    let mut shown = notices(&mut battery, vec![power(64, false), power(64, true), power(100, true)]);
    shown.extend(notices(&mut battery, vec![power(21, false), power(20, false), power(9, false)]));
    let mut downloads = started::<::downloads::Downloads>();
    let landed = DownloadChange::Finished { download: downloading(9, Some(9)), path: "clip.mov".into() };
    shown.extend(notices(&mut downloads, vec![Event::Download { change: landed }]));
    assert_eq!(shown.len(), 6, "charging, full, on battery, two warnings and a landing");
    shown
}

fn system_lives() -> Vec<LiveNode> {
    let mut lives = Vec::new();
    let mut sensors = started::<camera_mic::CameraMic>();
    let call = Sensors { camera: true, microphone: true, apps: vec!["Microsoft Teams".into()], muted: true };
    sensors.handle(vec![Event::Sensors { sensors: call }]);
    lives.extend(sensors.views().live);
    let mut downloads = started::<::downloads::Downloads>();
    for total in [Some(100), None] {
        let change = DownloadChange::Progress { download: downloading(25, total) };
        downloads.handle(vec![Event::Download { change }]);
        lives.extend(downloads.views().live);
    }
    let mut awake = started::<keep_awake::KeepAwake>();
    let press = awake.views().tile.and_then(|tile| tile.on_press).expect("a tile to press");
    awake.handle(vec![Event::Ui { surface: Surface::Tile, handler: press, value: UiValue::Press }]);
    lives.extend(awake.views().live);
    assert_eq!(lives.len(), 4, "a call, two downloads and the cup");
    lives
}

fn notch(size: Option<(f32, f32)>) -> Notch {
    match size {
        Some((width, height)) => Notch::with_preview(Style::Notch, Some(egui::vec2(width, height)), None),
        None => Notch::with_preview(Style::Island, None, None),
    }
}

#[test]
fn system_notices_fit_canvas() {
    let (rig, keys) = (Rig::new(), shortcuts(None));
    for size in MACBOOKS {
        for notice in system_notices() {
            let mut n = notch(size);
            n.show_notice(SLOT, notice.clone());
            for _ in 0..FRAMES {
                rig.frame(&mut n, &keys, Vec::new());
            }
            settle(&mut n);
            let Some(ActivityKind::Tool(shown)) = &n.activity.kind else { panic!("a tool notice") };
            assert!(shown.widths.is_some_and(|[leading, trailing]| leading > 0.0 && trailing > 0.0), "{notice:?}");
            let body = n.silhouette().rect();
            assert!(body.width() <= CANVAS.x - ROOM, "{size:?}: {} wide for {notice:?}", body.width());
            if let Some((width, height)) = size {
                assert!(body.width() > width && body.height() >= height, "it grows past the camera housing");
            }
        }
    }
}

#[test]
fn system_live_activities_fit() {
    let (rig, keys) = (Rig::new(), shortcuts(None));
    for size in MACBOOKS {
        for live in system_lives() {
            let mut n = notch(size);
            n.set_activities(vec![Activity::Tool(SLOT, live.clone())]);
            n.tick(Instant::now(), Some(AWAY));
            for _ in 0..FRAMES {
                rig.frame(&mut n, &keys, Vec::new());
            }
            settle(&mut n);
            assert!(n.shows_live(), "{live:?}");
            let body = n.silhouette().rect();
            assert!(body.width() <= CANVAS.x - ROOM, "{size:?}: {} wide for {live:?}", body.width());
            if let Some((width, _)) = size {
                assert!(
                    body.width() <= width + 2.0 * WIDEST_WING,
                    "{} wide beside the camera for {live:?}",
                    body.width()
                );
            }
        }
    }
}
