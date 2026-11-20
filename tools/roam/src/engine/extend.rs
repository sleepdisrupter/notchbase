//! Screens lent between computers as extended displays.
//!
//! The controlling computer asks a computer it controls for one of its screens (`Lend`). That
//! computer says how big the screen is (`Lent`), and the controller makes a virtual display
//! that size where the screen sits on the desk, then streams it over a connection of its own.
//! Either side ends it with `Unlend`.
//!
//! While a screen is lent, the desk counts it as the host's display: the host's cursor reaches
//! it the way it reaches any of its own displays, and the lending computer's other screens stay
//! where they were for the cursor to cross to.

use std::io::{BufReader, BufWriter, Write as _};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Core, Cursor, Event, Session, Side};
use crate::crypto::{self, hex};
use crate::display::stream::{self, Down, Hello, Keys, Opener, Sealer, Up};
use crate::display::{self, Ability, Displays, Feed, Panel, Remote, Want};
use crate::input::Play;
use crate::layout::{self, Computer, Rect};
use crate::wire::{Id, Message};

/// How long the other computer has to say yes to lending its screen.
const ASK_FOR: u64 = 6 * super::SECOND;
/// How often the host checks where the system put its virtual display.
const CHECK_EVERY: Duration = Duration::from_millis(500);
/// The host's network send buffer: a few pictures' worth, so a busy network shows up as the
/// feed being busy within a frame or two, not as a growing delay.
const SEND_BUFFER: usize = 128 * 1024;
/// How often the host writes how its pictures went to the log.
const TALLY_EVERY: Duration = Duration::from_secs(5);
/// A key picture is asked for at most this often after pictures fail to show.
const KEY_EVERY: Duration = Duration::from_millis(500);

/// One of another computer's screens shown here as an extended display (this computer hosts).
pub(super) struct Extended {
    pub serial: u32,
    pub display: u8,
    pub state: State,
    /// Where the virtual display is, in this computer's units, once it's up.
    pub rect: Option<Rect>,
    pub feed: Option<Feed>,
    pub place: Option<Sender<[i32; 2]>>,
    pub asked_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum State {
    Asking,
    Working(String),
    Running,
    Failed(String),
}

/// This computer's screen, shown as another computer's extended display (this computer lends).
pub(super) struct Lending {
    pub serial: u32,
    pub to: Id,
    pub display: u8,
    pub rect: Rect,
    pub session: u32,
    /// The connection, once it's up: shutting it down ends the showing.
    pub stream: Arc<Mutex<Option<TcpStream>>>,
}

/// Word from the threads that carry the pictures.
pub(super) enum ExtendEvent {
    /// The host's side is getting on: what it's doing, in a few words.
    Working { peer: Id, serial: u32, words: String },
    /// The virtual display is up, here (in this computer's units).
    At { peer: Id, serial: u32, rect: Option<Rect> },
    /// The lending computer's own mouse or keyboard, over the lent screen.
    Remote { peer: Id, serial: u32, remote: Remote },
    /// The host's side is over, with why.
    Ended { peer: Id, serial: u32, why: String },
    /// A stream is coming in for `session`: is a screen lent for it?
    Stream { session: u32, answer: Sender<Option<Grant>> },
    /// The lending side is over.
    LendEnded { serial: u32, why: String },
    /// A line for the log from the host's side.
    Note { text: String },
}

/// What a lending computer needs to show an incoming stream.
pub(super) struct Grant {
    keys: Keys,
    rect: Rect,
    serial: u32,
    displays: Arc<dyn Displays>,
    stream: Arc<Mutex<Option<TcpStream>>>,
}

/// An extended display, for the page.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedView {
    /// The computer whose screen it is.
    pub id: String,
    pub name: String,
    pub display: u8,
    /// What's happening, when it isn't up yet: "Installing the display driver".
    pub working: Option<String>,
    pub running: bool,
    pub failed: Option<String>,
}

/// A screen of this computer lent to another, for the page.
#[derive(Clone, Debug, PartialEq)]
pub struct LendingView {
    pub to: String,
    pub display: u8,
}

impl Core {
    /// This computer's own displays, without the virtual ones it makes for other computers.
    pub(super) fn real_screens(&self) -> (Vec<Rect>, f64) {
        let (displays, ppu) = &self.screens;
        let virtuals: Vec<Rect> = self.extended.values().filter_map(|e| e.rect).collect();
        let real: Vec<Rect> = displays.iter().copied().filter(|d| !virtuals.iter().any(|v| same(d, v))).collect();
        if real.is_empty() { (displays.clone(), *ppu) } else { (real, *ppu) }
    }

    /// This computer on the desk, with the virtual displays it hosts among its own, placed so its
    /// own displays stay where they are.
    pub(super) fn me_on_desk(&self, at: [f64; 2]) -> Computer {
        let (all, ppu) = self.screens.clone();
        let (real, _) = self.real_screens();
        let shift = shift(&real, &all, ppu);
        Computer::new("me", all, ppu, [at[0] + shift[0], at[1] + shift[1]])
    }

    /// A computer on the desk, without the screen it lends here. Its other screens keep their
    /// place beside that screen, now the virtual display on `me`: the virtual display is as many
    /// points as the system makes it, which needn't be the size the lent screen has on the desk.
    /// `None` when it lends every screen it has.
    pub(super) fn peer_on_desk(
        &self,
        id: Id,
        name: String,
        displays: Vec<Rect>,
        ppu: f64,
        at: [f64; 2],
        me: &Computer,
    ) -> Option<Computer> {
        let Some((lent, rect)) = self.extended.get(&id).and_then(|e| Some((usize::from(e.display), e.rect?))) else {
            return Some(Computer::new(name, displays, ppu, at));
        };
        let whole = Computer::new(name.clone(), displays.clone(), ppu, at);
        let desk = whole.desk_displays();
        let lent_on_desk = *desk.get(lent)?;
        let left: Vec<Rect> = displays.iter().enumerate().filter(|(i, _)| *i != lent).map(|(_, d)| *d).collect();
        if left.is_empty() {
            return None;
        }
        let rest =
            layout::bounds(&desk.iter().enumerate().filter(|(i, _)| *i != lent).map(|(_, d)| *d).collect::<Vec<_>>());
        let [vx, vy] = me.to_desk([rect.x, rect.y]);
        let virtual_on_desk = Rect::new(vx, vy, rect.w / me.ppu, rect.h / me.ppu);
        let [dx, dy] = follow(lent_on_desk, rest, virtual_on_desk);
        let moved = Rect::new(rest.x + dx, rest.y + dy, rest.w, rest.h);
        // The system puts the virtual display against this computer's others, which can land
        // the rest where this computer's screens are: then they go beside it instead.
        let beside = rest.right() <= lent_on_desk.x + 0.5 || rest.x >= lent_on_desk.right() - 0.5;
        let clear = clear_of(moved, virtual_on_desk, &me.desk_displays(), beside);
        let shift = shift(&left, &displays, ppu);
        let [cx, cy] = [clear[0] - moved.x, clear[1] - moved.y];
        Some(Computer::new(name, left, ppu, [at[0] + shift[0] + dx + cx, at[1] + shift[1] + dy + cy]))
    }

    /// Where the virtual display for `display` of `peer` goes: its top left where that screen
    /// sits on the desk, in this computer's units.
    fn virtual_at(&self, peer: Id, display: u8) -> Option<[i32; 2]> {
        let id = hex(&peer);
        let placement = self.placements.iter().find(|p| p.id == id)?;
        let theirs = self.peers.get(&peer)?;
        let computer = Computer::new(id, theirs.displays.clone(), theirs.ppu, placement.at);
        let screen = *computer.desk_displays().get(usize::from(display))?;
        let mine = hex(&self.identity.id);
        let my_spot = self.placements.iter().find(|p| p.id == mine).map_or([0.0, 0.0], |p| p.at);
        let (real, ppu) = self.real_screens();
        let [x, y] = Computer::new("me", real, ppu, my_spot).from_desk([screen.x, screen.y]);
        Some([x.round() as i32, y.round() as i32])
    }

    fn next_serial(&mut self) -> u32 {
        self.extend_serial = self.extend_serial.wrapping_add(1);
        self.extend_serial
    }

    // ------------------------------------------------------------ hosting another's screen

    /// The person asked for `display` of `peer` as an extended display of this computer.
    pub(super) fn extend(&mut self, peer: Id, display: u8) {
        self.unextend(peer, "Replaced by another screen");
        let serial = self.next_serial();
        let now = self.now();
        let mut entry =
            Extended { serial, display, state: State::Asking, rect: None, feed: None, place: None, asked_at: now };
        let lends = self.peers.get(&peer).is_some_and(|p| p.lends);
        let live = self.sessions.get(&peer).is_some_and(|s| s.side == Side::Controller && s.is_live());
        let failed = match self.ability.clone() {
            Ability::No(why) => Some(why),
            _ if !live => Some(format!("{} isn't connected", self.who(peer))),
            _ if !lends => Some(format!("Update notchbase on {} to lend its screens", self.who(peer))),
            _ => None,
        };
        match failed {
            Some(why) => entry.state = State::Failed(why),
            None => {
                self.note(&format!("asking {} for its display {display} as an extended display", self.who(peer)));
                if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
                    link.queue(Message::Lend { display });
                }
            }
        }
        self.extended.insert(peer, entry);
        self.flush(now, true);
    }

    /// Ends the extended display of `peer`'s screen, telling it why.
    pub(super) fn unextend(&mut self, peer: Id, why: &str) {
        let Some(entry) = self.extended.remove(&peer) else { return };
        if let Some(feed) = &entry.feed {
            feed.stop();
        }
        if !matches!(entry.state, State::Failed(_)) {
            self.note(&format!("extended display of {} ends: {why}", self.who(peer)));
            if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
                link.queue(Message::Unlend { display: entry.display, why: why.to_owned() });
            }
        }
    }

    /// The other computer said yes: make the display and start sending it.
    pub(super) fn lent(&mut self, peer: Id, display: u8, pixels: [u16; 2], hz: u8, scale: f32) {
        let Some(entry) = self.extended.get(&peer).filter(|e| e.display == display && e.state == State::Asking) else {
            return;
        };
        let serial = entry.serial;
        let panel = Panel {
            pixels: pixels.map(u32::from),
            hz: u32::from(hz).clamp(24, 240),
            scale: if scale.is_finite() { scale.clamp(1.0, 4.0) } else { 1.0 },
        };
        let at = self.virtual_at(peer, display).unwrap_or_default();
        let name = self.who(peer);
        let Some(session) = self.sessions.get_mut(&peer) else { return };
        let addr = session.addr;
        let session_id = session.id;
        let Some(link) = session.live() else { return };
        let keys = stream::keys(link.bulk_send());
        let feed = Feed::new();
        let (place, placed) = mpsc::channel();
        let install = self.ability == Ability::NeedsDriver;
        let job = HostJob {
            peer,
            serial,
            want: Want { name: name.clone(), panel, at },
            addr,
            hello: Hello { session: session_id, stream: u32::from_le_bytes(crypto::random::<4>()) },
            keys,
            feed: feed.clone(),
            placed,
            events: self.events.clone(),
            displays: self.displays.clone(),
            install,
        };
        self.note(&format!(
            "{name} lends its display {display}: {}x{} at {} Hz, scale {}; placing it at ({}, {})",
            panel.pixels[0], panel.pixels[1], panel.hz, panel.scale, at[0], at[1]
        ));
        let started = std::thread::Builder::new().name("roam-host".into()).spawn(move || host(job));
        let Some(entry) = self.extended.get_mut(&peer) else { return };
        if started.is_err() {
            entry.state = State::Failed("Couldn't start the display".into());
            return;
        }
        let words =
            if install { "Installing the display driver. Say Yes when Windows asks" } else { "Starting the display" };
        entry.state = State::Working(words.into());
        entry.feed = Some(feed);
        entry.place = Some(place);
    }

    /// The desk changed: move each virtual display to where its screen now sits.
    pub(super) fn place_extended(&mut self) {
        let wanted: Vec<(Id, u8)> = self.extended.iter().map(|(id, e)| (*id, e.display)).collect();
        for (peer, display) in wanted {
            let Some(at) = self.virtual_at(peer, display) else { continue };
            if let Some(place) = self.extended.get(&peer).and_then(|e| e.place.as_ref()) {
                let _ = place.send(at);
            }
        }
    }

    /// An extended display asked for and never answered gives up.
    pub(super) fn check_extended(&mut self, now: u64) {
        for (id, entry) in &mut self.extended {
            if entry.state == State::Asking && now.saturating_sub(entry.asked_at) > ASK_FOR {
                let name = self.peers.get(id).map_or_else(|| "The other computer".into(), |p| p.name.clone());
                entry.state = State::Failed(format!("{name} didn't answer"));
            }
        }
    }

    // ------------------------------------------------------------ lending this computer's screen

    /// The controller asks for one of this computer's screens.
    pub(super) fn lend(&mut self, to: Id, display: u8) {
        let (real, _) = self.real_screens();
        let rect = real.get(usize::from(display)).copied();
        let panel = rect.and_then(|rect| self.displays.panel(rect));
        let session = self.sessions.get(&to).map(|s| s.id);
        let (Some(rect), Some(panel), Some(session)) = (rect, panel, session) else {
            let why = if self.displays.lends() {
                "That screen isn't there anymore"
            } else {
                "This computer can't lend its screens"
            };
            if let Some(link) = self.sessions.get_mut(&to).and_then(Session::live) {
                link.queue(Message::Unlend { display, why: why.into() });
            }
            return;
        };
        self.stop_lending(None);
        let serial = self.next_serial();
        self.note(&format!(
            "{} asked for display {display} ({}x{} pixels at {} Hz) as its extended display",
            self.who(to),
            panel.pixels[0],
            panel.pixels[1],
            panel.hz
        ));
        self.lending = Some(Lending { serial, to, display, rect, session, stream: Arc::default() });
        let clamp = |side: u32| side.min(u32::from(u16::MAX)) as u16;
        if let Some(link) = self.sessions.get_mut(&to).and_then(Session::live) {
            link.queue(Message::Lent {
                display,
                pixels: panel.pixels.map(clamp),
                hz: panel.hz.min(255) as u8,
                scale: panel.scale,
            });
        }
    }

    /// Stops showing another computer's picture, telling it why when `why` is given.
    pub(super) fn stop_lending(&mut self, why: Option<&str>) {
        let Some(lending) = self.lending.take() else { return };
        if let Some(stream) = lending.stream.lock().ok().and_then(|mut s| s.take()) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(why) = why {
            self.note(&format!("no longer lending display {} to {}: {why}", lending.display, self.who(lending.to)));
            if let Some(link) = self.sessions.get_mut(&lending.to).and_then(Session::live) {
                link.queue(Message::Unlend { display: lending.display, why: why.to_owned() });
            }
        }
    }

    /// A computer in a session with this one stopped: whatever was lent between them ends.
    pub(super) fn session_ended_displays(&mut self, id: Id) {
        if let Some(entry) = self.extended.remove(&id)
            && let Some(feed) = entry.feed
        {
            feed.stop();
        }
        if self.lending.as_ref().is_some_and(|l| l.to == id) {
            self.stop_lending(None);
        }
    }

    pub(super) fn shut_down_displays(&mut self) {
        let hosted: Vec<Id> = self.extended.keys().copied().collect();
        for id in hosted {
            self.unextend(id, "Roam stopped");
        }
        self.stop_lending(Some("Roam stopped"));
    }

    // ------------------------------------------------------------ word from the threads

    pub(super) fn extend_event(&mut self, event: ExtendEvent) {
        match event {
            ExtendEvent::Working { peer, serial, words } => {
                if let Some(entry) = self.extended.get_mut(&peer).filter(|e| e.serial == serial) {
                    entry.state = State::Working(words);
                }
            }
            ExtendEvent::At { peer, serial, rect } => {
                let Some(entry) = self.extended.get_mut(&peer).filter(|e| e.serial == serial) else { return };
                let first = entry.state != State::Running;
                entry.state = State::Running;
                entry.rect = rect;
                let words = rect
                    .map_or_else(|| "somewhere".into(), |r| format!("({:.0}, {:.0}) {:.0}x{:.0}", r.x, r.y, r.w, r.h));
                if first {
                    self.note(&format!("extended display of {} is up at {words}", self.who(peer)));
                }
                // The system lists it among this computer's displays on its next look.
                self.screens_at = 0;
            }
            ExtendEvent::Remote { peer, serial, remote } => {
                let Some(rect) = self.extended.get(&peer).filter(|e| e.serial == serial).and_then(|e| e.rect) else {
                    return;
                };
                self.remote(rect, remote);
            }
            ExtendEvent::Ended { peer, serial, why } => {
                let Some(entry) = self.extended.get(&peer).filter(|e| e.serial == serial) else { return };
                let running = entry.state == State::Running;
                let display = entry.display;
                self.note(&format!("extended display of {} ended: {why}", self.who(peer)));
                if running {
                    self.extended.remove(&peer);
                } else if let Some(entry) = self.extended.get_mut(&peer) {
                    entry.state = State::Failed(why.clone());
                    entry.feed = None;
                    entry.place = None;
                }
                if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
                    link.queue(Message::Unlend { display, why });
                }
                self.screens_at = 0;
            }
            ExtendEvent::Stream { session, answer } => {
                let grant = self.lending.as_ref().filter(|l| l.session == session).and_then(|lending| {
                    let session = self.sessions.get_mut(&lending.to)?;
                    let keys = stream::keys(session.live()?.bulk_receive());
                    Some(Grant {
                        keys,
                        rect: lending.rect,
                        serial: lending.serial,
                        displays: self.displays.clone(),
                        stream: lending.stream.clone(),
                    })
                });
                let _ = answer.send(grant);
            }
            ExtendEvent::LendEnded { serial, why } => {
                if self.lending.as_ref().is_some_and(|l| l.serial == serial) {
                    self.stop_lending(Some(&why));
                }
            }
            ExtendEvent::Note { text } => self.note(&text),
        }
    }

    /// The lending computer's own mouse or keyboard, over its lent screen: it works this
    /// computer's cursor there.
    fn remote(&mut self, rect: Rect, remote: Remote) {
        match remote {
            Remote::Move([x, y]) => {
                let at =
                    [rect.x + f64::from(x.clamp(0.0, 1.0)) * rect.w, rect.y + f64::from(y.clamp(0.0, 1.0)) * rect.h];
                if matches!(self.cursor, Cursor::Away { .. }) {
                    self.go_home(Some(at));
                } else {
                    self.platform.warp(at);
                }
            }
            _ if self.cursor != Cursor::Home => {}
            Remote::Button { button, down } => self.platform.play(Play::Button { button, down }),
            Remote::Wheel { dx, dy } => self.platform.play(Play::Wheel { dx, dy }),
            Remote::Key { hid, down } => self.platform.play(Play::Key { hid, down }),
        }
    }

    // ------------------------------------------------------------ for the page

    pub(super) fn extended_views(&self) -> Vec<ExtendedView> {
        let mut views: Vec<ExtendedView> = self
            .extended
            .iter()
            .map(|(id, entry)| ExtendedView {
                id: hex(id),
                name: self.who(*id),
                display: entry.display,
                working: match &entry.state {
                    State::Asking => Some(format!("Asking {}", self.who(*id))),
                    State::Working(words) => Some(words.clone()),
                    _ => None,
                },
                running: entry.state == State::Running,
                failed: match &entry.state {
                    State::Failed(why) => Some(why.clone()),
                    _ => None,
                },
            })
            .collect();
        views.sort_by(|a, b| a.name.cmp(&b.name));
        views
    }

    pub(super) fn lending_view(&self) -> Option<LendingView> {
        self.lending.as_ref().map(|l| LendingView { to: self.who(l.to), display: l.display })
    }
}

/// Whether two rectangles are the same display, give or take rounding.
fn same(a: &Rect, b: &Rect) -> bool {
    (a.x - b.x).abs() < 1.5 && (a.y - b.y).abs() < 1.5 && (a.w - b.w).abs() < 1.5 && (a.h - b.h).abs() < 1.5
}

/// How far the rest of a computer's screens (`rest`) move when the screen they sit beside
/// (`lent`) becomes `virtual_on_desk`: screens beyond its right or bottom edge stay beyond it,
/// the others keep their place by its top left.
fn follow(lent: Rect, rest: Rect, virtual_on_desk: Rect) -> [f64; 2] {
    let dx =
        if rest.x >= lent.right() - 0.5 { virtual_on_desk.right() - lent.right() } else { virtual_on_desk.x - lent.x };
    let dy = if rest.y >= lent.bottom() - 0.5 {
        virtual_on_desk.bottom() - lent.bottom()
    } else {
        virtual_on_desk.y - lent.y
    };
    [dx, dy]
}

/// Where `rest` (a lending computer's other screens, on the desk) goes so it covers none of
/// the host's screens (`mine`, the virtual display among them): where it is if it covers none,
/// else the nearest spot against a side of the virtual display, so the cursor can cross to it:
/// its left or right if it was `beside` the lent screen, above or below if not.
fn clear_of(rest: Rect, virtual_on_desk: Rect, mine: &[Rect], beside: bool) -> [f64; 2] {
    let free = |x: f64, y: f64| {
        let spot = Rect::new(x + 0.5, y + 0.5, rest.w - 1.0, rest.h - 1.0);
        !mine.iter().any(|d| d.overlaps(&spot))
    };
    if free(rest.x, rest.y) {
        return [rest.x, rest.y];
    }
    let v = virtual_on_desk;
    // Facing the virtual display as fully as it can along the side it goes on.
    let along = |at: f64, size: f64, start: f64, length: f64| {
        if size <= length { at.clamp(start, start + length - size) } else { start }
    };
    let y = along(rest.y, rest.h, v.y, v.h);
    let x = along(rest.x, rest.w, v.x, v.w);
    let sides = [[v.right(), y], [v.x - rest.w, y]];
    let ends = [[x, v.bottom()], [x, v.y - rest.h]];
    let (first, then) = if beside { (sides, ends) } else { (ends, sides) };
    let far = |[sx, sy]: [f64; 2]| (sx - rest.x).hypot(sy - rest.y);
    let nearest = |spots: [[f64; 2]; 2]| {
        spots.into_iter().filter(|[sx, sy]| free(*sx, *sy)).min_by(|a, b| far(*a).total_cmp(&far(*b)))
    };
    nearest(first).or_else(|| nearest(then)).unwrap_or([rest.x, rest.y])
}

/// How far, in points, the top left of `part`'s bounds is from the top left of `all`'s.
fn shift(part: &[Rect], all: &[Rect], ppu: f64) -> [f64; 2] {
    let (part, all) = (layout::bounds(part), layout::bounds(all));
    let ppu = if ppu > 0.0 { ppu } else { 1.0 };
    [(part.x - all.x) / ppu, (part.y - all.y) / ppu]
}

// ---------------------------------------------------------------- the host's threads

struct HostJob {
    peer: Id,
    serial: u32,
    want: Want,
    addr: SocketAddr,
    hello: Hello,
    keys: Keys,
    feed: Feed,
    placed: Receiver<[i32; 2]>,
    events: Sender<Event>,
    displays: Arc<dyn Displays>,
    install: bool,
}

fn host(job: HostJob) {
    let why = match run_host(&job) {
        Ok(why) => why,
        Err(why) => why,
    };
    job.feed.stop();
    let _ = job.events.send(Event::Extend(ExtendEvent::Ended { peer: job.peer, serial: job.serial, why }));
}

/// Makes the display and sends it until either side stops. Answers why it stopped.
fn run_host(job: &HostJob) -> Result<String, String> {
    let (peer, serial) = (job.peer, job.serial);
    let working = |words: &str| {
        let _ = job.events.send(Event::Extend(ExtendEvent::Working { peer, serial, words: words.into() }));
    };
    if job.install {
        job.displays.install()?;
        working("Starting the display");
    }
    if job.feed.stopped() {
        return Ok("Stopped".into());
    }
    let mut hosted = job.displays.host(&job.want, job.feed.clone())?;
    let size = hosted.size();
    working("Connecting");
    let stream = TcpStream::connect_timeout(&job.addr, Duration::from_secs(4))
        .map_err(|err| format!("Couldn't reach {}: {err}", job.want.name))?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    // A small send buffer, as game streaming keeps: pictures that can't go out yet wait in the
    // feed, where a newer one replaces them, rather than queueing in the network for seconds.
    let _ = socket2::SockRef::from(&stream).set_send_buffer_size(SEND_BUFFER);
    let reader = stream.try_clone().map_err(|err| err.to_string())?;
    let mut out = BufWriter::with_capacity(256 * 1024, stream.try_clone().map_err(|err| err.to_string())?);
    job.hello.write(&mut out).map_err(|err| err.to_string())?;
    let mut sealer = Sealer::new(job.keys.pictures, job.hello.stream);
    sealer.down(&mut out, &Down::Size(size)).map_err(|err| err.to_string())?;
    out.flush().map_err(|err| err.to_string())?;
    // What comes back: the lending computer's mouse and keyboard, and wishes for key pictures.
    let back = {
        let feed = job.feed.clone();
        let events = job.events.clone();
        let mut opener = Opener::new(job.keys.input, job.hello.stream);
        std::thread::Builder::new().name("roam-host-input".into()).spawn(move || {
            let mut reader = BufReader::new(reader);
            loop {
                match opener.up(&mut reader) {
                    Ok(Up::Input(remote)) => {
                        let _ = events.send(Event::Extend(ExtendEvent::Remote { peer, serial, remote }));
                    }
                    Ok(Up::KeyPlease) => feed.ask_key(),
                    Ok(Up::Bye) | Err(_) => break,
                }
            }
            feed.stop();
        })
    };
    if back.is_err() {
        return Err("Couldn't start the display".into());
    }
    let mut checked = Instant::now() - CHECK_EVERY;
    let mut was: Option<Option<Rect>> = None;
    let mut why = "Stopped".to_owned();
    let mut tallied = Instant::now();
    let _ = job.feed.take_tally();
    while !job.feed.stopped() {
        if let Some(picture) = job.feed.next(Duration::from_millis(40)) {
            let started = Instant::now();
            let sent = sealer.down(&mut out, &Down::Picture(picture)).and_then(|()| out.flush());
            if let Err(err) = sent {
                why = format!("The connection dropped: {err}");
                break;
            }
            job.feed.count_sent(started.elapsed());
        }
        if tallied.elapsed() >= TALLY_EVERY {
            let text = job.feed.take_tally().line(tallied.elapsed().as_secs_f64());
            tallied = Instant::now();
            let _ = job.events.send(Event::Extend(ExtendEvent::Note { text }));
        }
        if checked.elapsed() >= CHECK_EVERY {
            checked = Instant::now();
            for text in job.feed.take_notes() {
                let _ = job.events.send(Event::Extend(ExtendEvent::Note { text }));
            }
            while let Ok(at) = job.placed.try_recv() {
                hosted.place(at);
            }
            let rect = hosted.rect();
            if was != Some(rect) {
                was = Some(rect);
                let _ = job.events.send(Event::Extend(ExtendEvent::At { peer, serial, rect }));
            }
        }
    }
    if let Some(failed) = hosted.failed() {
        why = failed;
    }
    let _ = sealer.down(&mut out, &Down::Bye).and_then(|()| out.flush());
    let _ = stream.shutdown(Shutdown::Both);
    drop(hosted);
    Ok(why)
}

// ---------------------------------------------------------------- the lending side's threads

/// Shows a stream that came in on the port for big clipboards, if this computer lent a screen
/// to the session it names.
pub(super) fn view(stream: TcpStream, hello: Hello, events: &Sender<Event>) -> std::io::Result<()> {
    let (answer, granted) = mpsc::channel();
    events
        .send(Event::Extend(ExtendEvent::Stream { session: hello.session, answer }))
        .map_err(std::io::Error::other)?;
    let Ok(Some(grant)) = granted.recv_timeout(Duration::from_secs(2)) else { return Ok(()) };
    stream.set_read_timeout(None)?;
    let _ = stream.set_nodelay(true);
    if let Ok(mut slot) = grant.stream.lock() {
        *slot = Some(stream.try_clone()?);
    }
    // What the person does over the screen goes back on its own thread, never holding up the
    // window it comes from.
    let (up, ups) = mpsc::channel::<Up>();
    let writer = stream.try_clone()?;
    let mut sealer = Sealer::new(grant.keys.input, hello.stream);
    std::thread::Builder::new().name("roam-view-input".into()).spawn(move || {
        let mut writer = BufWriter::new(writer);
        while let Ok(message) = ups.recv() {
            if sealer.up(&mut writer, &message).and_then(|()| writer.flush()).is_err() {
                break;
            }
        }
    })?;
    let mut opener = Opener::new(grant.keys.pictures, hello.stream);
    let mut reader = BufReader::with_capacity(256 * 1024, stream);
    let mut showing: Option<Box<dyn display::Showing>> = None;
    let mut asked_key = Instant::now() - KEY_EVERY;
    let mut arrivals = Arrivals::default();
    let mut tallied = Instant::now();
    let why = loop {
        if tallied.elapsed() >= TALLY_EVERY {
            let text = std::mem::take(&mut arrivals).line(tallied.elapsed().as_secs_f64());
            tallied = Instant::now();
            let _ = events.send(Event::Extend(ExtendEvent::Note { text }));
        }
        match opener.down(&mut reader) {
            Ok(Down::Size(size)) => {
                showing = None;
                let up = up.clone();
                let remote = Box::new(move |remote: Remote| {
                    let _ = up.send(Up::Input(remote));
                });
                match grant.displays.show(grant.rect, size, remote) {
                    Ok(shown) => showing = Some(shown),
                    Err(err) => break format!("Couldn't show the picture: {err}"),
                }
            }
            Ok(Down::Picture(picture)) => {
                let Some(shown) = showing.as_mut() else { continue };
                let came = Instant::now();
                let result = shown.picture(&picture);
                arrivals.count(came, picture.data.len(), came.elapsed(), result.is_err());
                if result.is_err() && asked_key.elapsed() >= KEY_EVERY {
                    asked_key = Instant::now();
                    let _ = up.send(Up::KeyPlease);
                }
            }
            Ok(Down::Bye) => break "Stopped".to_owned(),
            Err(err) => break format!("The connection ended: {err}"),
        }
    };
    let _ = up.send(Up::Bye);
    drop(showing);
    let _ = events.send(Event::Extend(ExtendEvent::LendEnded { serial: grant.serial, why }));
    Ok(())
}

/// How a lent screen's pictures came in since the last look, for the log.
#[derive(Default)]
struct Arrivals {
    pictures: u32,
    bytes: u64,
    failed: u32,
    /// The longest time between two pictures, and the longest one took to hand to the screen.
    longest_gap: Duration,
    longest_show: Duration,
    last: Option<Instant>,
}

impl Arrivals {
    fn count(&mut self, at: Instant, bytes: usize, show: Duration, failed: bool) {
        self.pictures += 1;
        self.bytes += bytes as u64;
        self.failed += u32::from(failed);
        if let Some(last) = self.last {
            self.longest_gap = self.longest_gap.max(at.saturating_duration_since(last));
        }
        self.last = Some(at);
        self.longest_show = self.longest_show.max(show);
    }

    fn line(&self, seconds: f64) -> String {
        let seconds = seconds.max(0.001);
        format!(
            "lent screen, last {seconds:.0} s: {:.0} pictures a second, {:.1} Mbit/s, longest gap {:.0} ms, longest to show {:.1} ms, {} not shown",
            f64::from(self.pictures) / seconds,
            self.bytes as f64 * 8.0 / seconds / 1_000_000.0,
            self.longest_gap.as_secs_f64() * 1000.0,
            self.longest_show.as_secs_f64() * 1000.0,
            self.failed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lent_screen_counts_what_came_in() {
        let start = Instant::now();
        let mut arrivals = Arrivals::default();
        arrivals.count(start, 250_000, Duration::from_micros(500), false);
        arrivals.count(start + Duration::from_millis(17), 250_000, Duration::from_millis(2), false);
        arrivals.count(start + Duration::from_millis(60), 250_000, Duration::from_micros(300), true);
        assert_eq!(
            arrivals.line(1.0),
            "lent screen, last 1 s: 3 pictures a second, 6.0 Mbit/s, longest gap 43 ms, longest to show 2.0 ms, 1 not shown"
        );
    }

    #[test]
    fn screens_left_on_the_lending_computer_never_sit_on_the_hosts() {
        // PC | MacBook | external, and the external lent: Windows puts the virtual display
        // against the PC's screen, where the MacBook was. The MacBook goes past it instead.
        let pc = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let virtual_on_desk = Rect::new(1920.0, 0.0, 1920.0, 1080.0);
        let macbook = Rect::new(1512.0, 100.0, 1512.0, 982.0);
        assert_eq!(clear_of(macbook, virtual_on_desk, &[pc, virtual_on_desk], true), [3840.0, 98.0]);
        // Already clear: it stays.
        let below = Rect::new(2000.0, 1080.0, 1512.0, 982.0);
        assert_eq!(clear_of(below, virtual_on_desk, &[pc, virtual_on_desk], false), [2000.0, 1080.0]);
        // Under both: it goes under the virtual display, still facing it.
        let under = Rect::new(0.0, 900.0, 1512.0, 982.0);
        assert_eq!(clear_of(under, virtual_on_desk, &[pc, virtual_on_desk], false), [1920.0, 1080.0]);
    }
}
