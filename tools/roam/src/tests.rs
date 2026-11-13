use notchbase_sdk::testing::Harness;

use super::*;

struct Offline;

impl Backend for Offline {
    fn start(_: &Context, _: Prefs, _: Vec<Placement>) -> Option<(Engine, Receiver<Status>)> {
        None
    }
}

type Tool = Roam<Offline>;

fn view(id: &str, name: &str, os: Os, link: LinkState) -> PeerView {
    PeerView {
        id: id.into(),
        name: name.into(),
        os,
        displays: vec![Rect::new(0.0, 0.0, 2560.0, 1440.0)],
        ppu: 1.5,
        open: true,
        nearby: true,
        paired: false,
        link,
        rtt: Some(2.4),
    }
}

fn status(peers: Vec<PeerView>) -> Status {
    let mut me = view("me", "MacBook", Os::Mac, LinkState::None);
    me.displays = vec![Rect::new(0.0, 0.0, 1512.0, 982.0)];
    me.ppu = 1.0;
    Status { me: Some(me), peers, ..Status::default() }
}

fn desk(harness: &Harness<Tool>) -> Vec<Spot> {
    serde_json::from_value(harness.saved(DESK).cloned().unwrap_or_default()).unwrap_or_default()
}

#[test]
fn nearby_computers_can_be_added_to_the_desk() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::None)])));
    assert!(harness.shows("Desk PC"));
    assert!(!harness.shows("No other computers yet. Open notchbase on another Mac or PC on this network."));
    harness.click("Add");
    let spots = desk(&harness);
    assert_eq!(spots.len(), 2, "this computer and the PC");
    assert_eq!(spots[0].id, "me");
    assert_eq!(spots[1].at, [1512.0, 0.0], "placed to the right");
    assert!(harness.shows("Drag the screens to match your desk"));
}

#[test]
fn dragging_a_screen_snaps_it_next_to_another() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)])));
    harness.click("Add");
    let view = View::fit(&harness.model().desk);
    // Grab the PC by its middle and drop it a little below and away from the Mac's right edge.
    let pc = harness.model().desk[1].computer().rect();
    let grab = view.to_canvas([pc.x + pc.w / 2.0, pc.y + pc.h / 2.0]);
    let target = view.to_canvas([pc.x + pc.w / 2.0 + 30.0, pc.y + pc.h / 2.0 + 300.0]);
    harness.pointer(0, Pointer::at(PointerKind::Press, grab[0], grab[1]));
    harness.pointer(0, Pointer::at(PointerKind::Drag, target[0], target[1]));
    harness.pointer(0, Pointer::at(PointerKind::Release, target[0], target[1]));
    let spots = desk(&harness);
    assert_eq!(spots[1].at[0], 1512.0, "snapped against the Mac");
    assert!((spots[1].at[1] - 300.0).abs() < 1.0, "{:?}", spots[1].at);
}

#[test]
fn a_request_to_be_controlled_pops_up_with_allow() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Pairing)]);
    update.asks = vec![Ask { id: "pc".into(), name: "Desk PC".into(), code: "123 456".into(), controlling: false }];
    harness.send(Msg::Status(update.clone()));
    assert!(harness.noticed().iter().any(|n| n.contains("123 456")), "{:?}", harness.noticed());
    assert!(harness.shows("123 456"));
    assert!(harness.shows("Allow"));
    // The same request doesn't pop up twice.
    harness.send(Msg::Status(update));
    assert!(harness.noticed().is_empty());
}

#[test]
fn the_controller_checks_the_code() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Pairing)]);
    update.asks = vec![Ask { id: "pc".into(), name: "Desk PC".into(), code: "042 007".into(), controlling: true }];
    harness.send(Msg::Status(update));
    assert!(harness.shows("Check that Desk PC shows this code"));
    assert!(harness.shows("It matches"));
    assert!(harness.noticed().is_empty(), "no pop-up on the computer you're using");
}

#[test]
fn the_cursor_away_shows_in_the_notch_and_comes_back() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]);
    update.away_on = Some("Desk PC".into());
    harness.send(Msg::Status(update));
    assert!(harness.live().is_some());
    assert!(harness.shows("On Desk PC"));
    harness.click("Bring back");
}

#[test]
fn connected_computers_show_their_latency() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)])));
    harness.click("Add");
    harness.send(Msg::Select(Some("pc".into())));
    assert!(harness.shows("Connected · 2 ms"));
    harness.click("Remove");
    assert_eq!(desk(&harness).len(), 1);
}

#[test]
fn settings_turn_into_prefs() {
    let harness = Harness::<Tool>::start_with([(ADDRESSES, "10.0.0.2, pc.local ;10.0.0.9")]);
    let prefs = &harness.model().prefs;
    assert_eq!(prefs.addresses, vec!["10.0.0.2", "pc.local", "10.0.0.9"]);
    assert!(prefs.open && prefs.clipboard && prefs.modifiers == keys::Modifiers::Same && prefs.smoothing);
}

#[test]
fn an_address_can_be_typed_in() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![]);
    update.addresses = vec!["192.168.50.61".into()];
    harness.send(Msg::Status(update));
    assert!(harness.shows("This computer is 192.168.50.61"));
    harness.type_text("Other computer's IP address", "192.168.50.70");
    harness.click("Connect");
    assert_eq!(harness.saved(ADDRESSES), Some(&Value::from("192.168.50.70")));
    assert!(harness.shows("192.168.50.70"));
    harness.click_icon(Icon::Xmark, 0);
    assert_eq!(harness.saved(ADDRESSES), Some(&Value::from("")));
}

#[test]
fn each_computer_has_its_own_scroll_speed() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)])));
    harness.click("Add");
    harness.send(Msg::Select(Some("pc".into())));
    assert!(harness.shows("Scroll speed"));
    assert!(harness.shows("1×"));
    harness.slide(0, 2.6);
    assert_eq!(desk(&harness)[1].scroll, 2.5, "rounded to the step");
    assert!(harness.shows("2.5×"));
    assert_eq!(desk(&harness)[0].scroll, 1.0, "this computer is left alone");
    assert_eq!(placements(&harness.model().desk)[1].scroll, 2.5, "the engine gets it too");
}

#[test]
fn a_desk_saved_before_scroll_speeds_reads_as_one() {
    let saved = r#"{"id":"pc","name":"Desk PC","displays":[],"ppu":1.0,"at":[0.0,0.0]}"#;
    let spot: Spot = serde_json::from_str(saved).unwrap();
    assert_eq!(spot.scroll, 1.0);
    assert_eq!(times(0.25), "0.25×");
    assert_eq!(times(4.0), "4×");
}
