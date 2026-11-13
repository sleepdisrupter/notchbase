//! Two engines talking over the loopback network, each with a pretend mouse, keyboard and screen.

use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::input::fake::Fake;

static NEXT_PORT: AtomicU16 = AtomicU16::new(47_960);

struct Machine {
    engine: Engine,
    status: Receiver<Status>,
    fake: Fake,
    last: Status,
    port: u16,
    data: PathBuf,
}

fn folder() -> PathBuf {
    let path = std::env::temp_dir().join(format!("roam-test-{}", hex(&crypto::random::<6>())));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn prefs(other: u16) -> Prefs {
    Prefs { clipboard: false, addresses: vec![format!("127.0.0.1:{other}")], ..Prefs::default() }
}

fn start(data: PathBuf, fake: Fake, port: u16, other: u16) -> Machine {
    let (engine, status) = Engine::start(data.clone(), Box::new(fake.clone()), prefs(other), Vec::new(), port);
    Machine { engine, status, fake, last: Status::default(), port, data }
}

impl Machine {
    fn until(&mut self, what: &str, done: impl Fn(&Status) -> bool) -> Status {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if done(&self.last) {
                return self.last.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out waiting for {what}; last status: {:#?}", self.last);
            if let Ok(status) = self.status.recv_timeout(left) {
                self.last = status;
            }
        }
    }

    fn id(&mut self) -> String {
        self.until("my id", |s| s.me.is_some()).me.unwrap().id
    }

    fn wait_for(&self, what: &str, done: impl Fn(&crate::input::fake::State) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.fake.state()) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn pair() -> (Machine, Machine) {
    let a_port = NEXT_PORT.fetch_add(2, Ordering::Relaxed);
    let b_port = a_port + 1;
    let mac = Fake::new(vec![Rect::new(0.0, 0.0, 1512.0, 982.0)], 1.0);
    let pc = Fake::new(vec![Rect::new(0.0, 0.0, 2560.0, 1440.0)], 1.5);
    (start(folder(), mac, a_port, b_port), start(folder(), pc, b_port, a_port))
}

fn place(a: &mut Machine, b: &mut Machine) {
    let (a_id, b_id) = (a.id(), b.id());
    a.until("seeing the other computer", |s| s.peers.iter().any(|p| p.id == b_id && p.displays.len() == 1));
    a.engine.send(Control::Desk(vec![
        Placement { id: a_id, at: [0.0, 0.0], scroll: 1.0 },
        Placement { id: b_id, at: [1512.0, 0.0], scroll: 1.0 },
    ]));
}

fn connected(side: &mut Machine, other: &str) {
    let other = other.to_owned();
    side.until("connected", |s| s.peers.iter().any(|p| p.id == other && p.link == LinkState::Connected));
}

#[test]
fn pairing_then_the_cursor_crosses_over_and_back() {
    let (mut a, mut b) = pair();
    let (a_id, b_id) = (a.id(), b.id());
    place(&mut a, &mut b);

    // Both computers show the same code; the person checks it on both.
    let ours = a.until("the code to check", |s| !s.asks.is_empty()).asks[0].clone();
    let theirs = b.until("the request to allow", |s| !s.asks.is_empty()).asks[0].clone();
    assert!(ours.controlling && !theirs.controlling);
    assert_eq!(ours.code, theirs.code);
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
    assert_eq!(b.last.controller.as_deref(), a.last.me.as_ref().map(|m| m.name.as_str()));
    a.wait_for("listening to the mouse", |s| s.sink.is_some());

    // Push against the right edge: the cursor lands on the PC's left edge, at the same height.
    assert!(a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] }));
    a.wait_for("capturing", |s| s.captured);
    b.wait_for("the cursor arriving", |s| !s.played.is_empty());
    let Play::Move([x, y]) = b.fake.state().played[0] else { panic!() };
    assert_eq!(x, 3.0, "2 points in from the edge");
    assert!((y - 720.0).abs() < 0.01, "{y}");

    // 10 points on the Mac is 15 pixels on the PC at 150%.
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [10.0, 0.0] });
    b.wait_for("the cursor moving", |s| {
        s.played.iter().any(|p| matches!(p, Play::Move([x, _]) if (*x - 18.0).abs() < 0.01))
    });

    // Keys and clicks arrive in order.
    a.fake.input(Input::Key { hid: 0x04, down: true });
    a.fake.input(Input::Key { hid: 0x04, down: false });
    a.fake.input(Input::Button { button: 0, down: true });
    a.fake.input(Input::Button { button: 0, down: false });
    b.wait_for("the keys", |s| s.played.iter().filter(|p| !matches!(p, Play::Move(_))).count() == 4);
    let pressed: Vec<Play> = b.fake.state().played.iter().filter(|p| !matches!(p, Play::Move(_))).copied().collect();
    assert_eq!(
        pressed,
        vec![
            Play::Key { hid: 4, down: true },
            Play::Key { hid: 4, down: false },
            Play::Button { button: 0, down: true },
            Play::Button { button: 0, down: false },
        ]
    );

    // A key held when the cursor goes home is let go on the PC.
    a.fake.input(Input::Key { hid: 0x05, down: true });
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [-100.0, 0.0] });
    a.wait_for("coming home", |s| !s.captured);
    let warped = *a.fake.state().warped.last().unwrap();
    assert_eq!(warped[0], 1509.0);
    b.wait_for("letting go", |s| s.played.last() == Some(&Play::Key { hid: 5, down: false }));
}

#[test]
fn the_cursor_picks_up_again_after_a_dropout() {
    let (mut a, mut b) = pair();
    let (a_id, b_id) = (a.id(), b.id());
    place(&mut a, &mut b);
    a.until("the code", |s| !s.asks.is_empty());
    b.until("the request", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
    a.wait_for("listening to the mouse", |s| s.sink.is_some());
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] });
    b.until("the cursor arriving", |s| s.here);

    // The PC stops hearing the Mac for two seconds: it lets go of the cursor...
    b.engine.send(Control::Deaf(true));
    b.until("letting go", |s| !s.here);
    std::thread::sleep(Duration::from_millis(500));
    b.engine.send(Control::Deaf(false));
    // ...but the Mac still hears the PC, so it keeps the cursor there, and the next move
    // brings it back to life on the PC.
    assert!(a.fake.state().captured, "the Mac still has the cursor away");
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [20.0, 0.0] });
    b.until("the cursor back", |s| s.here);
    b.wait_for("the move played", |s| {
        s.played.iter().any(|p| matches!(p, Play::Move([x, _]) if (*x - 33.0).abs() < 0.01))
    });
}

#[test]
fn paired_computers_reconnect_by_themselves() {
    let (mut a, mut b) = pair();
    let (a_id, b_id) = (a.id(), b.id());
    place(&mut a, &mut b);
    a.until("the code", |s| !s.asks.is_empty());
    b.until("the request", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);

    // The PC restarts Roam: it comes back without asking, because both remember each other.
    let Machine { engine, fake, port, data, .. } = b;
    drop(engine);
    std::thread::sleep(Duration::from_millis(300));
    let mut b = start(data, fake, port, a.port);
    assert_eq!(b.id(), b_id, "the same identity");
    connected(&mut b, &a_id);
    connected(&mut a, &b_id);
    assert!(a.last.asks.is_empty() && b.last.asks.is_empty());
}

#[test]
fn declining_stops_the_requests() {
    let (mut a, mut b) = pair();
    let a_id = a.id();
    let b_id = b.id();
    place(&mut a, &mut b);
    b.until("the request", |s| !s.asks.is_empty());
    b.engine.send(Control::Decline(a_id));
    a.until("hearing no", |s| s.peers.iter().any(|p| p.id == b_id && p.link == LinkState::Declined));
    std::thread::sleep(Duration::from_millis(1200));
    assert!(b.status.try_iter().last().is_none_or(|s| s.asks.is_empty()), "no new request");
}

#[test]
fn a_closed_computer_is_not_offered_to() {
    let (mut a, mut b) = pair();
    let b_id = b.id();
    b.engine.send(Control::Prefs(Prefs { open: false, ..prefs(a.port) }));
    place(&mut a, &mut b);
    a.until("seeing it closed", |s| s.peers.iter().any(|p| p.id == b_id && !p.open));
    std::thread::sleep(Duration::from_millis(700));
    assert!(b.status.try_iter().last().is_none_or(|s| s.asks.is_empty()));
}
