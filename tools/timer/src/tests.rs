use std::cell::Cell;

use notchbase_sdk::protocol::{
    Effects, Event, HandlerId, Node, PickerColumn, Request, Span, Start, Surface, UiValue, Values,
};
use notchbase_sdk::{Driver, Runtime, Waker};

use super::*;

const MINUTE: u64 = 60_000;
const START: u64 = 1_700_000_000_000;

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(0) };
}

struct Fake;

impl Clock for Fake {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }
}

type Tested = Timer<Fake>;

fn at(now: u64) {
    NOW.with(|cell| cell.set(now));
}

fn started_with(settings: Values) -> Runtime<Tested> {
    at(START);
    let mut runtime = Runtime::new(Waker::noop());
    runtime.start(Start { settings, ..Start::default() });
    runtime
}

fn started() -> Runtime<Tested> {
    started_with(Values::default())
}

fn countdown(runtime: &Runtime<Tested>) -> Countdown {
    runtime.tool().expect("a started tool").countdown
}

fn tick(runtime: &mut Runtime<Tested>, now: u64) -> Vec<Request> {
    at(now);
    runtime.handle(vec![Event::Tick { every_ms: 1000 }]).requests
}

fn find<'a>(node: &'a Node, test: &impl Fn(&Node) -> bool) -> Option<&'a Node> {
    if test(node) {
        return Some(node);
    }
    node.children().into_iter().find_map(|child| find(child, test))
}

fn page_of(runtime: &mut Runtime<Tested>) -> Node {
    runtime.views().page.expect("a page")
}

fn round(page: &Node, wanted: &str) -> Node {
    find(page, &|node| matches!(node, Node::RoundButton { label, .. } if label == wanted))
        .cloned()
        .unwrap_or_else(|| panic!("no {wanted} button in {page:?}"))
}

fn press(runtime: &mut Runtime<Tested>, wanted: &str) -> Effects {
    let page = page_of(runtime);
    let Node::RoundButton { on_press, .. } = round(&page, wanted) else { unreachable!() };
    let handler = on_press.unwrap_or_else(|| panic!("{wanted} is disabled"));
    runtime.handle(vec![Event::Ui { surface: Surface::Page, handler, value: UiValue::Press }])
}

fn wheels(page: &Node) -> (Vec<PickerColumn>, HandlerId) {
    let Some(Node::Picker { columns, on_change }) = find(page, &|node| matches!(node, Node::Picker { .. })) else {
        panic!("no picker in {page:?}")
    };
    (columns.clone(), on_change.expect("a live picker"))
}

#[test]
fn opens_on_five_minutes() {
    let mut runtime = started();
    let page = page_of(&mut runtime);
    let (columns, _) = wheels(&page);
    let picked: Vec<(usize, &str, bool)> =
        columns.iter().map(|column| (column.selected, column.unit.as_str(), column.looping)).collect();
    assert_eq!(picked, [(0, "hours", true), (5, "min", true), (0, "sec", true)]);
    assert_eq!(columns.iter().map(|column| column.options.len()).collect::<Vec<_>>(), [24, 60, 60]);
    let Node::RoundButton { on_press, .. } = round(&page, "Cancel") else { unreachable!() };
    assert!(on_press.is_none(), "nothing to cancel yet");
}

#[test]
fn turning_a_wheel_sets_length() {
    let mut runtime = started();
    let (_, handler) = wheels(&page_of(&mut runtime));
    let value = UiValue::Picked { column: 2, index: 30 };
    runtime.handle(vec![Event::Ui { surface: Surface::Page, handler, value }]);
    let (columns, _) = wheels(&page_of(&mut runtime));
    assert_eq!(columns[2].selected, 30);
    assert_eq!(runtime.tool().expect("tool").length.ms(), 5 * MINUTE + 30_000);
}

#[test]
fn start_shows_ring_saves_length() {
    let mut runtime = started();
    let effects = press(&mut runtime, "Start");
    let running = Countdown::start(5 * MINUTE, START).save();
    assert_eq!(
        effects.requests,
        [
            Request::SaveSetting { key: LAST.into(), value: Value::from(5 * MINUTE) },
            Request::SaveSetting { key: RUNNING.into(), value: Value::from(running) },
        ]
    );
    assert_eq!(effects.wants.every_ms, vec![1000]);
    let page = page_of(&mut runtime);
    let ring = find(&page, &|node| matches!(node, Node::Ring { span: Some(_), .. }));
    let Some(Node::Ring { span: Some(span), center: Some(_), .. }) = ring else { panic!("{page:?}") };
    assert_eq!(*span, Span { start: START, end: START + 5 * MINUTE });
    round(&page, "Pause");
    assert!(find(&page, &|node| matches!(node, Node::Picker { .. })).is_none());
}

#[test]
fn pause_freezes_resume_continues() {
    let mut runtime = started();
    press(&mut runtime, "Start");
    at(START + MINUTE);
    let effects = press(&mut runtime, "Pause");
    assert!(effects.wants.every_ms.is_empty(), "no ticks while paused");
    let page = page_of(&mut runtime);
    let Some(Node::Ring { value, span: None, .. }) = find(&page, &|node| matches!(node, Node::Ring { .. })) else {
        panic!("{page:?}")
    };
    assert_eq!(*value, 0.8);
    at(START + 10 * MINUTE);
    press(&mut runtime, "Resume");
    assert_eq!(countdown(&runtime), Countdown::Running { start: START + 9 * MINUTE, end: START + 14 * MINUTE });
}

fn ring(runtime: &mut Runtime<Tested>) -> HandlerId {
    press(runtime, "Start");
    assert!(tick(runtime, START + MINUTE).is_empty());
    let requests = tick(runtime, START + 5 * MINUTE);
    let [Request::Notice { notice }, Request::SaveSetting { .. }] = &requests[..] else { panic!("{requests:?}") };
    assert!(notice.alarm && notice.sticky() && notice.shake, "it rings until it's stopped");
    let Some(Node::Button { label, on_press: Some(stop), .. }) = notice.controls.as_ref() else { panic!("{notice:?}") };
    assert_eq!(label, "Stop");
    *stop
}

#[test]
fn alarm_rings_until_stopped() {
    let mut runtime = started();
    let stop = ring(&mut runtime);
    assert_eq!(countdown(&runtime), Countdown::Idle);
    wheels(&page_of(&mut runtime));
    assert!(tick(&mut runtime, START + 6 * MINUTE).is_empty(), "the notch rings it, not the tool");
    let pressed = runtime.handle(vec![Event::Ui { surface: Surface::Notice, handler: stop, value: UiValue::Press }]);
    assert_eq!(pressed.requests, [Request::DismissNotice]);
}

#[test]
fn a_new_timer_stops_the_alarm() {
    let mut runtime = started();
    ring(&mut runtime);
    let requests = press(&mut runtime, "Start").requests;
    assert_eq!(requests.first(), Some(&Request::DismissNotice), "{requests:?}");
    assert!(countdown(&runtime).running());
    let again = press(&mut runtime, "Pause").requests;
    assert!(!again.contains(&Request::DismissNotice), "only once");
}

#[test]
fn cancel_keeps_the_length() {
    let mut runtime = started();
    press(&mut runtime, "Start");
    press(&mut runtime, "Cancel");
    assert_eq!(countdown(&runtime), Countdown::Idle);
    let (columns, _) = wheels(&page_of(&mut runtime));
    assert_eq!(columns[1].selected, 5);
}

#[test]
fn shortcut_starts_pauses_resumes() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    assert!(countdown(&runtime).running());
    runtime.send(Msg::Toggle);
    assert!(matches!(countdown(&runtime), Countdown::Paused { .. }));
    runtime.send(Msg::Toggle);
    assert!(countdown(&runtime).running());
}

#[test]
fn zero_length_cannot_start() {
    let mut runtime = started();
    for column in 0..3 {
        runtime.send(Msg::Pick(column, 0));
    }
    let page = page_of(&mut runtime);
    let Node::RoundButton { on_press, .. } = round(&page, "Start") else { unreachable!() };
    assert!(on_press.is_none());
    runtime.send(Msg::Toggle);
    assert_eq!(countdown(&runtime), Countdown::Idle);
}

#[test]
fn live_counts_down_with_ring() {
    let mut runtime = started();
    assert!(runtime.views().live.is_none());
    press(&mut runtime, "Start");
    let live = runtime.views().live.expect("a live activity");
    assert!(matches!(live.leading, Node::Ring { span: Some(_), .. }));
    assert!(matches!(live.trailing, Node::Timer { at, .. } if at == START + 5 * MINUTE));
}

fn press_tile(runtime: &mut Runtime<Tested>) -> Effects {
    let tile = runtime.views().tile.expect("a tile");
    assert_eq!(tile.title, "Timer");
    let handler = tile.on_press.expect("a pressable tile");
    runtime.handle(vec![Event::Ui { surface: Surface::Tile, handler, value: UiValue::Press }])
}

#[test]
fn tile_always_opens_the_page() {
    let mut runtime = started();
    assert_eq!(press_tile(&mut runtime).requests, [Request::OpenPage]);
    press(&mut runtime, "Start");
    assert_eq!(press_tile(&mut runtime).requests, [Request::OpenPage]);
    press(&mut runtime, "Pause");
    assert_eq!(press_tile(&mut runtime).requests, [Request::OpenPage]);
    press(&mut runtime, "Cancel");
    assert!(runtime.views().live.is_none(), "cancelling clears the notch");
}

#[test]
fn last_length_comes_back() {
    let settings: Values = [(LAST.to_owned(), Value::from(90 * MINUTE))].into_iter().collect();
    let runtime = started_with(settings);
    assert_eq!(runtime.tool().expect("tool").length.words(), "1 hr 30 min");
}

#[test]
fn live_offers_pause_and_cancel() {
    at(START);
    let mut timer = notchbase_sdk::testing::Harness::<Tested>::start();
    timer.send(Msg::Start);
    assert!(timer.live().and_then(|live| live.controls).is_some());
    timer.click_icon(Icon::Pause, 0);
    assert!(matches!(timer.model().countdown, Countdown::Paused { .. }));
    timer.click_icon(Icon::Play, 0);
    assert!(matches!(timer.model().countdown, Countdown::Running { .. }));
    timer.click_icon(Icon::Xmark, 0);
    assert_eq!(timer.model().countdown, Countdown::Idle);
    assert!(timer.live().is_none());
}

#[test]
fn running_timer_survives_restart() {
    let saved = Countdown::start(5 * MINUTE, START).save();
    let settings: Values = [(RUNNING.to_owned(), Value::from(saved))].into_iter().collect();
    let runtime = started_with(settings);
    at(START + MINUTE);
    assert_eq!(countdown(&runtime), Countdown::Running { start: START, end: START + 5 * MINUTE });
    assert_eq!(countdown(&runtime).left(START + MINUTE), 4 * MINUTE);
}
