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

/// Waits out the moment after a crossing when the cursor stays put whatever the mouse does.
fn settle() {
    std::thread::sleep(Duration::from_millis(super::SETTLE_AFTER_CROSSING / super::MS + 30));
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
    settle();
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [-100.0, 0.0] });
    a.wait_for("coming home", |s| !s.captured);
    let warped = *a.fake.state().warped.last().unwrap();
    assert_eq!(warped[0], 1509.0);
    b.wait_for("letting go", |s| s.played.last() == Some(&Play::Key { hid: 5, down: false }));
}

#[test]
fn a_jolt_right_after_crossing_does_not_bounce_the_cursor_back() {
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
    a.wait_for("capturing", |s| s.captured);

    // The very next move jumps back past the edge, as a parked cursor's first move can: it
    // stays over on the PC.
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [-200.0, 0.0] });
    std::thread::sleep(Duration::from_millis(50));
    assert!(a.fake.state().captured, "the cursor stayed on the PC");

    // Once it has settled, going back works as usual.
    settle();
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [-200.0, 0.0] });
    a.wait_for("coming home", |s| !s.captured);
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
fn a_session_lives_through_packets_going_astray_one_way() {
    let (mut a, mut b) = pair();
    let (a_id, b_id) = (a.id(), b.id());
    connect(&mut a, &mut b);
    // The PC's packets straight to the Mac go missing for longer than a silent session lasts,
    // as after the PC hops to the router's other band; broadcasts still arrive.
    b.engine.send(Control::LoseDirect(true));
    std::thread::sleep(Duration::from_millis(super::DEAD / super::MS + 2_000));
    assert!(!log_of(&a).contains("session over"), "{}", log_of(&a));
    assert!(!log_of(&b).contains("session over"), "{}", log_of(&b));
    assert!(log_of(&b).contains("go astray"), "{}", log_of(&b));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
}

#[test]
fn a_session_forms_while_packets_go_astray_one_way() {
    let (mut a, mut b) = pair();
    let (a_id, b_id) = (a.id(), b.id());
    connect(&mut a, &mut b);
    b.engine.send(Control::LoseDirect(true));
    // The other one restarts Roam meanwhile: the handshake has to get through as well.
    let Machine { engine, fake, port, data, .. } = a;
    drop(engine);
    std::thread::sleep(Duration::from_millis(300));
    let mut a = start(data, fake, port, b.port);
    place(&mut a, &mut b);
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
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

#[test]
fn a_mac_swipe_is_the_same_shortcut_on_windows() {
    use crate::keys::{D, LEFT_CTRL, LEFT_GUI, RIGHT, TAB};
    let up = super::swipe_keys(Swipe::Up, Os::Windows);
    assert_eq!(up, [(LEFT_GUI, true), (TAB, true), (TAB, false), (LEFT_GUI, false)]);
    assert_eq!(super::swipe_keys(Swipe::Down, Os::Windows)[1], (D, true));
    let right = super::swipe_keys(Swipe::Right, Os::Windows);
    assert_eq!(right[..3], [(LEFT_CTRL, true), (LEFT_GUI, true), (RIGHT, true)]);
    assert_eq!(super::swipe_keys(Swipe::Up, Os::Mac)[0], (LEFT_CTRL, true));
}

/// Pairs the two and connects them, the Mac controlling the PC.
fn connect(a: &mut Machine, b: &mut Machine) {
    let (a_id, b_id) = (a.id(), b.id());
    place(a, b);
    a.until("the code to check", |s| !s.asks.is_empty());
    b.until("the request to allow", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(a, &b_id);
    connected(b, &a_id);
    a.wait_for("listening to the mouse", |s| s.sink.is_some());
}

#[test]
fn files_dragged_across_the_edge_are_picked_up_on_the_other_side_and_can_come_back() {
    let (mut a, mut b) = pair();
    connect(&mut a, &mut b);
    let there = a.data.join("report.pdf");
    std::fs::write(&there, b"the report").unwrap();

    // Held against the edge with no files dragged, the cursor stays home.
    a.fake.input(Input::Button { button: 0, down: true });
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] });
    std::thread::sleep(Duration::from_millis(100));
    assert!(!a.fake.state().captured);

    // Dragging files, it goes across: the drag here ends, and the PC picks them up.
    a.fake.state().dragging = Some(vec![there]);
    std::thread::sleep(Duration::from_millis(80));
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] });
    a.wait_for("capturing", |s| s.captured);
    assert_eq!(a.fake.state().cancelled, 1);
    b.wait_for("picking the files up", |s| !s.picked.is_empty());
    let (paths, played) = b.fake.state().picked[0].clone();
    assert!(played);
    assert_eq!(paths.len(), 1);
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"the report");
    assert_eq!(paths[0].file_name().unwrap(), "report.pdf");

    // Letting go there drops them.
    a.fake.input(Input::Button { button: 0, down: false });
    b.wait_for("the drop", |s| s.played.contains(&Play::Button { button: 0, down: false }));

    // A drag on the PC comes back home once the PC says files are being dragged.
    let back = b.data.join("notes.txt");
    std::fs::write(&back, b"some notes").unwrap();
    b.fake.state().dragging = Some(vec![back]);
    a.fake.input(Input::Button { button: 0, down: true });
    b.wait_for("the press", |s| s.played.iter().filter(|p| **p == Play::Button { button: 0, down: true }).count() == 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.fake.state().captured {
        assert!(Instant::now() < deadline, "the cursor never came home");
        a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [-4000.0, 0.0] });
        std::thread::sleep(Duration::from_millis(30));
    }
    b.wait_for("the drag there ending", |s| s.cancelled == 1);
    a.wait_for("picking the files up at home", |s| !s.picked.is_empty());
    let (paths, played) = a.fake.state().picked[0].clone();
    assert!(!played, "held on this computer's own mouse");
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"some notes");
}

#[test]
fn a_button_let_go_where_roam_could_not_hear_it_stops_holding_the_cursor_home() {
    let (mut a, mut b) = pair();
    connect(&mut a, &mut b);
    // The right button went down, and its release went to an app Roam can't hear.
    a.fake.input(Input::Button { button: 1, down: true });
    a.fake.state().held = Some(0b10);
    std::thread::sleep(Duration::from_millis(700));
    a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] });
    std::thread::sleep(Duration::from_millis(100));
    assert!(!a.fake.state().captured, "still held: stays home");
    a.fake.state().held = Some(0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !a.fake.state().captured {
        assert!(Instant::now() < deadline, "the cursor never crossed");
        a.fake.input(Input::Move { at: [1511.0, 480.0], delta: [3.0, 0.0] });
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(a.data.join("roam.log")).unwrap();
    assert!(log.contains("but a mouse button is held (buttons 00010)"), "{log}");
    assert!(log.contains("were let go without Roam hearing it"), "{log}");
}

/// Virtual displays and lent screens that only pretend: a host's display is a rectangle added to
/// its pretend screens, sending numbered pictures; a lent screen keeps what it's shown.
mod displays {
    use std::sync::Mutex;

    use super::*;
    use crate::display::{Feed, Hosted, Panel, Picture, Remote, RemoteSink, Showing, Want};

    #[derive(Default)]
    pub struct Seen {
        pub wants: Vec<Want>,
        pub shown: Vec<(Rect, [u32; 2])>,
        pub pictures: Vec<Picture>,
        pub gone: bool,
        pub remote: Option<RemoteSink>,
        pub pointers: Vec<crate::display::PointerAt>,
    }

    #[derive(Clone)]
    pub struct Pretend {
        pub fake: Fake,
        pub seen: Arc<Mutex<Seen>>,
        /// Puts a virtual display where Windows would: touching another screen.
        pub snap: bool,
    }

    impl Pretend {
        pub fn new(fake: Fake) -> Self {
            Self { fake, seen: Arc::default(), snap: false }
        }
    }

    struct PretendHosted {
        fake: Fake,
        rect: Rect,
        feed: Feed,
    }

    impl Hosted for PretendHosted {
        fn rect(&mut self) -> Option<Rect> {
            Some(self.rect)
        }

        fn place(&mut self, at: [i32; 2]) {
            let mut state = self.fake.state();
            state.screens.retain(|s| *s != self.rect);
            self.rect.x = f64::from(at[0]);
            self.rect.y = f64::from(at[1]);
            state.screens.push(self.rect);
        }

        fn size(&self) -> [u32; 2] {
            [self.rect.w as u32, self.rect.h as u32]
        }
    }

    impl Drop for PretendHosted {
        fn drop(&mut self) {
            self.feed.stop();
            self.fake.state().screens.retain(|s| *s != self.rect);
        }
    }

    struct PretendShowing(Arc<Mutex<Seen>>);

    impl Showing for PretendShowing {
        fn picture(&mut self, picture: &Picture) -> Result<(), String> {
            self.0.lock().unwrap().pictures.push(picture.clone());
            Ok(())
        }

        fn pointer(&mut self, at: crate::display::PointerAt) {
            self.0.lock().unwrap().pointers.push(at);
        }
    }

    impl Drop for PretendShowing {
        fn drop(&mut self) {
            self.0.lock().unwrap().gone = true;
        }
    }

    impl Displays for Pretend {
        fn ability(&self) -> Ability {
            Ability::Ready
        }

        fn lends(&self) -> bool {
            true
        }

        fn draws_pointer(&self) -> bool {
            true
        }

        fn host(&self, want: &Want, feed: Feed) -> Result<Box<dyn Hosted>, String> {
            self.seen.lock().unwrap().wants.push(want.clone());
            let rect = Rect::new(
                f64::from(want.at[0]),
                f64::from(want.at[1]),
                f64::from(want.panel.pixels[0]),
                f64::from(want.panel.pixels[1]),
            );
            let rect = if self.snap { snap_like_windows(rect, &self.fake.state().screens) } else { rect };
            self.fake.state().screens.push(rect);
            if want.pointer_apart {
                let size = [rect.w as u32, rect.h as u32];
                feed.set_cursor(crate::display::Cursor { hotspot: [2, 3], visible: true, size });
            }
            let sending = feed.clone();
            let apart = want.pointer_apart;
            std::thread::spawn(move || {
                let mut n = 0u8;
                while !sending.stopped() {
                    // As a real capture does, it says where the pointer is as it moves.
                    if apart {
                        let at = crate::display::PointerAt { at: [i32::from(n), 7], visible: true };
                        sending.pointer(Some(at), None);
                    }
                    if !sending.busy() {
                        let key = sending.key_wanted();
                        sending.push(Picture { key, data: vec![0, 0, 0, 1, if key { 0x65 } else { 0x41 }, n] });
                        n = n.wrapping_add(1);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            Ok(Box::new(PretendHosted { fake: self.fake.clone(), rect, feed }))
        }

        fn panel(&self, rect: Rect) -> Option<Panel> {
            // A Full HD monitor shows its points as pixels; smaller ones here are Retina.
            if rect.w >= 1920.0 {
                return Some(Panel { pixels: [rect.w as u32, rect.h as u32], hz: 75, scale: 1.0 });
            }
            Some(Panel { pixels: [rect.w as u32 * 2, rect.h as u32 * 2], hz: 60, scale: 2.0 })
        }

        fn show(&self, rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Box<dyn Showing>, String> {
            let mut seen = self.seen.lock().unwrap();
            seen.shown.push((rect, size));
            seen.remote = Some(remote);
            Ok(Box::new(PretendShowing(self.seen.clone())))
        }
    }

    /// Windows keeps every screen touching another: one placed apart moves the least way that
    /// makes it touch one.
    fn snap_like_windows(rect: Rect, screens: &[Rect]) -> Rect {
        let touches = |r: &Rect| {
            screens.iter().any(|s| {
                let beside = (r.right() == s.x || r.x == s.right()) && r.y < s.bottom() && s.y < r.bottom();
                let over = (r.bottom() == s.y || r.y == s.bottom()) && r.x < s.right() && s.x < r.right();
                beside || over
            })
        };
        if touches(&rect) {
            return rect;
        }
        let mut best: Option<(Rect, f64)> = None;
        for s in screens {
            let x = rect.x.clamp(s.x - rect.w + 1.0, s.right() - 1.0);
            let y = rect.y.clamp(s.y - rect.h + 1.0, s.bottom() - 1.0);
            let spots = [[x, s.y - rect.h], [x, s.bottom()], [s.x - rect.w, y], [s.right(), y]];
            for [sx, sy] in spots {
                let spot = Rect::new(sx, sy, rect.w, rect.h);
                if screens.iter().any(|o| o.overlaps(&spot)) {
                    continue;
                }
                let far = (sx - rect.x).abs() + (sy - rect.y).abs();
                if best.is_none_or(|(_, nearest)| far < nearest) {
                    best = Some((spot, far));
                }
            }
        }
        best.map_or(rect, |(spot, _)| spot)
    }

    pub fn start(data: PathBuf, fake: Fake, port: u16, other: u16) -> (Machine, Pretend) {
        start_snapping(data, fake, port, other, false)
    }

    pub fn start_snapping(data: PathBuf, fake: Fake, port: u16, other: u16, snap: bool) -> (Machine, Pretend) {
        let pretend = Pretend { snap, ..Pretend::new(fake.clone()) };
        let (engine, status) = Engine::start_with(
            data.clone(),
            Box::new(fake.clone()),
            Arc::new(pretend.clone()),
            prefs(other),
            Vec::new(),
            port,
        );
        (Machine { engine, status, fake, last: Status::default(), port, data }, pretend)
    }

    pub fn wait(what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[allow(dead_code)]
    pub fn remote(seen: &Arc<Mutex<Seen>>, remote: Remote) {
        if let Some(sink) = &seen.lock().unwrap().remote {
            sink(remote);
        }
    }
}

#[test]
fn a_screen_lent_as_an_extended_display_takes_its_place_on_the_desk() {
    use crate::display::Remote;

    let a_port = NEXT_PORT.fetch_add(2, Ordering::Relaxed);
    let b_port = a_port + 1;
    // The PC hosts; the Mac lends the second of its screens, an external one to its right.
    let pc = Fake::new(vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)], 1.0);
    let mac = Fake::new(vec![Rect::new(0.0, 0.0, 1512.0, 982.0), Rect::new(1512.0, 0.0, 1280.0, 720.0)], 1.0);
    let (mut a, host) = displays::start(folder(), pc, a_port, b_port);
    let (mut b, viewer) = displays::start(folder(), mac, b_port, a_port);
    let (a_id, b_id) = (a.id(), b.id());
    a.until("seeing the Mac", |s| s.peers.iter().any(|p| p.id == b_id && p.displays.len() == 2 && p.lends));
    // The Mac sits to the right of the PC, its external screen further right.
    a.engine.send(Control::Desk(vec![
        Placement { id: a_id.clone(), at: [0.0, 0.0], scroll: 1.0 },
        Placement { id: b_id.clone(), at: [1920.0, 0.0], scroll: 1.0 },
    ]));
    a.until("the code to check", |s| !s.asks.is_empty());
    b.until("the request to allow", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);

    a.engine.send(Control::Extend { id: b_id.clone(), display: 1 });
    a.until("the extended display up", |s| s.extended.iter().any(|e| e.id == b_id && e.running));
    b.until("lending the screen", |s| s.lending.as_ref().is_some_and(|l| l.display == 1));
    {
        let wants = &host.seen.lock().unwrap().wants;
        assert_eq!(wants.len(), 1);
        assert_eq!(wants[0].panel.pixels, [2560, 1440], "the lent screen's own pixels");
        assert_eq!(wants[0].at, [1920 + 1512, 0], "where that screen sits on the desk");
    }
    displays::wait("pictures shown on the Mac", || viewer.seen.lock().unwrap().pictures.len() >= 3);
    {
        let seen = viewer.seen.lock().unwrap();
        assert_eq!(seen.shown, vec![(Rect::new(1512.0, 0.0, 1280.0, 720.0), [2560, 1440])]);
        assert!(seen.pictures[0].key, "it starts from a key picture");
    }

    // The PC's own screens, as other computers see them, leave the virtual one out.
    a.until("the PC's screens without the virtual one", |s| {
        s.me.as_ref().is_some_and(|m| m.displays == vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)])
    });

    // The Mac's built-in screen stays where it is on the desk, between the PC's own screen and
    // the virtual display: pushing right from the PC's screen still reaches it, even though the
    // PC's screens now reach past it.
    a.wait_for("listening", |s| s.sink.is_some());
    assert!(a.fake.input(Input::Move { at: [1919.0, 500.0], delta: [5.0, 0.0] }));
    a.wait_for("the cursor over on the Mac's built-in screen", |s| s.captured);
    b.wait_for("the cursor arriving on the Mac's built-in screen", |s| {
        s.played.iter().any(|p| matches!(p, Play::Move([x, _]) if *x < 1512.0))
    });
    // And straight back the way it came.
    settle();
    a.fake.input(Input::Move { at: [1919.0, 500.0], delta: [-10.0, 0.0] });
    a.wait_for("coming home", |s| !s.captured);

    // The Mac's own mouse over the lent screen works the PC's cursor there.
    displays::remote(&viewer.seen, Remote::Move([0.5, 0.5]));
    a.wait_for("the cursor put on the virtual display", |s| s.warped.contains(&[3432.0 + 1280.0, 720.0]));

    // Disconnecting gives the screen back.
    a.engine.send(Control::Unextend(b_id.clone()));
    a.until("the extended display gone", |s| s.extended.is_empty());
    b.until("no longer lending", |s| s.lending.is_none());
    displays::wait("the picture closed on the Mac", || viewer.seen.lock().unwrap().gone);
    assert!(!a.fake.state().screens.iter().any(|s| s.x > 3000.0), "the virtual display is gone");
}

/// The PC hosts the Mac's second screen as an extended display, pictures flowing.
fn lend_a_screen() -> (Machine, Machine, displays::Pretend, displays::Pretend, String) {
    let a_port = NEXT_PORT.fetch_add(2, Ordering::Relaxed);
    let b_port = a_port + 1;
    let pc = Fake::new(vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)], 1.0);
    let mac = Fake::new(vec![Rect::new(0.0, 0.0, 1512.0, 982.0), Rect::new(1512.0, 0.0, 1280.0, 720.0)], 1.0);
    let (mut a, host) = displays::start(folder(), pc, a_port, b_port);
    let (mut b, viewer) = displays::start(folder(), mac, b_port, a_port);
    let (a_id, b_id) = (a.id(), b.id());
    a.until("seeing the Mac", |s| s.peers.iter().any(|p| p.id == b_id && p.displays.len() == 2 && p.lends));
    a.engine.send(Control::Desk(vec![
        Placement { id: a_id.clone(), at: [0.0, 0.0], scroll: 1.0 },
        Placement { id: b_id.clone(), at: [1920.0, 0.0], scroll: 1.0 },
    ]));
    a.until("the code to check", |s| !s.asks.is_empty());
    b.until("the request to allow", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
    a.engine.send(Control::Extend { id: b_id.clone(), display: 1 });
    a.until("the extended display up", |s| s.extended.iter().any(|e| e.id == b_id && e.running));
    displays::wait("pictures shown on the Mac", || viewer.seen.lock().unwrap().pictures.len() >= 3);
    (a, b, host, viewer, b_id)
}

/// Whether the PC still has its virtual display, and the Mac its one window.
fn still_extended(a: &Machine, host: &displays::Pretend, viewer: &displays::Pretend) {
    assert_eq!(host.seen.lock().unwrap().wants.len(), 1, "the virtual display was never made again");
    assert!(a.fake.state().screens.iter().any(|s| s.x > 3000.0), "the virtual display is still there");
    let seen = viewer.seen.lock().unwrap();
    assert_eq!(seen.shown.len(), 1, "the same window throughout");
    assert!(!seen.gone, "the window stayed up");
}

fn log_of(machine: &Machine) -> String {
    std::fs::read_to_string(machine.data.join("roam.log")).unwrap_or_default()
}

#[test]
fn a_dropped_picture_stream_comes_back_with_the_display_kept() {
    let (mut a, b, host, viewer, b_id) = lend_a_screen();
    // Long enough for the host to know the stream got through.
    std::thread::sleep(Duration::from_secs(3));
    b.engine.send(Control::CutStream);
    displays::wait("the host noticing", || log_of(&a).contains("picture stream dropped"));
    let before = viewer.seen.lock().unwrap().pictures.len();
    displays::wait("pictures again", || viewer.seen.lock().unwrap().pictures.len() > before + 5);
    still_extended(&a, &host, &viewer);
    a.until("running again", |s| s.extended.iter().any(|e| e.id == b_id && e.running));
    assert!(log_of(&b).contains("waiting for it again"), "{}", log_of(&b));
}

#[test]
fn an_extended_display_waits_for_a_session_lost_to_silence() {
    let (mut a, mut b, host, viewer, b_id) = lend_a_screen();
    // Neither hears the other for longer than a session lasts in silence.
    a.engine.send(Control::Deaf(true));
    b.engine.send(Control::Deaf(true));
    std::thread::sleep(Duration::from_millis(super::DEAD / super::MS + 1_500));
    assert!(log_of(&a).contains("session over"), "{}", log_of(&a));
    assert!(log_of(&a).contains("stays up while"), "{}", log_of(&a));
    assert!(log_of(&b).contains("stays up while"), "{}", log_of(&b));
    still_extended(&a, &host, &viewer);
    a.until("waiting for the Mac", |s| {
        s.extended.iter().any(|e| e.id == b_id && e.working.as_deref().is_some_and(|w| w.contains("come back")))
    });
    // The network comes back: the session forms again by itself, and the display carries on.
    a.engine.send(Control::Deaf(false));
    b.engine.send(Control::Deaf(false));
    connected(&mut a, &b_id);
    displays::wait("the PC asking again", || log_of(&a).contains("again: the extended display carries on"));
    assert!(log_of(&b).contains("again: the lent screen carries on"), "{}", log_of(&b));
    let before = viewer.seen.lock().unwrap().pictures.len();
    displays::wait("pictures still coming", || viewer.seen.lock().unwrap().pictures.len() > before + 5);
    still_extended(&a, &host, &viewer);
    b.until("still lending", |s| s.lending.as_ref().is_some_and(|l| l.display == 1));
}

#[test]
fn an_extended_display_waits_when_only_one_side_went_deaf() {
    let (mut a, mut b, host, viewer, b_id) = lend_a_screen();
    let a_id = a.id();
    // Only the Mac stops hearing: it ends the session, while the PC still hears it until then.
    b.engine.send(Control::Deaf(true));
    std::thread::sleep(Duration::from_millis(super::DEAD / super::MS + 1_500));
    assert!(log_of(&b).contains("session over"), "{}", log_of(&b));
    assert!(log_of(&b).contains("stays up while"), "{}", log_of(&b));
    assert!(!log_of(&a).contains("said goodbye"), "the PC must not take the silence for a goodbye: {}", log_of(&a));
    b.engine.send(Control::Deaf(false));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
    displays::wait("the PC asking again", || log_of(&a).contains("again: the extended display carries on"));
    assert!(log_of(&b).contains("again: the lent screen carries on"), "{}", log_of(&b));
    let before = viewer.seen.lock().unwrap().pictures.len();
    displays::wait("pictures still coming", || viewer.seen.lock().unwrap().pictures.len() > before + 5);
    still_extended(&a, &host, &viewer);
}

#[test]
fn moves_the_lent_window_hears_off_the_lent_screen_leave_the_cursor_where_it_is() {
    use crate::display::Remote;

    let (a, b, _host, viewer, _b_id) = lend_a_screen();
    a.wait_for("listening", |s| s.sink.is_some());
    assert!(a.fake.input(Input::Move { at: [1919.0, 500.0], delta: [5.0, 0.0] }));
    a.wait_for("the cursor over on the Mac's built-in screen", |s| s.captured);
    settle();
    for _ in 0..20 {
        a.fake.input(Input::Move { at: [960.0, 540.0], delta: [20.0, 3.0] });
        // The lent window, key now, hears the moves played on the built-in screen beside it.
        displays::remote(&viewer.seen, Remote::Move([-0.4, 0.6]));
        std::thread::sleep(Duration::from_millis(4));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(a.fake.state().captured, "still over on the Mac");
    let last = b.fake.state().played.iter().rev().find_map(|p| if let Play::Move(at) = p { Some(*at) } else { None });
    let last = last.unwrap();
    assert!(last[0] > 300.0 && last[0] < 600.0, "the cursor went on toward the middle: {last:?}");
}

#[test]
fn the_pointer_over_an_extended_display_goes_the_quick_way() {
    let (a, _b, _host, viewer, _b_id) = lend_a_screen();
    a.wait_for("listening", |s| s.sink.is_some());
    // The virtual display sits at (3432, 0), 2560 by 1440.
    for x in [100.0, 140.0, 180.0] {
        a.fake.input(Input::Move { at: [3432.0 + x, 50.0], delta: [1.0, 0.0] });
        std::thread::sleep(Duration::from_millis(10));
    }
    displays::wait("the pointer drawn on the Mac", || {
        viewer.seen.lock().unwrap().pointers.contains(&crate::display::PointerAt { at: [178, 47], visible: true })
    });
    // Back on the PC's own screen: it hides on the lent one.
    a.fake.input(Input::Move { at: [900.0, 500.0], delta: [-1.0, 0.0] });
    displays::wait("the pointer hidden on the Mac", || {
        viewer.seen.lock().unwrap().pointers.last()
            == Some(&crate::display::PointerAt { at: [178, 47], visible: false })
    });
}

#[test]
fn with_roaming_off_the_cursor_stays_but_the_extended_display_works() {
    let (a, _b, _host, viewer, _b_id) = lend_a_screen();
    a.wait_for("listening", |s| s.sink.is_some());
    let mut off = prefs(a.port + 1);
    off.roam = false;
    a.engine.send(Control::Prefs(off));
    std::thread::sleep(Duration::from_millis(50));
    for _ in 0..5 {
        a.fake.input(Input::Move { at: [1919.0, 500.0], delta: [5.0, 0.0] });
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert!(!a.fake.state().captured, "the cursor stayed on the PC");
    // Its own virtual display still shows the pointer on the Mac.
    a.fake.input(Input::Move { at: [3432.0 + 300.0, 200.0], delta: [1.0, 0.0] });
    displays::wait("the pointer drawn on the Mac", || {
        viewer.seen.lock().unwrap().pointers.contains(&crate::display::PointerAt { at: [298, 197], visible: true })
    });
}

#[test]
fn an_extended_displays_pictures_go_over_udp() {
    let (a, _b, _host, viewer, _) = lend_a_screen();
    displays::wait("the pictures going over UDP", || log_of(&a).contains("pictures go over UDP"));
    let before = viewer.seen.lock().unwrap().pictures.len();
    displays::wait("pictures coming that way", || viewer.seen.lock().unwrap().pictures.len() > before + 20);
    let seen = viewer.seen.lock().unwrap();
    assert!(seen.pictures.iter().all(|p| !p.data.is_empty()), "the host's tries at the way aren't shown");
    // One to start from, and one more to start the new way from.
    assert!(seen.pictures.iter().filter(|p| p.key).count() >= 2, "a key picture to start the new way from");
    assert!(!log_of(&a).contains("go over TCP"), "{}", log_of(&a));
}

#[test]
fn dragging_over_an_extended_display_keeps_its_stream() {
    let (a, _b, _host, viewer, _) = lend_a_screen();
    a.wait_for("listening", |s| s.sink.is_some());
    displays::wait("the pictures going over UDP", || log_of(&a).contains("pictures go over UDP"));
    // The cursor keeps moving over the virtual display, at (3432, 0), for longer than a stream
    // may go quiet: as dragging a selection box does.
    let until = std::time::Instant::now() + Duration::from_secs(7);
    let mut x = 100.0;
    while std::time::Instant::now() < until {
        a.fake.input(Input::Move { at: [3432.0 + x, 50.0], delta: [1.0, 0.0] });
        x = if x > 2000.0 { 100.0 } else { x + 3.0 };
        std::thread::sleep(Duration::from_millis(8));
    }
    let shown = viewer.seen.lock().unwrap().pictures.len();
    displays::wait("pictures still coming", || viewer.seen.lock().unwrap().pictures.len() > shown + 10);
    assert!(!log_of(&a).contains("picture stream dropped"), "{}", log_of(&a));
}

/// The Mac lends its Full HD monitor, which sits above its built-in screen, to a PC at 150%
/// beside the built-in screen. Windows puts the virtual display against the PC's own screen,
/// not where the monitor is on the desk, and it's smaller there than the monitor is.
fn lend_the_screen_above(pc_on_the_left: bool) -> (Machine, Machine, Rect) {
    let a_port = NEXT_PORT.fetch_add(2, Ordering::Relaxed);
    let b_port = a_port + 1;
    let pc = Fake::new(vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)], 1.5);
    let mac = Fake::new(vec![Rect::new(0.0, 0.0, 1470.0, 956.0), Rect::new(-60.0, -1080.0, 1920.0, 1080.0)], 1.0);
    let (mut a, _host) = displays::start_snapping(folder(), pc, a_port, b_port, true);
    let (mut b, _viewer) = displays::start(folder(), mac, b_port, a_port);
    let (a_id, b_id) = (a.id(), b.id());
    a.until("seeing the Mac", |s| s.peers.iter().any(|p| p.id == b_id && p.displays.len() == 2 && p.lends));
    // The PC's screen is 1280 by 720 points; the built-in screen's top lines up with its top.
    let mac_at = if pc_on_the_left { [1280.0 - 60.0, -1080.0] } else { [-1470.0 - 60.0, -1080.0] };
    a.engine.send(Control::Desk(vec![
        Placement { id: a_id.clone(), at: [0.0, 0.0], scroll: 1.0 },
        Placement { id: b_id.clone(), at: mac_at, scroll: 1.0 },
    ]));
    a.until("the code to check", |s| !s.asks.is_empty());
    b.until("the request to allow", |s| !s.asks.is_empty());
    a.engine.send(Control::Allow(b_id.clone()));
    b.engine.send(Control::Allow(a_id.clone()));
    connected(&mut a, &b_id);
    connected(&mut b, &a_id);
    a.engine.send(Control::Extend { id: b_id.clone(), display: 1 });
    a.until("the extended display up", |s| s.extended.iter().any(|e| e.id == b_id && e.running));
    a.wait_for("listening", |s| s.sink.is_some());
    let virtual_display = a.fake.state().screens.iter().copied().find(|s| s.x != 0.0 || s.y != 0.0).unwrap();
    (a, b, virtual_display)
}

/// Pushes from `at` by `push`, then keeps going: whether the cursor went over and stayed.
fn crosses_and_stays(a: &Machine, at: [f64; 2], push: [f64; 2]) -> bool {
    a.fake.input(Input::Move { at, delta: push });
    std::thread::sleep(Duration::from_millis(60));
    if !a.fake.state().captured {
        return false;
    }
    settle();
    for _ in 0..10 {
        a.fake.input(Input::Move { at: [960.0, 540.0], delta: push });
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(50));
    let stayed = a.fake.state().captured;
    a.engine.send(Control::Home);
    std::thread::sleep(Duration::from_millis(60));
    stayed
}

#[test]
fn the_lending_computers_other_screen_stays_beside_the_pc_while_one_is_lent() {
    // The PC on the left: right from anywhere along its edge that faces the built-in screen
    // (clear of the corner, which a Mac running this keeps for hot corners).
    let (a, _b, virtual_display) = lend_the_screen_above(true);
    assert_eq!(virtual_display, Rect::new(1830.0, -1080.0, 1920.0, 1080.0), "where Windows puts it");
    for y in [100.0, 600.0, 1040.0] {
        assert!(crosses_and_stays(&a, [1919.0, y], [6.0, 0.0]), "right from the PC at {y}\n{}", log_of(&a));
    }
    // And down from the virtual display, as from the monitor above the built-in screen.
    let below = virtual_display.bottom() - 1.0;
    assert!(crosses_and_stays(&a, [virtual_display.x + 300.0, below], [0.0, 6.0]), "{}", log_of(&a));

    // The PC on the right: Windows puts the virtual display above its screen, and the built-in
    // screen is still across from the PC's left edge.
    let (a, _b, virtual_display) = lend_the_screen_above(false);
    assert_eq!(virtual_display, Rect::new(-1919.0, -1080.0, 1920.0, 1080.0), "where Windows puts it");
    for y in [100.0, 600.0, 1040.0] {
        assert!(crosses_and_stays(&a, [0.0, y], [-6.0, 0.0]), "left from the PC at {y}\n{}", log_of(&a));
    }
    let below = virtual_display.bottom() - 1.0;
    assert!(crosses_and_stays(&a, [virtual_display.x + 300.0, below], [0.0, 6.0]), "{}", log_of(&a));
}

#[test]
fn a_move_on_the_lent_screens_far_edge_leaves_the_cursor_on_the_screen_below() {
    use crate::display::Remote;

    let (a, _b, _host, viewer, _b_id) = lend_a_screen();
    a.wait_for("listening", |s| s.sink.is_some());
    assert!(a.fake.input(Input::Move { at: [1919.0, 500.0], delta: [5.0, 0.0] }));
    a.wait_for("the cursor over on the Mac's built-in screen", |s| s.captured);
    settle();
    // The first row of a screen right below the lent one is the lent window's bottom edge.
    displays::remote(&viewer.seen, Remote::Move([0.5, 1.0]));
    displays::remote(&viewer.seen, Remote::Move([1.0, 0.5]));
    std::thread::sleep(Duration::from_millis(150));
    assert!(a.fake.state().captured, "still over on the Mac");
}
