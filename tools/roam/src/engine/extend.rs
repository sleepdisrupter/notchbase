//! Screens lent between computers as extended displays.
//!
//! The controlling computer asks a computer it controls for one of its screens (`Lend`). That
//! computer says how big the screen is (`Lent`), and the controller makes a virtual display
//! that size where the screen sits on the desk, then streams it over a connection of its own.
//! Either side ends it with `Unlend`.
//!
//! A network hiccup ends neither: the host keeps its virtual display (so the system doesn't
//! move every window off it) and tries the picture stream again, the lending computer keeps its
//! window with the last picture, and when the network went quiet long enough to end the session,
//! the host asks for the screen again as soon as the session is back.
//!
//! While a screen is lent, the desk counts it as the host's display: the host's cursor reaches
//! it the way it reaches any of its own displays, and the lending computer's other screens stay
//! where they were for the cursor to cross to.

use std::io::{BufReader, BufWriter, Write as _};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Core, Cursor, Event, Session, Side};
use crate::crypto::{self, hex};
use crate::display::stream::{self, Down, Hello, Keys, Opener, Sealer, Up};
use crate::display::{self, Ability, Displays, Feed, Panel, Remote, Showing, Want};
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
/// How long an extended display waits for the other computer after the network drops, before
/// it gives up and the virtual display goes.
const HOLD_FOR: u64 = 120 * super::SECOND;
const HOLD: Duration = Duration::from_secs(120);
/// How often the host tries the picture stream again while it's down.
const RETRY_EVERY: Duration = Duration::from_millis(700);
/// A stream that lasted this long reached the other computer, which would have turned it away
/// within two seconds: trying again starts the wait over.
const LASTED: Duration = Duration::from_millis(2_500);
/// A lent screen that hears nothing for this long takes the stream as gone (a still screen still
/// sends a picture a second).
const QUIET: Duration = Duration::from_secs(5);

/// One of another computer's screens shown here as an extended display (this computer hosts).
pub(super) struct Extended {
    pub serial: u32,
    pub display: u8,
    pub state: State,
    /// Where the virtual display is, in this computer's units, once it's up.
    pub rect: Option<Rect>,
    pub feed: Option<Feed>,
    pub place: Option<Sender<[i32; 2]>>,
    /// The way to the other computer's stream, for the host's thread: a new one when the
    /// session forms again.
    pub route: Option<Sender<Route>>,
    pub asked_at: u64,
    /// When the session went quiet, while the display waits for it to come back.
    pub held_since: Option<u64>,
}

/// Where the host sends its pictures, and with which keys.
#[derive(Clone, Copy)]
pub(super) struct Route {
    addr: SocketAddr,
    session: u32,
    keys: Keys,
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
    /// The window showing the pictures, kept from one stream to the next.
    pub shown: Arc<Mutex<Option<Shown>>>,
    /// Where what the person does over it goes: the newest stream's way back.
    pub up: Arc<Mutex<Option<Sender<Up>>>>,
    /// When the session went quiet, while the window waits for the host to come back.
    pub held_since: Option<u64>,
}

/// A lent screen's window, and the size of the pictures it was made for.
pub(super) struct Shown {
    size: [u32; 2],
    showing: Box<dyn Showing>,
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
    shown: Arc<Mutex<Option<Shown>>>,
    up: Arc<Mutex<Option<Sender<Up>>>>,
    who: String,
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
        let mut entry = Extended {
            serial,
            display,
            state: State::Asking,
            rect: None,
            feed: None,
            place: None,
            route: None,
            asked_at: now,
            held_since: None,
        };
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
        let Some(entry) = self.extended.get(&peer).filter(|e| e.display == display) else { return };
        let held = entry.held_since.is_some() && entry.route.is_some();
        if !held && entry.state != State::Asking {
            return;
        }
        let serial = entry.serial;
        let panel = Panel {
            pixels: pixels.map(u32::from),
            hz: u32::from(hz).clamp(24, 240),
            scale: if scale.is_finite() { scale.clamp(1.0, 4.0) } else { 1.0 },
        };
        let at = self.virtual_at(peer, display).unwrap_or_default();
        let name = self.who(peer);
        // The other computer draws the pointer itself, where it can: it then moves without
        // waiting for the pictures.
        let pointer_apart = self.peers.get(&peer).is_some_and(|p| p.draws_pointer)
            && std::env::var_os(display::POINTER_IN_PICTURE).is_none();
        let Some(session) = self.sessions.get_mut(&peer) else { return };
        let addr = session.addr;
        let session_id = session.id;
        let Some(link) = session.live() else { return };
        let route = Route { addr, session: session_id, keys: stream::keys(link.bulk_send()) };
        if held {
            // Back after the network dropped: the display is still up, only the way to it is new.
            self.note(&format!("{name} lends its display {display} again: the extended display carries on"));
            if let Some(entry) = self.extended.get_mut(&peer) {
                entry.held_since = None;
                entry.state = State::Working("Reconnecting".into());
                if let Some(routes) = &entry.route {
                    let _ = routes.send(route);
                }
            }
            return;
        }
        let feed = Feed::new();
        let (reroute, routes) = mpsc::channel();
        let (place, placed) = mpsc::channel();
        let install = self.ability == Ability::NeedsDriver;
        let job = HostJob {
            peer,
            serial,
            want: Want { name: name.clone(), panel, at, pointer_apart },
            route,
            routes,
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
        entry.route = Some(reroute);
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

    /// An extended display asked for and never answered gives up; one waiting for its session to
    /// come back asks again once it has, and gives up when it doesn't.
    pub(super) fn check_extended(&mut self, now: u64) {
        let (mut ask, mut gone) = (Vec::new(), Vec::new());
        for (id, entry) in &mut self.extended {
            if entry.state == State::Asking && now.saturating_sub(entry.asked_at) > ASK_FOR {
                let name = self.peers.get(id).map_or_else(|| "The other computer".into(), |p| p.name.clone());
                entry.state = State::Failed(format!("{name} didn't answer"));
            }
            let Some(since) = entry.held_since else { continue };
            if now.saturating_sub(since) > HOLD_FOR {
                gone.push(*id);
            } else if now.saturating_sub(entry.asked_at) > ASK_FOR
                && self.sessions.get(id).is_some_and(|s| s.side == Side::Controller && s.is_live())
            {
                entry.asked_at = now;
                ask.push((*id, entry.display));
            }
        }
        for (id, display) in ask {
            self.note(&format!("{} is back: asking for its display {display} again", self.who(id)));
            if let Some(link) = self.sessions.get_mut(&id).and_then(Session::live) {
                link.queue(Message::Lend { display });
            }
        }
        for id in gone {
            let why = format!("{} didn't come back", self.who(id));
            self.unextend(id, &why);
        }
        if self.lending.as_ref().and_then(|l| l.held_since).is_some_and(|since| now.saturating_sub(since) > HOLD_FOR) {
            if let Some(to) = self.lending.as_ref().map(|l| l.to) {
                self.note(&format!("{} didn't come back for its extended display", self.who(to)));
            }
            self.stop_lending(None);
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
        if let Some(lending) =
            self.lending.as_mut().filter(|l| l.to == to && l.display == display && same(&l.rect, &rect))
        {
            // The host is back after the network dropped: the same window shows its pictures.
            lending.session = session;
            lending.held_since = None;
            self.note(&format!("{} asked for display {display} again: the lent screen carries on", self.who(to)));
        } else {
            self.lend_anew(to, display, rect, panel, session);
        }
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

    fn lend_anew(&mut self, to: Id, display: u8, rect: Rect, panel: Panel, session: u32) {
        self.stop_lending(None);
        let serial = self.next_serial();
        self.note(&format!(
            "{} asked for display {display} ({}x{} pixels at {} Hz) as its extended display",
            self.who(to),
            panel.pixels[0],
            panel.pixels[1],
            panel.hz
        ));
        self.lending = Some(Lending {
            serial,
            to,
            display,
            rect,
            session,
            stream: Arc::default(),
            shown: Arc::default(),
            up: Arc::default(),
            held_since: None,
        });
    }

    /// Stops showing another computer's picture, telling it why when `why` is given.
    pub(super) fn stop_lending(&mut self, why: Option<&str>) {
        let Some(lending) = self.lending.take() else { return };
        if let Some(up) = lending.up.lock().ok().and_then(|mut up| up.take()) {
            let _ = up.send(Up::Bye);
        }
        if let Some(stream) = lending.stream.lock().ok().and_then(|mut s| s.take()) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        // Gives the screen back.
        drop(lending.shown.lock().ok().and_then(|mut shown| shown.take()));
        if let Some(why) = why {
            self.note(&format!("no longer lending display {} to {}: {why}", lending.display, self.who(lending.to)));
            if let Some(link) = self.sessions.get_mut(&lending.to).and_then(Session::live) {
                link.queue(Message::Unlend { display: lending.display, why: why.to_owned() });
            }
        }
    }

    /// The network to `id` went quiet for long enough to end the session: whatever is lent
    /// between them holds on for it to come back, rather than ending.
    pub(super) fn hold_displays(&mut self, id: Id) {
        let now = self.now();
        let name = self.who(id);
        if let Some(entry) = self.extended.get_mut(&id).filter(|e| e.route.is_some() && e.held_since.is_none())
            && matches!(entry.state, State::Running | State::Working(_))
        {
            entry.held_since = Some(now);
            // Ask again as soon as the session is back.
            entry.asked_at = 0;
            entry.state = State::Working(format!("Waiting for {name} to come back"));
            self.note(&format!("the extended display of {name} stays up while {name} comes back"));
        }
        if let Some(lending) = self.lending.as_mut().filter(|l| l.to == id && l.held_since.is_none()) {
            lending.held_since = Some(now);
            self.note(&format!("the screen lent to {name} stays up while {name} comes back"));
        }
    }

    /// A computer in a session with this one stopped: whatever was lent between them ends,
    /// unless it holds on for the session to come back.
    pub(super) fn session_ended_displays(&mut self, id: Id) {
        if self.extended.get(&id).is_none_or(|e| e.held_since.is_none())
            && let Some(entry) = self.extended.remove(&id)
            && let Some(feed) = entry.feed
        {
            feed.stop();
        }
        if self.lending.as_ref().is_some_and(|l| l.to == id && l.held_since.is_none()) {
            self.stop_lending(None);
        }
    }

    /// Tests: cuts the picture stream of the lent screen, as a network hiccup does.
    #[cfg(test)]
    pub(super) fn cut_lent_stream(&mut self) {
        if let Some(stream) = self.lending.as_ref().and_then(|l| l.stream.lock().ok()?.take()) {
            let _ = stream.shutdown(Shutdown::Both);
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
                let who = self.lending.as_ref().map(|l| self.who(l.to)).unwrap_or_default();
                let grant = self.lending.as_ref().filter(|l| l.session == session).and_then(|lending| {
                    let session = self.sessions.get_mut(&lending.to)?;
                    let keys = stream::keys(session.live()?.bulk_receive());
                    Some(Grant {
                        keys,
                        rect: lending.rect,
                        serial: lending.serial,
                        displays: self.displays.clone(),
                        stream: lending.stream.clone(),
                        shown: lending.shown.clone(),
                        up: lending.up.clone(),
                        who,
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
    /// The way to the other computer's stream, and newer ones after the session forms again.
    route: Route,
    routes: Receiver<Route>,
    feed: Feed,
    placed: Receiver<[i32; 2]>,
    events: Sender<Event>,
    displays: Arc<dyn Displays>,
    install: bool,
}

impl HostJob {
    fn working(&self, words: &str) {
        let (peer, serial) = (self.peer, self.serial);
        let _ = self.events.send(Event::Extend(ExtendEvent::Working { peer, serial, words: words.into() }));
    }

    fn note(&self, text: String) {
        let _ = self.events.send(Event::Extend(ExtendEvent::Note { text }));
    }
}

fn host(job: HostJob) {
    let why = match run_host(&job) {
        Ok(why) => why,
        Err(why) => why,
    };
    job.feed.stop();
    let _ = job.events.send(Event::Extend(ExtendEvent::Ended { peer: job.peer, serial: job.serial, why }));
}

/// How one picture stream ended.
enum Streamed {
    /// For good: either side stopped.
    Over(String),
    /// The network dropped it; `lasted` when it had got through to the other computer.
    Dropped { why: String, lasted: bool },
}

/// What the host's thread looks after between pictures: lines for the log, and where the
/// virtual display is.
struct Upkeep {
    checked: Instant,
    was: Option<Option<Rect>>,
    tallied: Instant,
}

impl Upkeep {
    fn run(&mut self, job: &HostJob, hosted: &mut Box<dyn display::Hosted>) {
        if self.tallied.elapsed() >= TALLY_EVERY {
            let text = job.feed.take_tally().line(self.tallied.elapsed().as_secs_f64());
            self.tallied = Instant::now();
            job.note(text);
        }
        if self.checked.elapsed() < CHECK_EVERY {
            return;
        }
        self.checked = Instant::now();
        for text in job.feed.take_notes() {
            job.note(text);
        }
        while let Ok(at) = job.placed.try_recv() {
            hosted.place(at);
        }
        let rect = hosted.rect();
        if self.was != Some(rect) {
            self.was = Some(rect);
            let (peer, serial) = (job.peer, job.serial);
            let _ = job.events.send(Event::Extend(ExtendEvent::At { peer, serial, rect }));
        }
    }
}

/// Makes the display and sends it until either side stops, keeping it up through network
/// hiccups: the stream is tried again until it's back, or the other computer hasn't come back
/// for a while. Answers why it stopped.
fn run_host(job: &HostJob) -> Result<String, String> {
    if job.install {
        job.displays.install()?;
        job.working("Starting the display");
    }
    if job.feed.stopped() {
        return Ok("Stopped".into());
    }
    let mut hosted = job.displays.host(&job.want, job.feed.clone())?;
    job.working("Connecting");
    let mut route = job.route;
    let mut upkeep = Upkeep { checked: Instant::now() - CHECK_EVERY, was: None, tallied: Instant::now() };
    let _ = job.feed.take_tally();
    // Since when the stream hasn't got through.
    let mut down_since: Option<Instant> = None;
    let mut ever = false;
    let mut why = loop {
        if job.feed.stopped() {
            break "Stopped".into();
        }
        while let Ok(newer) = job.routes.try_recv() {
            route = newer;
        }
        match stream_once(job, route, &mut hosted, &mut upkeep) {
            Streamed::Over(why) => break why,
            Streamed::Dropped { why, lasted } => {
                if job.feed.stopped() {
                    break "Stopped".into();
                }
                if !ever && !lasted {
                    // It never got through at all: as before, that's the end.
                    return Err(why);
                }
                if lasted {
                    ever = true;
                    down_since = None;
                    job.note(format!("the extended display's picture stream dropped ({why}): trying again"));
                    job.working("Reconnecting");
                    // The next one starts the picture over.
                    upkeep.was = None;
                }
                let since = *down_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= HOLD {
                    break format!("The other computer didn't come back: {why}");
                }
                if let Ok(newer) = job.routes.recv_timeout(RETRY_EVERY) {
                    route = newer;
                }
                upkeep.run(job, &mut hosted);
            }
        }
    };
    if let Some(failed) = hosted.failed() {
        why = failed;
    }
    drop(hosted);
    Ok(why)
}

/// One picture stream, from connecting until it ends.
fn stream_once(job: &HostJob, route: Route, hosted: &mut Box<dyn display::Hosted>, upkeep: &mut Upkeep) -> Streamed {
    let started = Instant::now();
    let dropped = |why: String| Streamed::Dropped { why, lasted: started.elapsed() >= LASTED };
    let stream = match TcpStream::connect_timeout(&route.addr, Duration::from_secs(4)) {
        Ok(stream) => stream,
        Err(err) => return dropped(format!("Couldn't reach {}: {err}", job.want.name)),
    };
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    // A small send buffer, as game streaming keeps: pictures that can't go out yet wait in the
    // feed, where a newer one replaces them, rather than queueing in the network for seconds.
    let _ = socket2::SockRef::from(&stream).set_send_buffer_size(SEND_BUFFER);
    let (Ok(reader), Ok(writer)) = (stream.try_clone(), stream.try_clone()) else {
        return dropped("Couldn't use the connection".into());
    };
    let mut out = BufWriter::with_capacity(256 * 1024, writer);
    // A new number for every stream: the same keys never seal two streams' messages alike.
    let hello = Hello { session: route.session, stream: u32::from_le_bytes(crypto::random::<4>()) };
    let mut sealer = Sealer::new(route.keys.pictures, hello.stream);
    // Pictures made while there was no stream are stale: start from a key picture, and say
    // where the pointer is.
    job.feed.clear_pictures();
    job.feed.ask_key();
    job.feed.ask_pointer();
    let said = hello
        .write(&mut out)
        .and_then(|()| sealer.down(&mut out, &Down::Size(hosted.size())))
        .and_then(|()| out.flush());
    if let Err(err) = said {
        return dropped(format!("The connection dropped: {err}"));
    }
    // What comes back: the lending computer's mouse and keyboard, and wishes for key pictures.
    let (gone, bye) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let back = {
        let (feed, events) = (job.feed.clone(), job.events.clone());
        let (gone, bye) = (gone.clone(), bye.clone());
        let (peer, serial) = (job.peer, job.serial);
        let mut opener = Opener::new(route.keys.input, hello.stream);
        std::thread::Builder::new().name("roam-host-input".into()).spawn(move || {
            let mut reader = BufReader::new(reader);
            loop {
                match opener.up(&mut reader) {
                    Ok(Up::Input(remote)) => {
                        let _ = events.send(Event::Extend(ExtendEvent::Remote { peer, serial, remote }));
                    }
                    Ok(Up::KeyPlease) => feed.ask_key(),
                    Ok(Up::Bye) => {
                        bye.store(true, Ordering::Release);
                        break;
                    }
                    Err(_) => break,
                }
            }
            gone.store(true, Ordering::Release);
            // Wakes the sending loop.
            feed.pointer(None, None);
        })
    };
    if back.is_err() {
        return dropped("Couldn't start the display".into());
    }
    let ended = loop {
        if job.feed.stopped() {
            let _ = sealer.down(&mut out, &Down::Bye).and_then(|()| out.flush());
            break Streamed::Over("Stopped".into());
        }
        if bye.load(Ordering::Acquire) {
            break Streamed::Over("Stopped".into());
        }
        if gone.load(Ordering::Acquire) {
            break dropped("the other computer closed it".into());
        }
        let picture = job.feed.next(Duration::from_millis(40));
        // The pointer's news goes first: it's small, and the reason it goes apart is to be quick.
        let news = job.feed.take_pointer();
        let pointed = news.shape.is_some() || news.at.is_some();
        let mut sent = Ok(());
        if let Some(shape) = news.shape {
            sent = sealer.down(&mut out, &Down::Shape(shape));
        }
        if let Some(at) = news.at.filter(|_| sent.is_ok()) {
            sent = sealer.down(&mut out, &Down::Pointer(at));
        }
        if pointed && sent.is_ok() {
            sent = out.flush();
        }
        if let Some(picture) = picture.filter(|_| sent.is_ok()) {
            let started = Instant::now();
            sent = sealer.down(&mut out, &Down::Picture(picture)).and_then(|()| out.flush());
            if sent.is_ok() {
                job.feed.count_sent(started.elapsed());
            }
        }
        if let Err(err) = sent {
            break dropped(format!("The connection dropped: {err}"));
        }
        upkeep.run(job, hosted);
    };
    let _ = stream.shutdown(Shutdown::Both);
    ended
}

// ---------------------------------------------------------------- the lending side's threads

/// Shows a stream that came in on the port for big clipboards, if this computer lent a screen
/// to the session it names. A stream that drops leaves the window up, with its last picture,
/// for the host's next stream to carry on in.
pub(super) fn view(stream: TcpStream, hello: Hello, events: &Sender<Event>) -> std::io::Result<()> {
    let (answer, granted) = mpsc::channel();
    events
        .send(Event::Extend(ExtendEvent::Stream { session: hello.session, answer }))
        .map_err(std::io::Error::other)?;
    let Ok(Some(grant)) = granted.recv_timeout(Duration::from_secs(2)) else { return Ok(()) };
    // A still screen still sends a picture a second: hearing nothing for longer, it's gone.
    stream.set_read_timeout(Some(QUIET))?;
    let _ = stream.set_nodelay(true);
    if let Ok(mut slot) = grant.stream.lock()
        && let Some(old) = slot.replace(stream.try_clone()?)
    {
        // The stream before this one, if it's still hanging on.
        let _ = old.shutdown(Shutdown::Both);
    }
    // What the person does over the screen goes back on its own thread, never holding up the
    // window it comes from, by the newest stream.
    let (up, ups) = mpsc::channel::<Up>();
    if let Ok(mut slot) = grant.up.lock() {
        *slot = Some(up.clone());
    }
    let writer = stream.try_clone()?;
    let mut sealer = Sealer::new(grant.keys.input, hello.stream);
    std::thread::Builder::new().name("roam-view-input".into()).spawn(move || {
        let mut writer = BufWriter::new(writer);
        while let Ok(message) = ups.recv() {
            let bye = message == Up::Bye;
            if sealer.up(&mut writer, &message).and_then(|()| writer.flush()).is_err() || bye {
                break;
            }
        }
    })?;
    let mut opener = Opener::new(grant.keys.pictures, hello.stream);
    let mut reader = BufReader::with_capacity(256 * 1024, stream);
    let mut asked_key = Instant::now() - KEY_EVERY;
    let mut arrivals = Arrivals::default();
    let mut tallied = Instant::now();
    let shown = grant.shown.clone();
    let with_shown = |what: &mut dyn FnMut(&mut Box<dyn Showing>)| {
        if let Some(shown) = shown.lock().ok().as_mut().and_then(|s| s.as_mut()) {
            what(&mut shown.showing);
        }
    };
    let ended = loop {
        if tallied.elapsed() >= TALLY_EVERY {
            with_shown(&mut |shown| arrivals.pointer_lag = shown.take_pointer_lag());
            let text = std::mem::take(&mut arrivals).line(tallied.elapsed().as_secs_f64());
            tallied = Instant::now();
            let _ = events.send(Event::Extend(ExtendEvent::Note { text }));
        }
        match opener.down(&mut reader) {
            Ok(Down::Size(size)) => {
                let Ok(mut slot) = grant.shown.lock() else { break Some("Couldn't show the picture".to_owned()) };
                if slot.as_ref().is_some_and(|s| s.size == size) {
                    continue;
                }
                *slot = None;
                let up = grant.up.clone();
                let remote = Box::new(move |remote: Remote| {
                    if let Some(up) = up.lock().ok().as_ref().and_then(|up| up.as_ref()) {
                        let _ = up.send(Up::Input(remote));
                    }
                });
                match grant.displays.show(grant.rect, size, remote) {
                    Ok(showing) => *slot = Some(Shown { size, showing }),
                    Err(err) => break Some(format!("Couldn't show the picture: {err}")),
                }
            }
            Ok(Down::Picture(picture)) => {
                let came = Instant::now();
                let mut result = None;
                with_shown(&mut |shown| result = Some(shown.picture(&picture)));
                let Some(result) = result else { continue };
                arrivals.count(came, picture.data.len(), came.elapsed(), result.is_err());
                if result.is_err() && asked_key.elapsed() >= KEY_EVERY {
                    asked_key = Instant::now();
                    let _ = up.send(Up::KeyPlease);
                }
            }
            Ok(Down::Pointer(at)) => {
                arrivals.pointers += 1;
                with_shown(&mut |shown| shown.pointer(at));
            }
            Ok(Down::Shape(shape)) => with_shown(&mut |shown| shown.shape(&shape)),
            Ok(Down::Bye) => break Some("Stopped".to_owned()),
            Err(err) => {
                let text = format!("the picture stream from {} dropped ({err}): waiting for it again", grant.who);
                let _ = events.send(Event::Extend(ExtendEvent::Note { text }));
                break None;
            }
        }
    };
    if let Some(why) = ended {
        let _ = events.send(Event::Extend(ExtendEvent::LendEnded { serial: grant.serial, why }));
    }
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
    /// Pointer moves the host sent apart from the pictures, and the longest one waited to be drawn.
    pointers: u32,
    pointer_lag: Option<Duration>,
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
        ) + &self.pointer_line(seconds)
    }

    fn pointer_line(&self, seconds: f64) -> String {
        if self.pointers == 0 {
            return String::new();
        }
        let lag = self
            .pointer_lag
            .map_or(String::new(), |lag| format!(", longest to draw {:.1} ms", lag.as_secs_f64() * 1000.0));
        format!("; the pointer drawn here, {:.0} moves a second{lag}", f64::from(self.pointers) / seconds)
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
        arrivals.pointers = 240;
        arrivals.pointer_lag = Some(Duration::from_micros(1500));
        assert!(
            arrivals
                .line(2.0)
                .ends_with("1 not shown; the pointer drawn here, 120 moves a second, longest to draw 1.5 ms")
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
