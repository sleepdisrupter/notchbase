use std::cell::Cell;

use notchbase_sdk::protocol::{Node, Request};
use notchbase_sdk::testing::Harness;

use super::*;
use prefs::{CITIES, HOME, HOURS};

const NOON_IN_LONDON: u64 = 1_785_927_960_000;
const MINUTE: u64 = 60_000;
const MINUS: char = '\u{2212}';

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(NOON_IN_LONDON) };
}

struct London;

impl Clock for London {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }

    fn zone() -> TimeZone {
        zones::find("Europe/London").expect("a zone")
    }
}

type World = WorldTime<London>;

fn at(now: u64) {
    NOW.with(|cell| cell.set(now));
}

fn started() -> Harness<World> {
    at(NOON_IN_LONDON);
    Harness::start()
}

fn started_with(settings: &[(&str, &str)]) -> Harness<World> {
    at(NOON_IN_LONDON);
    Harness::start_with(settings.iter().copied())
}

fn opened(open: bool) -> NotchState {
    NotchState { open, ..NotchState::default() }
}

fn names(tool: &Harness<World>) -> Vec<&str> {
    tool.model().places.iter().map(|place| place.name.as_str()).collect()
}

fn drawn(tool: &mut Harness<World>) -> Vec<Shape> {
    fn flat(shapes: &[Shape], all: &mut Vec<Shape>) {
        for shape in shapes {
            match shape {
                Shape::Group { shapes, .. } | Shape::Run { parts: shapes, .. } => flat(shapes, all),
                other => all.push(other.clone()),
            }
        }
    }
    let Some(Node::Column { children, .. }) = tool.page() else { panic!("a page") };
    let Some(Node::Canvas(canvas)) = children.first() else { panic!("a canvas") };
    assert!(canvas.stretch && canvas.width == page::WIDTH);
    let mut all = Vec::new();
    flat(&canvas.shapes, &mut all);
    all
}

fn bands(tool: &mut Harness<World>) -> usize {
    drawn(tool).iter().filter(|shape| matches!(shape, Shape::Glass { .. })).count()
}

fn frames(tool: &mut Harness<World>) -> Vec<f32> {
    let frame = |shape: &Shape| match shape {
        Shape::Ellipse { center, stroke: Some(_), .. } => Some(center[0]),
        _ => None,
    };
    drawn(tool).iter().filter_map(frame).collect()
}

fn guides(tool: &mut Harness<World>) -> Vec<f32> {
    let guide = |shape: &Shape| match shape {
        Shape::Line { points, .. } => Some(points[0][0]),
        _ => None,
    };
    drawn(tool).iter().filter_map(guide).collect()
}

fn reads(tool: &mut Harness<World>) -> Vec<String> {
    let read = |shape: &Shape| match shape {
        Shape::Text { text, size, color, .. } if *size == 17.0 && *color == Tone::Yellow.color() => Some(text.clone()),
        _ => None,
    };
    drawn(tool).iter().filter_map(read).collect()
}

fn has(tool: &mut Harness<World>, wanted: &str) -> bool {
    tool.texts().iter().any(|text| text == wanted)
}

fn has_all(tool: &mut Harness<World>, wanted: &[&str]) -> bool {
    wanted.iter().all(|wanted| has(tool, wanted))
}

fn behind(gap: &str) -> String {
    format!("{MINUS}{gap}")
}

fn over(hour: f32) -> f32 {
    page::WIDTH * 0.5 + hour * page::CELL_W
}

fn click(tool: &mut Harness<World>, hour: f32) {
    tool.pointer(0, Pointer::at(PointerKind::Click, over(hour), 40.0));
}

fn drag(tool: &mut Harness<World>, by: f32) {
    tool.pointer(0, Pointer::moved(PointerKind::Drag, [over(0.0), 40.0], [by, 0.0]));
}

fn leave(tool: &mut Harness<World>) {
    tool.pointer(0, Pointer::at(PointerKind::Leave, -1.0, -1.0));
}

fn hover(tool: &mut Harness<World>, at: f32) {
    tool.pointer(0, Pointer::at(PointerKind::Hover, at, 40.0));
}

#[test]
fn home_is_this_computer() {
    let mut tool = started();
    assert_eq!(names(&tool), ["London", "New York", "Tokyo"]);
    assert!(has_all(&mut tool, &["12:06", "7:06", "8:06", "Wed 5", "BST", "EDT", "JST", "+8", &behind("5")]));
    assert!(has(&mut tool, "pm") && has(&mut tool, "am"));
}

#[test]
fn each_city_gets_a_band() {
    let mut tool = started();
    assert_eq!(frames(&mut tool), [over(0.0); 3]);
    let hours =
        drawn(&mut tool).iter().filter(|shape| matches!(shape, Shape::Text { size, .. } if *size == 13.0)).count();
    assert_eq!(hours, 3 * 13);
    assert!(guides(&mut tool).is_empty(), "no cursor until the pointer is over the hours");
    hover(&mut tool, over(2.0));
    let hours =
        drawn(&mut tool).iter().filter(|shape| matches!(shape, Shape::Text { size, .. } if *size == 13.0)).count();
    assert_eq!(hours, 3 * 13, "and the cursor covers none of them");
}

#[test]
fn page_fits_its_cities() {
    let block = page::HEAD_H + page::STRIP_GAP + page::BAND_H;
    let height = |tool: &mut Harness<World>| match tool.page() {
        Some(Node::Column { children, .. }) => match &children[0] {
            Node::Canvas(canvas) => canvas.height,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    assert_eq!(height(&mut started()), 3.0 * block + 2.0 * page::BLOCK_GAP);
    let mut missing = started_with(&[(CITIES[0], "Lodnon"), (CITIES[1], ""), (CITIES[2], "")]);
    assert_eq!(height(&mut missing), block + page::BLOCK_GAP + page::HEAD_H, "a city that isn't found has no band");
}

#[test]
fn band_breaks_at_new_day() {
    let mut tool = started();
    assert_eq!(bands(&mut tool), 2 + 2 + 3);
    assert!(has(&mut tool, "AUG"));
    click(&mut tool, -6.0);
    assert_eq!(tool.model().chosen, -6);
    assert_eq!(bands(&mut tool), 1 + 1 + 2);
}

#[test]
fn click_reads_every_city() {
    let mut tool = started();
    click(&mut tool, 3.2);
    assert_eq!((tool.model().chosen, tool.model().pan), (3, 0.0));
    assert!(has_all(&mut tool, &["3:00", "10:00", "11:00", "In 3 hours"]));
    assert!(!has(&mut tool, "12:06"));
    click(&mut tool, -2.4);
    assert_eq!(tool.model().chosen, -2);
    assert!(has_all(&mut tool, &["10:00", "5:00", "6:00", "2 hours ago"]));
    click(&mut tool, 0.0);
    assert!(has(&mut tool, "12:06"));
}

#[test]
fn drag_slides_bands_together() {
    let mut tool = started();
    drag(&mut tool, -10.0);
    assert_eq!(frames(&mut tool), [over(0.0) - 10.0; 3]);
    leave(&mut tool);
    assert!(has(&mut tool, "12:06"));
    drag(&mut tool, -65.0);
    assert_eq!((tool.model().pan, tool.model().chosen), (2.0, 0));
    assert_eq!(frames(&mut tool), [over(-2.0); 3]);
    tool.pointer(0, Pointer::moved(PointerKind::Scroll, [10.0, 10.0], [30.0, 4.0]));
    assert_eq!(frames(&mut tool), [over(-2.0) + 30.0; 3], "a sideways swipe slides it too");
    tool.pointer(0, Pointer::moved(PointerKind::Scroll, [10.0, 10.0], [0.0, -30.0]));
    assert_eq!(frames(&mut tool), [over(-2.0); 3], "and so does a mouse wheel");
}

#[test]
fn click_after_drag_hits_the_hour() {
    let mut tool = started();
    drag(&mut tool, -20.0 * page::CELL_W);
    assert!(frames(&mut tool).is_empty());
    click(&mut tool, 0.0);
    leave(&mut tool);
    assert_eq!(tool.model().chosen, 20);
    assert_eq!(frames(&mut tool).len(), 3);
    assert!(has_all(&mut tool, &["8:00", "Thu 6", "In 20 hours"]));
}

#[test]
fn cursor_reads_one_moment() {
    let mut tool = started();
    let twenty_five_past = over(-0.5) + page::CELL_W * 25.0 / 60.0;
    hover(&mut tool, twenty_five_past + 1.0);
    assert_eq!(guides(&mut tool), [twenty_five_past + 1.0; 3], "a hairline on every band, right under the pointer");
    assert_eq!(reads(&mut tool), ["12:25", "7:25", "8:25"], "each city's clock reads the nearest five minutes");
    assert!(!has(&mut tool, "12:06") && has(&mut tool, "In 20 min"));
    hover(&mut tool, over(3.5));
    assert_eq!(reads(&mut tool), ["4:00", "11:00", "12:00"]);
    assert!(has_all(&mut tool, &["Thu 6", "In 3 h 55 min"]), "Tokyo is already on tomorrow there");
    leave(&mut tool);
    assert!(guides(&mut tool).is_empty() && reads(&mut tool).is_empty());
    assert_eq!(tool.model().hover, None);
    assert!(has(&mut tool, "12:06"), "the clocks go back to now");
}

#[test]
fn cursor_follows_drag() {
    let mut tool = started();
    hover(&mut tool, over(0.0));
    assert_eq!(reads(&mut tool)[0], "12:30");
    drag(&mut tool, -page::CELL_W);
    assert_eq!(reads(&mut tool)[0], "1:30", "the hours moved under a still pointer");
    click(&mut tool, 0.0);
    assert_eq!(tool.model().chosen, 1);
    assert_eq!(reads(&mut tool)[0], "1:30", "a click keeps the cursor where it is");
    hover(&mut tool, f32::NAN);
    assert!(guides(&mut tool).is_empty());
}

#[test]
fn cursor_reads_half_hour_zones() {
    let mut tool = started_with(&[(CITIES[0], "Asia/Kolkata"), (CITIES[1], ""), (CITIES[2], ""), (HOURS, "24")]);
    hover(&mut tool, over(-0.5) + page::CELL_W * 25.0 / 60.0);
    assert_eq!(reads(&mut tool), ["12:25", "16:55"]);
}

#[test]
fn cursor_distance_is_exact() {
    let spans = [0.0, 2.0, 20.0, 60.0, 154.0, -45.0, -61.0, -1500.0].map(page::apart);
    assert_eq!(spans, ["Now", "Now", "In 20 min", "In 1 h", "In 2 h 35 min", "45 min ago", "1 h ago", "25 h ago"]);
}

#[test]
fn strip_stops_a_week_out() {
    let mut tool = started();
    drag(&mut tool, -1.0e9);
    assert_eq!(tool.model().pan, REACH);
    drag(&mut tool, f32::NAN);
    tool.pointer(0, Pointer::at(PointerKind::Click, f32::NAN, 0.0));
    assert_eq!((tool.model().pan, tool.model().chosen), (REACH, 0));
    drag(&mut tool, 1.0e9);
    assert_eq!(tool.model().pan, -REACH);
    click(&mut tool, -9_000.0);
    assert_eq!(tool.model().chosen, -(REACH as i32) - 6, "a click can't land past the last hour drawn");
}

#[test]
fn now_brings_back_the_present() {
    let mut tool = started();
    let pressable = |tool: &mut Harness<World>| {
        let now = |node: &Node| matches!(node, Node::Button { label, on_press: Some(_), .. } if label == "Now");
        tool.page().expect("a page").any(&now)
    };
    assert!(!pressable(&mut tool));
    assert!(has(&mut tool, "Move over the hours to read a time. Drag to scroll."));
    drag(&mut tool, -3.0);
    assert!(pressable(&mut tool));
    tool.click("Now");
    assert!(!pressable(&mut tool));
    click(&mut tool, 5.0);
    drag(&mut tool, 50.0);
    leave(&mut tool);
    tool.click("Now");
    assert_eq!((tool.model().pan, tool.model().chosen), (0.0, 0));
    assert!(has(&mut tool, "12:06"));
}

#[test]
fn distance_from_now_is_spelt_out() {
    let spans = [1, 5, 24, 25, 50, -1, -30].map(page::away);
    assert_eq!(
        spans,
        [
            "In 1 hour",
            "In 5 hours",
            "In 1 day",
            "In 1 day 1 hour",
            "In 2 days 2 hours",
            "1 hour ago",
            "1 day 6 hours ago"
        ]
    );
}

#[test]
fn ticks_only_while_open() {
    let mut tool = started();
    assert!(tool.wants().every_ms.is_empty());
    tool.notch(opened(true));
    assert_eq!(tool.wants().every_ms, [TICK.as_millis() as u64]);
    at(NOON_IN_LONDON + 2 * MINUTE);
    tool.tick(TICK);
    assert!(has_all(&mut tool, &["12:08", "7:08"]));
    tool.notch(opened(false));
    assert!(tool.wants().every_ms.is_empty());
}

#[test]
fn cities_follow_settings() {
    let mut tool =
        started_with(&[(HOME, "Tokyo"), (CITIES[0], "san francisco"), (CITIES[1], "Asia/Kolkata"), (CITIES[2], "")]);
    assert_eq!(names(&tool), ["Tokyo", "San Francisco", "Kolkata"]);
    assert!(has_all(&mut tool, &[&behind("16"), "PDT", &behind("3:30"), "IST", "8:06", "4:06", "4:36"]));
    tool.change_settings([(HOME, ""), (CITIES[0], "Europe/Paris")]);
    assert_eq!(names(&tool), ["London", "Paris", "Kolkata"]);
    assert!(has_all(&mut tool, &["+1", "CEST", "+4:30"]));
}

#[test]
fn listed_city_is_not_repeated() {
    let tool = started_with(&[(CITIES[0], "london"), (CITIES[1], "Berlin"), (CITIES[2], "berlin")]);
    assert_eq!(names(&tool), ["London", "Berlin"]);
}

#[test]
fn misspelt_city_says_so() {
    let mut tool = started_with(&[(CITIES[0], "Lodnon"), (CITIES[1], "Tokyo"), (CITIES[2], "")]);
    assert!(has_all(&mut tool, &["Lodnon", "Not found. Try a bigger city nearby.", "8:06"]));
    assert_eq!(frames(&mut tool).len(), 2);
}

#[test]
fn misspelt_home_falls_back() {
    let mut tool = started_with(&[(HOME, "Nowhere"), (CITIES[0], "Tokyo"), (CITIES[1], ""), (CITIES[2], "")]);
    assert!(has_all(&mut tool, &["Not found. Try a bigger city nearby.", "+8", "JST"]));
}

#[test]
fn no_cities_points_to_settings() {
    let mut tool = started_with(&[(CITIES[0], ""), (CITIES[1], ""), (CITIES[2], "")]);
    assert!(tool.shows("Add cities in Settings to compare them."));
    assert!(tool.tile().expect("a tile").footer.is_none());
}

#[test]
fn full_day_drops_am_pm() {
    let mut tool = started_with(&[(HOURS, "24")]);
    assert!(has_all(&mut tool, &["12:06", "07:06", "20:06", ":00"]));
    assert!(!has(&mut tool, "pm") && !has(&mut tool, ""));
}

#[test]
fn tile_shows_next_city() {
    let mut tool = started();
    let footer = tool.tile().expect("a tile").footer.expect("a footer");
    assert!(footer.any(&|node| matches!(node, Node::Text(text) if text.text == "New York 7:06 AM")));
    assert_eq!(tool.press_tile(), [Request::OpenPage]);
    assert_eq!(tool.shortcut(OPEN), [Request::OpenPage]);
}
