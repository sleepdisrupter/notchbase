use crate::element::{Handlers, lower_static};
use crate::prelude::*;
use crate::protocol::{Node, TextNode};

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Picked(usize),
}

#[test]
fn nodes_travel_as_tagged_json() {
    let node = lower_static(text("Hello").headline().secondary());
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["style"], "headline");
    assert_eq!(json["tone"], "secondary");
    let back: Node = serde_json::from_value(json).unwrap();
    assert_eq!(back, node);
}

#[test]
fn minimal_json_node_gets_defaults() {
    let node: Node = serde_json::from_str(r#"{"type":"text","text":"hi"}"#).unwrap();
    let expected =
        TextNode { text: "hi".into(), style: TextStyle::Body, tone: Tone::Primary, wrap: false, align: Align::Start };
    assert_eq!(node, Node::Text(expected));
}

#[test]
fn every_handler_gets_its_own_id() {
    let element: Element<Msg> = crate::row![
        dropdown(["A", "B"], Some(0)).on_select(Msg::Picked),
        button("Go").on_press(Msg::Picked(9)),
        text("static"),
    ]
    .spacing(6.0)
    .into();
    let mut handlers = Handlers::default();
    let Node::Row { children, spacing, .. } = element.lower(&mut handlers) else { panic!() };
    assert_eq!(spacing, Some(6.0));
    let Node::Dropdown { on_select: Some(select), .. } = children[0] else { panic!() };
    let Node::Button { on_press: Some(go), .. } = children[1] else { panic!() };
    assert_ne!(select, go);
    assert_eq!(handlers.dispatch(select, crate::protocol::UiValue::Index(1)), Some(Msg::Picked(1)));
    assert_eq!(handlers.dispatch(go, crate::protocol::UiValue::Press), Some(Msg::Picked(9)));
}

#[test]
fn child_view_maps_to_parent() {
    #[derive(Clone, Debug, PartialEq)]
    enum Parent {
        Child(Msg),
    }
    let child: Element<Msg> =
        crate::row![button("Go").on_press(Msg::Picked(3)), dropdown(["A"], None).on_select(Msg::Picked)].into();
    let mut handlers = Handlers::default();
    let Node::Row { children, .. } = child.map(Parent::Child).lower(&mut handlers) else { panic!() };
    let Node::Button { on_press: Some(go), .. } = children[0] else { panic!() };
    let Node::Dropdown { on_select: Some(pick), .. } = children[1] else { panic!() };
    assert_eq!(handlers.dispatch(go, crate::protocol::UiValue::Press), Some(Parent::Child(Msg::Picked(3))));
    assert_eq!(handlers.dispatch(pick, crate::protocol::UiValue::Index(0)), Some(Parent::Child(Msg::Picked(0))));
}

#[test]
fn sizing_wraps_in_a_frame() {
    let node = lower_static(text("x").width(80.0));
    assert!(matches!(node, Node::Frame { width: Some(80.0), height: None, .. }));
    let padded = lower_static(card(text("x")).padding(4.0));
    assert!(matches!(padded, Node::Card { padding: Some(4.0), .. }));
}

#[test]
fn banner_notice_keeps_its_layout() {
    let notice = Notice::new(badge(Icon::Bell, Tone::Blue, 30.0), text("Deploy").headline())
        .below(text("All green").callout().wrap())
        .banner()
        .into_node();
    let json = serde_json::to_value(&notice).unwrap();
    assert_eq!(json["banner"], true);
    assert_eq!(serde_json::from_value::<crate::protocol::NoticeNode>(json.clone()).unwrap(), notice);
    let mut older = json;
    older.as_object_mut().unwrap().remove("banner");
    assert!(!serde_json::from_value::<crate::protocol::NoticeNode>(older).unwrap().banner);
}

#[test]
fn pan_hears_drags_and_taps() {
    #[derive(Clone, Debug, PartialEq)]
    enum Strip {
        Moved(f32),
        Tapped(f32),
    }
    let strip: Element<Strip> =
        pan(crate::row![text("9"), text("10")]).offset(-12.5).on_pan(Strip::Moved).on_tap(Strip::Tapped).into();
    let mut handlers = Handlers::default();
    let node = strip.lower(&mut handlers);
    let Node::Pan { offset, on_pan: Some(moved), on_tap: Some(tapped), .. } = &node else { panic!() };
    assert_eq!(*offset, -12.5);
    assert_eq!(handlers.dispatch(*moved, crate::protocol::UiValue::Number(4.0)), Some(Strip::Moved(4.0)));
    assert_eq!(handlers.dispatch(*tapped, crate::protocol::UiValue::Number(60.0)), Some(Strip::Tapped(60.0)));
    assert!(node.any(&|node| matches!(node, Node::Text(text) if text.text == "10")));
    let quiet: Node = serde_json::from_str(r#"{"type":"pan","child":{"type":"divider"}}"#).unwrap();
    assert_eq!(quiet, Node::Pan { child: Box::new(Node::Divider), offset: 0.0, on_pan: None, on_tap: None });
}

#[test]
fn canvas_hears_the_pointer() {
    use crate::protocol::{Paint, Pointer, PointerKind, Shape, UiValue};
    #[derive(Clone, Debug, PartialEq)]
    enum Sketch {
        Pointed(Pointer),
        Scrolled(Pointer),
    }
    let drawing: Element<Sketch> = canvas(200.0, 80.0)
        .push(draw::rect(0.0, 0.0, 200.0, 80.0).radius(12.0).fill(draw::linear(Tone::Red, Tone::Yellow, 90.0)))
        .push(draw::circle(40.0, 40.0, 10.0).fill(Tone::White).stroke(2.0, Tone::Gray))
        .extend([draw::text("hello", 100.0, 40.0).size(20.0).semibold().centered().opacity(0.5).offset(4.0, 2.0)])
        .stretch()
        .on_pointer(Sketch::Pointed)
        .on_scroll(Sketch::Scrolled)
        .into();
    let mut handlers = Handlers::default();
    let node = drawing.lower(&mut handlers);
    let Node::Canvas(canvas) = &node else { panic!() };
    assert_eq!((canvas.width, canvas.height, canvas.stretch, canvas.shapes.len()), (200.0, 80.0, true, 3));
    let Shape::Rect { radius, fill: Some(Paint::Linear { angle, .. }), stroke: None, .. } = canvas.shapes[0] else {
        panic!()
    };
    assert_eq!((radius, angle), (12.0, 90.0));
    let Shape::Group { shapes, opacity, offset, scale, clip } = &canvas.shapes[2] else { panic!() };
    assert_eq!(
        (*opacity, *offset, *scale, *clip, shapes.len()),
        (0.5, [4.0, 2.0], 1.0, None, 1),
        "one group holds both"
    );
    let press = Pointer::at(PointerKind::Press, 3.0, 4.0);
    let (pointed, scrolled) = (canvas.on_pointer.unwrap(), canvas.on_scroll.unwrap());
    assert_eq!(handlers.dispatch(pointed, UiValue::Pointer(press)), Some(Sketch::Pointed(press)));
    assert_eq!(handlers.dispatch(scrolled, UiValue::Pointer(press)), Some(Sketch::Scrolled(press)));
    let back: Node = serde_json::from_value(serde_json::to_value(&node).unwrap()).unwrap();
    assert_eq!(back, node);
}

#[test]
fn bare_json_shape_gets_defaults() {
    use crate::protocol::{Shape, Stroke};
    let group: Shape = serde_json::from_str(r#"{"type":"group","shapes":[]}"#).unwrap();
    assert_eq!(group, draw::group([]));
    let line: Shape = serde_json::from_str(
        r#"{"type":"line","points":[[0,0],[4,4]],"stroke":{"width":1.0,"color":{"r":255,"g":255,"b":255,"a":217}}}"#,
    )
    .unwrap();
    assert_eq!(line, draw::line([(0.0, 0.0), (4.0, 4.0)]));
    assert_eq!(Stroke::default().color, Tone::Primary.color());
}

#[test]
fn mismatched_settings_are_ignored() {
    let dot = draw::circle(1.0, 2.0, 3.0);
    assert_eq!(dot.clone().size(40.0).radius(9.0).semibold().wrap(10.0).blur(4.0), dot);
    assert_eq!(draw::line([(0.0, 0.0)]).fill(Tone::Red), draw::line([(0.0, 0.0)]));
}

#[test]
fn arcs_and_curves_are_points() {
    let quarter = draw::arc(0.0, 0.0, 10.0, 0.0, 90.0);
    let (first, last) = (quarter[0], quarter[quarter.len() - 1]);
    assert!((first.0 - 10.0).abs() < 1e-4 && first.1.abs() < 1e-4);
    assert!(last.0.abs() < 1e-4 && (last.1 - 10.0).abs() < 1e-4);
    assert!(quarter.iter().all(|(x, y)| (x.hypot(*y) - 10.0).abs() < 1e-3));
    let bend = draw::curve((0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0));
    assert_eq!((bend[0], bend[bend.len() - 1]), ((0.0, 0.0), (10.0, 0.0)));
    assert_eq!(bend[bend.len() / 2], (5.0, 7.5));
}

#[test]
fn pointer_knows_what_it_is_over() {
    use crate::protocol::{Pointer, PointerKind};
    let pointer = Pointer::at(PointerKind::Hover, 12.0, 8.0);
    assert!(pointer.within(10.0, 0.0, 5.0, 10.0) && !pointer.within(0.0, 0.0, 12.0, 10.0));
    assert!(pointer.near(12.0, 11.0, 3.0) && !pointer.near(20.0, 8.0, 5.0));
}

#[test]
fn run_takes_anchor_and_gap() {
    use crate::protocol::Shape;
    let time = draw::run(
        [draw::text("10:57", 0.0, 0.0).size(17.0).semibold(), draw::icon(Icon::Clock, 0.0, 0.0)],
        340.0,
        11.0,
    )
    .anchor(Anchor::Right)
    .gap(6.0);
    let Shape::Run { at, parts, gap, anchor } = &time else { panic!() };
    assert_eq!((*at, parts.len(), *gap, *anchor), ([340.0, 11.0], 2, 6.0, Anchor::Right));
    let bare: Shape = serde_json::from_str(r#"{"type":"run","at":[0,0],"parts":[]}"#).unwrap();
    assert_eq!(bare, draw::run([], 0.0, 0.0).gap(0.0));
}
