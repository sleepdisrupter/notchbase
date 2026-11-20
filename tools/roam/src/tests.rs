use engine::Landed;
use notchbase_sdk::protocol::Request;
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
        lends: true,
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
    harness.send(Msg::Status(Box::new(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::None)]))));
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
    harness.send(Msg::Status(Box::new(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]))));
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
    harness.send(Msg::Status(Box::new(update.clone())));
    assert!(harness.noticed().iter().any(|n| n.contains("123 456")), "{:?}", harness.noticed());
    assert!(harness.shows("123 456"));
    assert!(harness.shows("Allow"));
    // The same request doesn't pop up twice.
    harness.send(Msg::Status(Box::new(update)));
    assert!(harness.noticed().is_empty());
}

#[test]
fn the_controller_checks_the_code() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Pairing)]);
    update.asks = vec![Ask { id: "pc".into(), name: "Desk PC".into(), code: "042 007".into(), controlling: true }];
    harness.send(Msg::Status(Box::new(update)));
    assert!(harness.shows("Check that Desk PC shows this code"));
    assert!(harness.shows("It matches"));
    assert!(harness.noticed().is_empty(), "no pop-up on the computer you're using");
}

#[test]
fn the_cursor_away_shows_in_the_notch_and_comes_back() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]);
    update.away_on = Some("Desk PC".into());
    harness.send(Msg::Status(Box::new(update)));
    assert!(harness.live().is_some());
    assert!(harness.shows("On Desk PC"));
    harness.click("Bring back");
}

#[test]
fn connected_computers_show_their_latency() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(Box::new(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]))));
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
    harness.send(Msg::Status(Box::new(update)));
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
    harness.send(Msg::Status(Box::new(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]))));
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

#[test]
fn a_big_copy_shows_how_far_along_it_is_and_says_when_it_is_ready() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]);
    update.transfer = Some(Transfer {
        incoming: true,
        with: "Desk PC".into(),
        what: "holiday.mov".into(),
        done: 1_250_000_000,
        total: 5_000_000_000,
    });
    harness.send(Msg::Status(Box::new(update.clone())));
    assert!(harness.shows("Getting holiday.mov from Desk PC"));
    assert!(harness.shows("1.25 GB of 5.00 GB"));
    assert_eq!(harness.model().summary(), "Getting holiday.mov · 25%");
    assert!(harness.live().is_some());
    let told = |harness: &Harness<Tool>| {
        harness
            .requests()
            .iter()
            .filter(|r| matches!(r, Request::Notify { text } if text.contains("ready to paste")))
            .count()
    };
    update.transfer = None;
    update.landed = Some(Landed { serial: 1, text: "holiday.mov from Desk PC is ready to paste".into() });
    harness.send(Msg::Status(Box::new(update.clone())));
    assert_eq!(told(&harness), 1);
    assert!(!harness.shows("Getting holiday.mov from Desk PC"));
    harness.send(Msg::Status(Box::new(update)));
    assert_eq!(told(&harness), 0, "told once");
    assert_eq!(size_words(840_000), "840 KB");
    assert_eq!(size_words(12_400_000), "12.4 MB");
}

#[test]
fn right_clicking_another_screen_offers_to_make_it_a_display() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]);
    update.ability = Ability::Ready;
    harness.send(Msg::Status(Box::new(update.clone())));
    harness.click("Add");
    let view = View::fit(&harness.model().desk);
    let pc = harness.model().desk[1].computer().rect();
    let [x, y] = view.to_canvas([pc.x + pc.w / 2.0, pc.y + pc.h / 2.0]);
    harness.right_click(0, x, y);
    assert_eq!(harness.model().menu, Some(("pc".to_owned(), 0)));
    assert!(harness.shows("Desk PC's screen"));
    harness.click("Turn into extended display");
    assert_eq!(harness.model().menu, None, "the menu closes once chosen");

    // While it's a display, the same screen offers to disconnect it.
    update.extended = vec![ExtendedView {
        id: "pc".into(),
        name: "Desk PC".into(),
        display: 0,
        working: None,
        running: true,
        failed: None,
    }];
    harness.send(Msg::Status(Box::new(update)));
    assert!(harness.shows("Desk PC's screen is a display of this computer"));
    harness.right_click(0, x, y);
    assert!(harness.shows("Disconnect display"));

    // This computer's own screen has no menu.
    let mac = harness.model().desk[0].computer().rect();
    let [x, y] = view.to_canvas([mac.x + mac.w / 2.0, mac.y + mac.h / 2.0]);
    harness.right_click(0, x, y);
    assert_eq!(harness.model().menu, None);
}

#[test]
fn a_computer_that_cant_lend_its_screen_says_why() {
    let mut harness = Harness::<Tool>::start();
    let mut pc = view("pc", "Desk PC", Os::Windows, LinkState::Connected);
    pc.lends = false;
    let mut update = status(vec![pc]);
    update.ability = Ability::Ready;
    harness.send(Msg::Status(Box::new(update)));
    harness.click("Add");
    let view = View::fit(&harness.model().desk);
    let rect = harness.model().desk[1].computer().rect();
    let [x, y] = view.to_canvas([rect.x + rect.w / 2.0, rect.y + rect.h / 2.0]);
    harness.right_click(0, x, y);
    assert!(harness.shows("Update notchbase on Desk PC first"));
}

/// Every shape drawn on the page's canvases, as JSON.
fn shapes(harness: &mut Harness<Tool>) -> Vec<serde_json::Value> {
    fn walk(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Array(shapes)) = map.get("shapes")
                    && map.contains_key("width")
                {
                    out.extend(shapes.iter().cloned());
                }
                map.values().for_each(|v| walk(v, out));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let page = serde_json::to_value(harness.page()).unwrap();
    let mut out = Vec::new();
    walk(&page, &mut out);
    out
}

fn two_screen_pc() -> PeerView {
    let mut pc = view("pc", "Desk PC", Os::Windows, LinkState::Connected);
    // A small second screen up and to the left of the main one, as on a laptop with a monitor.
    pc.displays = vec![Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(-1500.0, -900.0, 1500.0, 900.0)];
    pc.ppu = 1.0;
    pc
}

#[test]
fn a_moved_screen_saves_on_its_own_and_undo_puts_it_back() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(Box::new(status(vec![view("pc", "Desk PC", Os::Windows, LinkState::Connected)]))));
    harness.click("Add");
    harness.click("Undo");
    assert_eq!(desk(&harness).len(), 1, "Undo takes the PC off again");
    harness.click("Add");
    let before = desk(&harness);
    let view = View::fit(&harness.model().desk);
    let pc = harness.model().desk[1].computer().rect();
    let grab = view.to_canvas([pc.x + pc.w / 2.0, pc.y + pc.h / 2.0]);
    let target = view.to_canvas([pc.x + pc.w / 2.0, pc.y + pc.h / 2.0 + 400.0]);
    harness.pointer(0, Pointer::at(PointerKind::Press, grab[0], grab[1]));
    harness.pointer(0, Pointer::at(PointerKind::Drag, target[0], target[1]));
    assert!(harness.shows("Lines show where edges line up"));
    let lines = shapes(&mut harness).into_iter().filter(|s| s["type"] == "line").count();
    assert!(lines >= 1, "a guide where the PC meets the Mac");
    harness.pointer(0, Pointer::at(PointerKind::Release, target[0], target[1]));
    let after = desk(&harness);
    assert_ne!(after[1].at, before[1].at, "saved as soon as it's dropped");
    assert!(harness.shows("Saved"));
    harness.click("Undo");
    assert_eq!(desk(&harness)[1].at, before[1].at);
}

#[test]
fn a_name_sits_on_its_main_screen_not_between_screens() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(Box::new(status(vec![two_screen_pc()]))));
    harness.click("Add");
    let view = View::fit(&harness.model().desk);
    let pc = harness.model().desk[1].computer();
    let main = pc.desk_displays()[0];
    let [left, top] = view.to_canvas([main.x, main.y]);
    let [right, bottom] = view.to_canvas([main.right(), main.bottom()]);
    let label = shapes(&mut harness).into_iter().find(|s| s["text"] == "Desk PC").expect("the PC's name is drawn");
    let [x, y] = [label["at"][0].as_f64().unwrap() as f32, label["at"][1].as_f64().unwrap() as f32];
    assert!(x > left && x < right && y > top && y < bottom, "{x},{y} in {left},{top} to {right},{bottom}");
}

#[test]
fn right_clicking_one_of_several_screens_picks_that_one() {
    let mut harness = Harness::<Tool>::start();
    let mut update = status(vec![two_screen_pc()]);
    update.ability = Ability::Ready;
    harness.send(Msg::Status(Box::new(update)));
    harness.click("Add");
    assert!(harness.shows("Right-click one of its screens to use it as a display"));
    assert!(!harness.shows("Turn into extended display"));
    let view = View::fit(&harness.model().desk);
    let second = harness.model().desk[1].computer().desk_displays()[1];
    let [x, y] = view.to_canvas([second.x + second.w / 2.0, second.y + second.h / 2.0]);
    harness.right_click(0, x, y);
    assert_eq!(harness.model().menu, Some(("pc".to_owned(), 1)));
    assert!(harness.shows("Desk PC's screen 2"));
    assert!(!harness.shows("Desk PC's screen 1"));
    harness.click("Turn into extended display");
}

#[test]
fn a_screen_coming_or_going_leaves_the_others_where_they_were() {
    let mut harness = Harness::<Tool>::start();
    harness.send(Msg::Status(Box::new(status(vec![two_screen_pc()]))));
    harness.click("Add");
    let main_before = harness.model().desk[1].computer().desk_displays()[0];
    // The monitor is unplugged: only the main screen is left.
    let mut pc = two_screen_pc();
    pc.displays.truncate(1);
    harness.send(Msg::Status(Box::new(status(vec![pc]))));
    let spot = &desk(&harness)[1];
    assert_eq!(spot.displays.len(), 1);
    assert_eq!(spot.computer().desk_displays()[0], main_before, "the main screen didn't move");
    // And plugged back in.
    harness.send(Msg::Status(Box::new(status(vec![two_screen_pc()]))));
    assert_eq!(desk(&harness)[1].computer().desk_displays()[0], main_before);
}

/// Writes the desk's shapes to ROAM_PREVIEW, for looking at the editor while working on it.
#[test]
#[ignore = "a preview, not a check"]
fn preview_the_desk() {
    let Ok(path) = std::env::var("ROAM_PREVIEW") else { return };
    let mut harness = Harness::<Tool>::start();
    let mut mac = view("mac", "John's MacBook Air", Os::Mac, LinkState::Connected);
    mac.displays = vec![Rect::new(0.0, 0.0, 1470.0, 956.0), Rect::new(-225.0, -1080.0, 1920.0, 1080.0)];
    mac.ppu = 1.0;
    let mut me = view("me", "DESKTOP-FTP7GKN", Os::Windows, LinkState::None);
    me.displays = vec![Rect::new(0.0, 0.0, 1920.0, 1200.0), Rect::new(1920.0, -300.0, 2560.0, 1440.0)];
    me.ppu = 1.25;
    let mut update = Status { me: Some(me), peers: vec![mac], ability: Ability::Ready, ..Status::default() };
    harness.send(Msg::Status(Box::new(update.clone())));
    harness.click("Add");
    update.away_on = Some("John's MacBook Air".into());
    harness.send(Msg::Status(Box::new(update)));
    let view = View::fit(&harness.model().desk);
    let mac = harness.model().desk[1].computer().desk_displays()[0];
    let grab = view.to_canvas([mac.x + 10.0, mac.y + 10.0]);
    harness.pointer(0, Pointer::at(PointerKind::Press, grab[0], grab[1]));
    harness.pointer(0, Pointer::at(PointerKind::Drag, grab[0] - 60.0, grab[1] + 5.0));
    let dragging = shapes(&mut harness);
    harness.pointer(0, Pointer::at(PointerKind::Release, grab[0] - 60.0, grab[1] + 5.0));
    let resting = shapes(&mut harness);
    let texts = harness.texts();
    let out = serde_json::json!({ "dragging": dragging, "resting": resting, "texts": texts });
    std::fs::write(path, out.to_string()).unwrap();
}
