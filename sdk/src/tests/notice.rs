use crate::Driver;
use crate::driver::Waker;
use crate::prelude::*;
use crate::protocol::{Event, HandlerId, Node, NoticeNode, Request, Start, Surface, UiValue};
use crate::runtime::Runtime;
use crate::testing::Harness;

#[derive(Default)]
struct Shooter {
    annotated: Vec<ImageId>,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Shot(ImageId),
    Plain,
    Pressed(Press),
}

#[derive(Clone, Debug, PartialEq)]
enum Press {
    Annotate(ImageId),
}

impl Tool for Shooter {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("shooter", "Shooter")
    }

    fn init(_: &Context) -> (Self, Command<Msg>) {
        (Self::default(), Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Shot(image) => {
                let controls = button("Annotate").on_press(Press::Annotate(image));
                Command::notice_with_controls(shown(), controls).map(Msg::Pressed)
            }
            Msg::Plain => Command::notice(shown()),
            Msg::Pressed(Press::Annotate(image)) => {
                self.annotated.push(image);
                Command::dismiss_notice().and(Command::annotate(image))
            }
        }
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        Some(Tile::new("Shooter").on_press(Msg::Plain))
    }
}

fn shown() -> Notice {
    Notice::new(text("Copied"), text("2560×1600"))
}

fn started() -> Runtime<Shooter> {
    let mut runtime = Runtime::<Shooter>::new(Waker::noop());
    runtime.start(Start::default());
    runtime
}

fn noticed(requests: &[Request]) -> &NoticeNode {
    let [Request::Notice { notice }] = requests else { panic!("{requests:?}") };
    notice
}

fn pressable(notice: &NoticeNode) -> HandlerId {
    let Some(Node::Button { on_press: Some(handler), .. }) = notice.controls else { panic!("{notice:?}") };
    handler
}

fn press(handler: HandlerId) -> Event {
    Event::Ui { surface: Surface::Notice, handler, value: UiValue::Press }
}

#[test]
fn annotate_goes_over_the_wire() {
    let request = serde_json::to_string(&Request::Annotate { image: ImageId(7) }).unwrap();
    assert_eq!(request, r#"{"type":"annotate","image":7}"#);
    let read: Request = serde_json::from_str(&request).unwrap();
    assert_eq!(read, Request::Annotate { image: ImageId(7) });
}

#[test]
fn notice_controls_round_trip() {
    let mut runtime = started();
    let requests = runtime.send(Msg::Shot(ImageId(7))).requests;
    let notice = noticed(&requests);
    let wire = serde_json::to_string(notice).unwrap();
    assert!(wire.contains(r#""controls":{"#), "{wire}");
    assert_eq!(&serde_json::from_str::<NoticeNode>(&wire).unwrap(), notice);
    let answered = runtime.handle(vec![press(pressable(notice))]).requests;
    assert_eq!(answered, [Request::DismissNotice, Request::Annotate { image: ImageId(7) }]);
    assert_eq!(runtime.tool().unwrap().annotated, [ImageId(7)]);
}

#[test]
fn notice_control_survives_redraw() {
    let mut runtime = started();
    let requests = runtime.send(Msg::Shot(ImageId(7))).requests;
    let handler = pressable(noticed(&requests));
    runtime.views();
    runtime.views();
    runtime.handle(vec![press(handler)]);
    assert_eq!(runtime.tool().unwrap().annotated, [ImageId(7)]);
}

#[test]
fn plain_notice_format_is_unchanged() {
    let mut runtime = started();
    let requests = runtime.send(Msg::Plain).requests;
    let wire = serde_json::to_string(noticed(&requests)).unwrap();
    let fields: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&wire).unwrap();
    let names: Vec<&str> = fields.keys().map(String::as_str).collect();
    assert_eq!(names, ["leading", "trailing", "below", "banner", "duration_ms", "pop", "shake", "size"]);
    let older: NoticeNode = serde_json::from_str(&wire).unwrap();
    assert_eq!(older.controls, None);
}

#[test]
fn plain_notice_drops_old_controls() {
    let mut runtime = started();
    let requests = runtime.send(Msg::Shot(ImageId(7))).requests;
    let stale = pressable(noticed(&requests));
    runtime.send(Msg::Plain);
    let answered = runtime.handle(vec![press(stale)]).requests;
    assert!(answered.is_empty(), "{answered:?}");
    assert!(runtime.tool().unwrap().annotated.is_empty());
}

#[test]
fn newer_notice_takes_presses() {
    let mut runtime = started();
    runtime.send(Msg::Shot(ImageId(7)));
    let requests = runtime.send(Msg::Shot(ImageId(8))).requests;
    runtime.handle(vec![press(HandlerId(99)), press(pressable(noticed(&requests)))]);
    assert_eq!(runtime.tool().unwrap().annotated, [ImageId(8)]);
}

#[test]
fn harness_presses_last_notice() {
    let mut tool = Harness::<Shooter>::start();
    tool.send(Msg::Shot(ImageId(7)));
    assert_eq!(tool.noticed(), ["Copied", "2560×1600", "Annotate"]);
    tool.press_tile();
    assert_eq!(tool.noticed(), ["Copied", "2560×1600"]);
    tool.send(Msg::Shot(ImageId(9)));
    tool.tile();
    let answered = tool.click_notice("Annotate").to_vec();
    assert_eq!(answered, [Request::DismissNotice, Request::Annotate { image: ImageId(9) }]);
    assert_eq!(tool.model().annotated, [ImageId(9)]);
}

#[test]
#[should_panic(expected = "the last notice has no control \"Annotate\"")]
fn harness_skips_replaced_controls() {
    let mut tool = Harness::<Shooter>::start();
    tool.send(Msg::Shot(ImageId(7)));
    tool.send(Msg::Plain);
    tool.click_notice("Annotate");
}
