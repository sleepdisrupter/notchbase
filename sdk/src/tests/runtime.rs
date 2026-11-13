use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::counter::{Counter, Msg};
use crate::Driver;
use crate::driver::Waker;
use crate::protocol::{Event, HandlerId, Node, Reply, Request, Start, Surface, UiValue, Values};
use crate::runtime::Runtime;
use crate::{Command, Subscription};

fn started() -> Runtime<Counter> {
    let mut runtime = Runtime::<Counter>::new(Waker::noop());
    runtime.start(Start::default());
    runtime
}

fn press(surface: Surface, handler: HandlerId) -> Event {
    Event::Ui { surface, handler, value: UiValue::Press }
}

#[test]
fn start_reports_wants() {
    let mut runtime = Runtime::<Counter>::new(Waker::noop());
    let effects = runtime.start(Start::default());
    assert_eq!(effects.wants.every_ms, vec![1000]);
    assert!(effects.wants.settings && !effects.wants.media);
    assert!(effects.requests.is_empty());
    assert!(effects.revision > 0);
}

#[test]
fn tile_presses_run_messages() {
    let mut runtime = started();
    let tile = runtime.views().tile.expect("a tile");
    let before = runtime.handle(Vec::new()).revision;
    runtime.handle(vec![press(Surface::Tile, tile.on_press.expect("pressable"))]);
    let Some(Node::Row { children, .. }) = tile.accessory else { panic!("an accessory row") };
    let Node::Button { on_press: Some(plus_five), .. } = children[0] else { panic!("a button") };
    let effects = runtime.handle(vec![press(Surface::Tile, plus_five)]);
    assert_eq!(runtime.tool().unwrap().count, 6);
    assert!(effects.revision > before);
}

#[test]
fn toggle_saves_its_setting() {
    let mut runtime = started();
    let tile = runtime.views().tile.unwrap();
    let Some(Node::Row { children, .. }) = tile.accessory else { panic!() };
    let Node::Toggle { on_toggle: Some(handler), .. } = children[1] else { panic!("a toggle") };
    let effects = runtime.handle(vec![Event::Ui { surface: Surface::Tile, handler, value: UiValue::Bool(true) }]);
    assert!(matches!(&effects.requests[..], [Request::SaveSetting { key, .. }] if key == "loud"));
    let mut values = Values::new();
    values.insert("loud".into(), true.into());
    runtime.handle(vec![Event::Settings { values }]);
    assert!(runtime.tool().unwrap().loud);
}

#[test]
fn replies_reach_the_asker() {
    let mut runtime = started();
    let page = runtime.views().page.unwrap();
    let Node::Column { children, .. } = page else { panic!() };
    let Node::Button { on_press: Some(pick), .. } = children[1] else { panic!() };
    let effects = runtime.handle(vec![press(Surface::Page, pick)]);
    let [Request::PickColor { reply }] = effects.requests[..] else { panic!("{:?}", effects.requests) };
    let color = crate::protocol::Color::rgb(1, 2, 3);
    runtime.handle(vec![Event::Reply { id: reply, reply: Reply::Color { color: Some(color) } }]);
    assert_eq!(runtime.tool().unwrap().picked, Some(color));
    runtime.handle(vec![Event::Reply { id: reply, reply: Reply::Color { color: None } }]);
    assert_eq!(runtime.tool().unwrap().picked, Some(color), "a reply is only answered once");
}

#[test]
fn ticks_and_shortcuts_arrive() {
    let mut runtime = started();
    runtime.handle(vec![Event::Tick { every_ms: 1000 }, Event::Tick { every_ms: 250 }]);
    runtime.handle(vec![Event::Shortcut { id: "add".into() }, Event::Shortcut { id: "other".into() }]);
    let tool = runtime.tool().unwrap();
    assert_eq!((tool.ticks, tool.count), (1, 1));
}

#[test]
fn background_work_wakes_host() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    let mut runtime = Runtime::<Counter>::new(Waker::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    }));
    runtime.start(Start::default());
    runtime.send(Msg::Later(21));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while wakes.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    runtime.handle(Vec::new());
    assert_eq!(runtime.tool().unwrap().count, 42);
}

#[test]
fn stale_handlers_are_ignored() {
    let mut runtime = started();
    runtime.views();
    runtime.handle(vec![press(Surface::Tile, HandlerId(99))]);
    runtime.handle(vec![Event::Ui { surface: Surface::Live, handler: HandlerId(0), value: UiValue::Press }]);
    assert_eq!(runtime.tool().unwrap().count, 0);
}

#[test]
fn commands_and_subs_map_to_parent() {
    #[derive(Clone, Debug, PartialEq)]
    enum Parent {
        Child(Msg),
    }
    let command: Command<Parent> =
        Command::batch([Command::message(Msg::Add(2)), Command::notify("hi")]).map(Parent::Child);
    assert_eq!(command.effects.len(), 2);
    let subscription = Subscription::shortcut("add", Msg::Add(1)).map(Parent::Child);
    let routed = subscription.route(&Event::Shortcut { id: "add".into() }, &crate::Settings::default());
    assert_eq!(routed, vec![Parent::Child(Msg::Add(1))]);
}

#[test]
fn settings_start_from_defaults() {
    let mut runtime = Runtime::<Counter>::new(Waker::noop());
    let mut values = Values::new();
    values.insert("loud".into(), true.into());
    runtime.start(Start { settings: values, ..Start::default() });
    assert!(runtime.tool().unwrap().loud);
    assert_eq!(runtime.settings().number("step"), 1.0);
}
