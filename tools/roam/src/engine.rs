//! The part that runs all the time: finds nearby computers, keeps a session with each one on
//! the desk, and moves the cursor between them. It runs on its own threads, so input never waits
//! for the notch to draw: a mouse move goes from the system's hook to the network in
//! microseconds.
//!
//! The controlling computer decides everything about the cursor: it keeps the cursor's position
//! on every computer itself and knows the desk, so crossing to the next screen happens the moment
//! you reach the edge, with no round trip. The controlled computer just shows what it's told.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

mod extend;

use extend::{ExtendEvent, Extended, Lending};
pub use extend::{ExtendedView, LendingView};

use crate::bulk::{self, Arrived, Content, Hello};
use crate::crypto::{self, Ephemeral, Handshake, Identity, commit, hex, unhex};
use crate::display::{self, Ability, Displays};
use crate::input::{self, Copied, Input, Platform, Play, Swipe};
use crate::keys;
use crate::layout::{self, Computer, Rect, Step};
use crate::link::Link;
use crate::priority::{self, Traffic};
use crate::smooth::{FRAME, Smoother};
use crate::wire::{BUTTONS, Beacon, CLIP_CHUNK, Id, Key, Message, Os, Packet, Refusal};

pub const PORT: u16 = 47911;
const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 71, 11);

const MS: u64 = 1_000;
const SECOND: u64 = 1_000_000;
const BEACON_EVERY: u64 = SECOND;
/// A computer not heard for this long is no longer nearby.
const GONE: u64 = 5 * SECOND;
const OFFER_EVERY: u64 = 500 * MS;
/// While the cursor is on another computer, packets go out at least this often. A steady trickle
/// keeps both Wi-Fi radios awake, so the next move isn't delayed by a radio waking from sleep.
const BUSY_EVERY: u64 = 20 * MS;
const IDLE_EVERY: u64 = 250 * MS;
/// Mouse moves closer together than this share a packet: a fast mouse reports 1,000 to 8,000
/// times a second, and that many packets would crowd the Wi-Fi for no gain.
const COALESCE: u64 = 1_500;
/// After the mouse stops, a packet without movement follows this soon, so the other computer
/// knows it stopped rather than wondering whether Wi-Fi went quiet.
const STILL: u64 = 4 * MS;
/// With no word for this long while the cursor is away, it comes home and held keys are let go.
const STALL: u64 = 1_500 * MS;
/// With no word for this long, the session is over (and is started again when possible).
const DEAD: u64 = 8 * SECOND;
const STATUS_EVERY: u64 = 100 * MS;
const SCREENS_EVERY: u64 = 2 * SECOND;
/// How often typed addresses are looked up again, and the name and networks read again.
const LOOKUP_EVERY: u64 = 5 * SECOND;
const MOST_CLIPBOARD: usize = 512 * 1024;
const CORNER: f64 = 12.0;
/// Text up to this long goes on the link; longer goes over TCP, as images and files do.
const SHORT_TEXT: usize = 16 * 1024;
/// How often a drag held against the edge is checked for files.
const DRAG_CHECK_EVERY: u64 = 60 * MS;
/// How long after the cursor arrives on another computer it stays there whatever the mouse
/// does: the first moves after a crossing can carry the push that made it, or a jump from the
/// cursor being parked, which would send it straight back (or somewhere else) at once.
const SETTLE_AFTER_CROSSING: u64 = 120 * MS;
/// How often a held button is checked against the mouse itself.
const BUTTONS_CHECK_EVERY: u64 = 500 * MS;
/// How often the log may say why the edge didn't cross.
const EDGE_TOLD_EVERY: u64 = 5 * SECOND;
/// Marks the number of a transfer of dragged files, which a newer clipboard doesn't stop.
const DRAG_ID: u32 = 1 << 31;
/// How many received copies of files are kept, newest first.
const KEEP_RECEIVED: usize = 4;
/// How often the computer checks whether it can make virtual displays.
const ABILITY_EVERY: u64 = 5 * SECOND;

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub open: bool,
    pub name: String,
    pub smoothing: bool,
    pub corners: bool,
    pub clipboard: bool,
    pub modifiers: keys::Modifiers,
    pub addresses: Vec<String>,
    pub fast_wifi: bool,
    /// Whether the cursor crosses to other computers. Off, the mouse and keyboard stay on this
    /// one, and extended displays (this computer's own screens) still work.
    pub roam: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            open: true,
            name: String::new(),
            smoothing: true,
            corners: true,
            clipboard: true,
            modifiers: keys::Modifiers::Same,
            addresses: Vec::new(),
            fast_wifi: true,
            roam: true,
        }
    }
}

/// Where a computer sits on the desk, in points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub id: String,
    pub at: [f64; 2],
    /// How far a scroll here goes on that computer: 1 is the same, 2 twice as far.
    #[serde(default = "same_speed")]
    pub scroll: f64,
}

fn same_speed() -> f64 {
    1.0
}

pub enum Control {
    Prefs(Prefs),
    Desk(Vec<Placement>),
    /// The code on the other computer matches (when we control it), or we allow it to control us.
    Allow(String),
    Decline(String),
    /// Try a computer that said no again.
    Retry(String),
    Forget(String),
    /// Bring the cursor back to this computer.
    Home,
    /// Stop being controlled.
    Release,
    /// Ask the system for the permission Roam is missing.
    AskPermission,
    /// Show this display (by its place in the computer's list) of a computer we control as an
    /// extended display of this one.
    Extend {
        id: String,
        display: u8,
    },
    /// Stop showing that computer's screen here, or forget why it couldn't be.
    Unextend(String),
    /// Stop lending this computer's screen to the computer controlling it.
    StopLending,
    /// Tests: stop hearing the network, as if Wi-Fi dropped out.
    #[cfg(test)]
    Deaf(bool),
    /// Tests: cut the lent screen's picture stream, as a network hiccup does.
    #[cfg(test)]
    CutStream,
}

enum Event {
    Packet(Vec<u8>, SocketAddr, u64),
    Input(Input, u64),
    Control(Control),
    /// The clipboard as read for `Purpose`, with its digest.
    Clipboard(Option<(Copied, [u8; 32])>, Purpose),
    /// A big clipboard is coming in over TCP for `session`: the answer is the key, and the name
    /// of the computer it comes from.
    Bulk {
        session: u32,
        answer: Sender<Option<([u8; 32], String)>>,
    },
    /// How a big clipboard going over is getting on: `None` once it's over, one way or another.
    Moving {
        incoming: bool,
        id: u32,
        transfer: Option<Transfer>,
    },
    /// A big clipboard came in, from the computer named. `slow` when it took long enough that
    /// it's worth saying it's here.
    Arrived {
        arrived: Arrived,
        from: String,
        slow: bool,
    },
    /// The clipboard now holds what came over, which has this digest, with what to tell.
    ClipSet {
        digest: [u8; 32],
        notice: Option<String>,
    },
    /// A line for the log, from another thread.
    Note(String),
    /// Word from the threads carrying extended displays.
    Extend(ExtendEvent),
    /// Slow lookups, done on their own thread: the computer's name, the typed addresses and
    /// the networks it's on.
    Looked {
        name: String,
        addresses: Vec<SocketAddr>,
        networks: Vec<(Ipv4Addr, Ipv4Addr)>,
    },
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// The cursor went to this peer: send it our clipboard.
    Entering(Id),
    /// The cursor left us for the controller: send it ours.
    Leaving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    None,
    Connecting,
    /// Waiting for the codes to be checked.
    Pairing,
    Connected,
    Declined,
    Busy,
    Closed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub os: Os,
    pub displays: Vec<Rect>,
    pub ppu: f64,
    pub open: bool,
    /// Can lend its screens as extended displays.
    pub lends: bool,
    pub nearby: bool,
    pub paired: bool,
    pub link: LinkState,
    /// The round trip, in milliseconds.
    pub rtt: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub id: String,
    pub name: String,
    pub code: String,
    /// We'd control them (and check the code), rather than they us (and allow it).
    pub controlling: bool,
}

/// A copy on its way between this computer and another.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    /// Coming here, rather than going from here.
    pub incoming: bool,
    /// The other computer's name.
    pub with: String,
    /// What is going over: a file's name, "3 items", "an image".
    pub what: String,
    pub done: u64,
    pub total: u64,
}

impl Transfer {
    pub fn share(&self) -> f32 {
        if self.total == 0 { 1.0 } else { (self.done as f64 / self.total as f64).clamp(0.0, 1.0) as f32 }
    }
}

/// Something that came over and is now on the clipboard, worth a notice.
#[derive(Clone, Debug, PartialEq)]
pub struct Landed {
    /// Counts up, so each one is told once.
    pub serial: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub me: Option<PeerView>,
    pub peers: Vec<PeerView>,
    /// Who is controlling this computer.
    pub controller: Option<String>,
    /// The computer our cursor is on, when it isn't here.
    pub away_on: Option<String>,
    /// Another computer's cursor is on this one.
    pub here: bool,
    pub locked: bool,
    pub asks: Vec<Ask>,
    pub problem: Option<String>,
    /// This computer's addresses, for typing into the other one.
    pub addresses: Vec<String>,
    /// The newest big copy on its way, if it's taking a moment.
    pub transfer: Option<Transfer>,
    /// The last big copy that came in.
    pub landed: Option<Landed>,
    /// Whether this computer can make virtual displays for other computers' screens.
    pub ability: Ability,
    /// Other computers' screens shown here as extended displays.
    pub extended: Vec<ExtendedView>,
    /// This computer's screen, lent to another computer as its extended display.
    pub lending: Option<LendingView>,
}

/// A running engine. Dropping it stops everything and gives the mouse and keyboard back.
pub struct Engine {
    events: Sender<Event>,
}

impl Engine {
    pub fn start(
        data: PathBuf,
        platform: Box<dyn Platform>,
        prefs: Prefs,
        desk: Vec<Placement>,
        port: u16,
    ) -> (Engine, Receiver<Status>) {
        Self::start_with(data, platform, display::native(), prefs, desk, port)
    }

    /// Starts with `displays` making and lending extended displays.
    pub fn start_with(
        data: PathBuf,
        platform: Box<dyn Platform>,
        displays: Arc<dyn Displays>,
        prefs: Prefs,
        desk: Vec<Placement>,
        port: u16,
    ) -> (Engine, Receiver<Status>) {
        let (events, inbox) = mpsc::channel();
        let (status_out, status) = mpsc::channel();
        let sender = events.clone();
        let spawned = std::thread::Builder::new().name("roam".into()).spawn(move || {
            let mut core = Core::new(data, platform, displays, prefs, desk, port, sender, status_out);
            core.run(inbox);
        });
        if let Err(err) = spawned {
            eprintln!("roam: couldn't start: {err}");
        }
        (Engine { events }, status)
    }

    pub fn send(&self, control: Control) {
        let _ = self.events.send(Event::Control(control));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Stop);
    }
}

struct Peer {
    key: Key,
    name: String,
    os: Os,
    addr: SocketAddr,
    open: bool,
    busy: bool,
    lends: bool,
    draws_pointer: bool,
    fast_pointer: bool,
    udp_pictures: bool,
    displays: Vec<Rect>,
    ppu: f64,
    seen: u64,
    refused: Option<(Refusal, u64)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Controller,
    Controlled,
}

enum Phase {
    Offered {
        ephemeral: Ephemeral,
        sent: u64,
    },
    Answered {
        ephemeral: Ephemeral,
        commit: [u8; 32],
        peer_key: Key,
    },
    Pairing {
        link: Link,
        code: u32,
        mine: bool,
        theirs: bool,
    },
    Live {
        link: Link,
    },
    /// Only while one phase turns into the next.
    Moving,
}

struct Session {
    id: u32,
    side: Side,
    addr: SocketAddr,
    phase: Phase,
    started: u64,
    /// The controller's revealed key, to reveal again if the answer comes twice.
    revealed: Option<Key>,
}

impl Session {
    fn link(&mut self) -> Option<&mut Link> {
        match &mut self.phase {
            Phase::Pairing { link, .. } | Phase::Live { link } => Some(link),
            _ => None,
        }
    }

    fn live(&mut self) -> Option<&mut Link> {
        match &mut self.phase {
            Phase::Live { link } => Some(link),
            _ => None,
        }
    }

    fn is_live(&self) -> bool {
        matches!(self.phase, Phase::Live { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cursor {
    Home,
    Away { peer: Id, at: [f64; 2], left_from: [f64; 2] },
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    secret: String,
    #[serde(default)]
    paired: HashMap<String, SavedPeer>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedPeer {
    key: String,
    name: String,
}

/// How evenly the cursor's packets come and go, written to the log every few seconds while
/// the cursor is on another computer, to tell a bad network from a slow computer.
#[derive(Default)]
struct Gaps {
    since: u64,
    last: u64,
    count: u32,
    longest: u64,
    over_20: u32,
    over_50: u32,
}

impl Gaps {
    fn record(&mut self, now: u64) {
        if self.since == 0 {
            self.since = now;
        }
        if self.last != 0 {
            let gap = now.saturating_sub(self.last);
            // A pause in moving the mouse isn't a gap.
            if gap < 250 * MS {
                self.longest = self.longest.max(gap);
                self.over_20 += u32::from(gap > 20 * MS);
                self.over_50 += u32::from(gap > 50 * MS);
            }
        }
        self.last = now;
        self.count += 1;
    }

    fn report(&mut self, now: u64, what: &str) -> Option<String> {
        if self.since == 0 || now.saturating_sub(self.since) < 5 * SECOND {
            return None;
        }
        let seconds = now.saturating_sub(self.since) as f64 / SECOND as f64;
        let line = format!(
            "{what}: {:.0} a second, longest gap {:.1} ms, {} gaps over 20 ms, {} over 50 ms",
            f64::from(self.count) / seconds,
            self.longest as f64 / MS as f64,
            self.over_20,
            self.over_50
        );
        *self = Gaps::default();
        Some(line)
    }
}

#[derive(Default)]
struct Clip {
    id: u32,
    total: u16,
    parts: Vec<Option<Vec<u8>>>,
}

struct Core {
    data: PathBuf,
    platform: Box<dyn Platform>,
    prefs: Prefs,
    placements: Vec<Placement>,
    port: u16,
    events: Sender<Event>,
    status_out: Sender<Status>,
    clock: Instant,
    socket: Option<Arc<UdpSocket>>,
    running: Arc<AtomicBool>,
    identity: Identity,
    paired: HashMap<Id, SavedPeer>,
    peers: HashMap<Id, Peer>,
    sessions: HashMap<Id, Session>,
    screens: (Vec<Rect>, f64),
    screens_at: u64,
    beacon_at: u64,
    status_at: u64,
    last_status: Option<Status>,
    problem: Option<String>,
    listening: bool,
    listen_at: u64,
    joined: HashSet<Ipv4Addr>,
    blocked: bool,
    networks: Vec<(Ipv4Addr, Ipv4Addr)>,
    host_name: String,
    typed: Vec<SocketAddr>,
    lookup_at: u64,
    looking: Arc<AtomicBool>,
    // The controller's side.
    cursor: Cursor,
    held_buttons: u8,
    held_keys: HashSet<u16>,
    sent_keys: HashMap<Id, HashSet<u16>>,
    locked: bool,
    pending_motion: Option<[f32; 2]>,
    /// The last packet to the computer the cursor is on carried movement.
    moving: bool,
    // The controlled side.
    here: bool,
    smoother: Smoother,
    played_keys: HashSet<u16>,
    played_buttons: u8,
    /// The cursor went home here because the controller went quiet, not because it left.
    stalled: bool,
    clip_in: Clip,
    clip_seen: Option<[u8; 32]>,
    clip_next: u32,
    /// The newest copy sent from here, and the newest coming in: older ones still on their
    /// way stop.
    sending: Arc<AtomicU32>,
    receiving: Arc<AtomicU32>,
    transfer: Option<(bool, u32, Transfer)>,
    landed: Option<Landed>,
    // Dragging files across. The controller's side:
    /// The computer the cursor is on said files are being dragged there.
    peer_dragging: Option<Id>,
    /// Files dragged home from another computer are on their way, to pick up here.
    drag_home: bool,
    /// When this computer's own drag was last checked for files, and the computer the cursor
    /// is on was last asked.
    drag_checked_at: u64,
    /// When the cursor last arrived on another computer.
    entered_at: u64,
    // The controlled side:
    /// The controller's left button is held while files it dragged here are on their way.
    drag_held: bool,
    /// Files being dragged here, to take across if the cursor goes back.
    drag_out: Option<Vec<std::path::PathBuf>>,
    /// Where the cursor was last played.
    played_at: [f64; 2],
    /// When the held buttons were last checked against the mouse itself.
    buttons_checked_at: u64,
    /// When the log last said why a push against the edge didn't cross.
    edge_told_at: Option<u64>,
    fast: bool,
    steady: bool,
    steady_at: u64,
    log: Option<std::fs::File>,
    gaps: Gaps,
    // Extended displays.
    displays: Arc<dyn Displays>,
    ability: Ability,
    ability_at: u64,
    /// Other computers' screens this computer shows as extended displays.
    extended: HashMap<Id, Extended>,
    /// This computer's screen, lent to the computer controlling it.
    lending: Option<Lending>,
    extend_serial: u32,
    /// The extended display this computer's cursor is over, whose lending computer is told
    /// where the pointer is the quick way, and where it was last said to be.
    pointer_over: Option<(Id, [i32; 2])>,
    #[cfg(test)]
    deaf: bool,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    fn new(
        data: PathBuf,
        platform: Box<dyn Platform>,
        displays: Arc<dyn Displays>,
        prefs: Prefs,
        placements: Vec<Placement>,
        port: u16,
        events: Sender<Event>,
        status_out: Sender<Status>,
    ) -> Self {
        let saved = load(&data);
        let identity = unhex::<32>(&saved.secret).map(Identity::from_secret);
        let fresh = identity.is_none();
        let identity = identity.unwrap_or_else(Identity::generate);
        let paired = saved.paired.into_iter().filter_map(|(id, peer)| Some((unhex::<8>(&id)?, peer))).collect();
        let smoothing = prefs.smoothing;
        let mut core = Self {
            data,
            platform,
            prefs,
            placements,
            port,
            events,
            status_out,
            clock: Instant::now(),
            socket: None,
            running: Arc::new(AtomicBool::new(true)),
            identity,
            paired,
            peers: HashMap::new(),
            sessions: HashMap::new(),
            screens: (Vec::new(), 1.0),
            screens_at: 0,
            beacon_at: 0,
            status_at: 0,
            last_status: None,
            problem: None,
            listening: false,
            listen_at: 0,
            joined: HashSet::new(),
            blocked: false,
            networks: local_networks(),
            host_name: computer_name(),
            typed: Vec::new(),
            lookup_at: 0,
            looking: Arc::new(AtomicBool::new(false)),
            cursor: Cursor::Home,
            held_buttons: 0,
            held_keys: HashSet::new(),
            sent_keys: HashMap::new(),
            locked: false,
            pending_motion: None,
            moving: false,
            here: false,
            smoother: Smoother::new(smoothing),
            played_keys: HashSet::new(),
            played_buttons: 0,
            stalled: false,
            clip_in: Clip::default(),
            clip_seen: None,
            clip_next: 1,
            sending: Arc::new(AtomicU32::new(0)),
            receiving: Arc::new(AtomicU32::new(0)),
            transfer: None,
            landed: None,
            peer_dragging: None,
            drag_home: false,
            drag_checked_at: 0,
            entered_at: 0,
            drag_held: false,
            drag_out: None,
            played_at: [0.0, 0.0],
            buttons_checked_at: 0,
            edge_told_at: None,
            fast: false,
            steady: false,
            steady_at: 0,
            log: None,
            gaps: Gaps::default(),
            ability: displays.ability(),
            displays,
            ability_at: 0,
            extended: HashMap::new(),
            lending: None,
            extend_serial: 0,
            pointer_over: None,
            #[cfg(test)]
            deaf: false,
        };
        if fresh {
            core.save();
        }
        core.log = open_log(&core.data);
        core.note(&format!("Roam started on {} ({:?})", core.host_name, Os::here()));
        core.screens = core.platform.screens();
        core
    }

    /// Writes a line to roam.log, so a dropout can be explained afterwards.
    fn note(&mut self, text: &str) {
        use std::io::Write;
        let Some(log) = &mut self.log else { return };
        let at = self.clock.elapsed().as_secs_f64();
        let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(log, "{wall} +{at:.3}s {text}");
    }

    fn who(&self, id: Id) -> String {
        self.peers.get(&id).map_or_else(|| hex(&id), |p| p.name.clone())
    }

    fn now(&self) -> u64 {
        self.clock.elapsed().as_micros() as u64 + 1
    }

    fn save(&self) {
        let saved = Saved {
            secret: hex(&self.identity.secret_bytes()),
            paired: self.paired.iter().map(|(id, peer)| (hex(id), peer.clone())).collect(),
        };
        let _ = std::fs::create_dir_all(&self.data);
        if let Ok(text) = serde_json::to_string_pretty(&saved) {
            let path = self.data.join("roam.json");
            let temporary = self.data.join("roam.json.tmp");
            if std::fs::write(&temporary, text).is_ok() {
                let _ = std::fs::rename(&temporary, &path);
            }
        }
    }

    fn bind(&mut self) {
        match open_socket(self.port) {
            Ok(socket) => {
                let socket = Arc::new(socket);
                let reader = socket.clone();
                let events = self.events.clone();
                let running = self.running.clone();
                let clock = self.clock;
                let _ = std::thread::Builder::new().name("roam-network".into()).spawn(move || {
                    input::urgent_thread();
                    let mut buffer = [0u8; 2048];
                    while running.load(Ordering::Relaxed) {
                        match reader.recv_from(&mut buffer) {
                            Ok((length, from)) => {
                                let now = clock.elapsed().as_micros() as u64 + 1;
                                if events.send(Event::Packet(buffer[..length].to_vec(), from, now)).is_err() {
                                    break;
                                }
                            }
                            Err(err)
                                if matches!(
                                    err.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            // Windows reports an earlier send's "port unreachable" here.
                            Err(_) => std::thread::sleep(Duration::from_millis(5)),
                        }
                    }
                });
                self.socket = Some(socket);
                self.problem = None;
                self.listen_for_bulk();
            }
            Err(err) => self.problem = Some(format!("Roam couldn't use the network: {err}")),
        }
    }

    fn run(&mut self, inbox: Receiver<Event>) {
        input::urgent_thread();
        self.bind();
        loop {
            let now = self.now();
            let wait = self.next_deadline(now).saturating_sub(now).max(100);
            match inbox.recv_timeout(Duration::from_micros(wait)) {
                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(event) => self.handle(event),
                Err(RecvTimeoutError::Timeout) => {}
            }
            // Handle everything already waiting before doing the timed work.
            loop {
                match inbox.try_recv() {
                    Ok(Event::Stop) => return self.shut_down(),
                    Ok(event) => self.handle(event),
                    Err(_) => break,
                }
            }
            self.tick();
        }
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.shut_down_displays();
        self.go_home(None);
        self.release_played();
        for (id, session) in &mut self.sessions {
            if let Some(link) = session.link() {
                link.queue(Message::Bye);
                let packet = link.packet(0, None);
                if let (Some(socket), Some(peer)) = (&self.socket, self.peers.get(id)) {
                    let _ = socket.send_to(&packet, peer.addr);
                }
            }
        }
        if self.listening {
            self.platform.stop_listening();
        }
        self.platform.fast(false);
        self.platform.steady_wifi(false);
        self.platform.reveal();
        self.running.store(false, Ordering::Relaxed);
    }

    fn handle(&mut self, event: Event) {
        match event {
            #[cfg(test)]
            Event::Packet(..) if self.deaf => {}
            Event::Packet(bytes, from, at) => self.packet(&bytes, from, at),
            Event::Input(input, at) => self.input(input, at),
            Event::Control(control) => self.control(control),
            Event::Clipboard(copied, purpose) => self.clipboard_read(copied, purpose),
            // Only the session's key opens it, so it may come from any of that computer's addresses.
            Event::Bulk { session, answer } => {
                let found = self.sessions.iter_mut().find(|(_, s)| s.id == session).map(|(id, s)| (*id, s));
                let key = found.and_then(|(peer, s)| Some((peer, s.live()?.bulk_receive())));
                let _ = answer.send(key.map(|(peer, key)| (key, self.who(peer))));
            }
            Event::Moving { incoming, id, transfer } => match transfer {
                Some(transfer) => self.transfer = Some((incoming, id, transfer)),
                None => {
                    if matches!(self.transfer, Some((i, n, _)) if i == incoming && n == id) {
                        self.transfer = None;
                    }
                }
            },
            Event::Arrived { arrived, from, slow } => self.arrived(arrived, Some((from, slow))),
            Event::ClipSet { digest, notice } => {
                self.clip_seen = Some(digest);
                if let Some(text) = notice {
                    let serial = self.landed.as_ref().map_or(1, |l| l.serial.wrapping_add(1));
                    self.landed = Some(Landed { serial, text });
                }
            }
            Event::Note(text) => self.note(&text),
            Event::Extend(event) => self.extend_event(event),
            Event::Looked { name, addresses, networks } => {
                self.host_name = name;
                self.typed = addresses;
                self.networks = networks;
            }
            Event::Stop => {}
        }
    }

    fn next_deadline(&self, now: u64) -> u64 {
        let mut next = now + 50 * MS;
        if self.pending_motion.is_some() {
            let sent = self.away_link_sent().unwrap_or(0);
            next = next.min((sent + COALESCE).max(now + 100));
        }
        if self.smoother.pending() {
            next = next.min(now + FRAME);
        }
        if let Some((peer, _)) = self.pointer_over
            && let Some(Phase::Live { link }) = self.sessions.get(&peer).map(|s| &s.phase)
            && link.pointer.is_some()
        {
            next = next.min((link.sent + COALESCE).max(now + 100));
        }
        if self.moving
            && let Some(sent) = self.away_link_sent()
        {
            next = next.min((sent + STILL).max(now + 100));
        }
        if matches!(self.cursor, Cursor::Away { .. }) || self.here {
            next = next.min(now + BUSY_EVERY / 2);
        }
        next
    }

    /// When the last packet went to the computer the cursor is on.
    fn away_link_sent(&self) -> Option<u64> {
        let Cursor::Away { peer, .. } = self.cursor else { return None };
        match &self.sessions.get(&peer)?.phase {
            Phase::Live { link } => Some(link.sent),
            _ => None,
        }
    }

    /// Looks up the computer's name and the typed addresses on another thread: either can take
    /// seconds, and the engine thread must never wait.
    fn look_up(&mut self) {
        if self.looking.swap(true, Ordering::AcqRel) {
            return;
        }
        let typed = self.prefs.addresses.clone();
        let events = self.events.clone();
        let looking = self.looking.clone();
        let spawned = std::thread::Builder::new().name("roam-lookup".into()).spawn(move || {
            let mut addresses = Vec::new();
            for address in typed {
                let with_port = if address.contains(':') { address } else { format!("{address}:{PORT}") };
                if let Ok(found) = with_port.to_socket_addrs() {
                    addresses.extend(found.filter(SocketAddr::is_ipv4));
                }
            }
            // Listing the networks takes tens of milliseconds on Windows: never on the engine
            // thread, where it would pause the cursor.
            let _ = events.send(Event::Looked { name: computer_name(), addresses, networks: local_networks() });
            looking.store(false, Ordering::Release);
        });
        if spawned.is_err() {
            self.looking.store(false, Ordering::Release);
        }
    }

    // ---------------------------------------------------------------- timed work

    fn tick(&mut self) {
        let now = self.now();
        if now >= self.screens_at {
            self.screens_at = now + SCREENS_EVERY;
            let screens = self.platform.screens();
            if screens != self.screens {
                self.screens = screens;
                self.note(&format!("this computer's screens: {}", screens_words(&self.screens.0, self.screens.1)));
                let (displays, ppu) = self.real_screens();
                for session in self.sessions.values_mut() {
                    if session.side == Side::Controlled
                        && let Some(link) = session.live()
                    {
                        link.queue(Message::Screens { ppu: ppu as f32, displays: displays.clone() });
                    }
                }
            }
        }
        let away = matches!(self.cursor, Cursor::Away { .. });
        if away || self.here {
            let what = if away { "cursor moves sent" } else { "cursor moves heard" };
            if let Some(line) = self.gaps.report(now, what) {
                let rtt = self.sessions.values_mut().filter_map(|s| s.live()).find_map(|l| l.rtt());
                let rtt = rtt.map_or(-1.0, |us| us / 1000.0);
                self.note(&format!("{line}; round trip {rtt:.1} ms"));
            }
        } else if self.gaps.since != 0 {
            self.gaps = Gaps::default();
        }
        if now >= self.lookup_at {
            self.lookup_at = now + LOOKUP_EVERY;
            self.look_up();
        }
        if now >= self.ability_at {
            self.ability_at = now + ABILITY_EVERY;
            self.ability = self.displays.ability();
        }
        self.check_extended(now);
        if matches!(self.cursor, Cursor::Away { .. }) {
            // Tell the mouse and keyboard Roam is still here, and notice if they let go.
            self.platform.alive();
            if !self.platform.still_captured() {
                self.note("the mouse and keyboard let go by themselves (Roam was too slow to check in)");
                self.go_home(None);
            }
        }
        if now >= self.beacon_at {
            self.beacon_at = now + BEACON_EVERY;
            self.beacon(None);
            self.offer_to_desk(now);
        }
        if self.here
            && let Some(at) = self.smoother.poll(now)
        {
            self.play_move(at);
        }
        self.keep_sessions(now);
        self.flush(now, false);
        let away = matches!(self.cursor, Cursor::Away { .. }) || self.here;
        let fast = away || self.sessions.values().any(Session::is_live);
        if fast != self.fast {
            self.fast = fast;
            self.platform.fast(fast);
        }
        let steady = fast && self.prefs.fast_wifi;
        if steady != self.steady || (steady && now >= self.steady_at) {
            self.steady = steady;
            self.steady_at = now + 2 * SECOND;
            self.platform.steady_wifi(steady);
        }
        self.platform.tick();
        self.listen_if_needed();
        self.check_buttons(now);
        if now >= self.status_at {
            self.status_at = now + STATUS_EVERY;
            let status = self.status(now);
            if self.last_status.as_ref() != Some(&status) {
                let _ = self.status_out.send(status.clone());
                self.last_status = Some(status);
            }
        }
    }

    fn keep_sessions(&mut self, now: u64) {
        let mut ended = Vec::new();
        for (id, session) in &mut self.sessions {
            let heard = match &mut session.phase {
                Phase::Pairing { link, .. } | Phase::Live { link } => link.heard,
                // A handshake gets a few seconds.
                _ => session.started,
            };
            if now.saturating_sub(heard) > DEAD {
                ended.push(*id);
            }
        }
        for id in ended {
            self.note(&format!("no word from {} for 8 s: session over", self.who(id)));
            // Only the network went: what's lent between them waits for it to come back.
            self.hold_displays(id);
            self.end_session(id, false);
        }
        // Stalled: bring the cursor home, let go of held keys.
        if let Cursor::Away { peer, .. } = self.cursor
            && self
                .sessions
                .get_mut(&peer)
                .and_then(Session::live)
                .is_none_or(|link| now.saturating_sub(link.heard) > STALL)
        {
            self.note(&format!("no word from {} for 1.5 s: cursor comes home", self.who(peer)));
            self.go_home(None);
        }
        if self.here {
            let controller = self.sessions.values_mut().find(|s| s.side == Side::Controlled).and_then(Session::live);
            if controller.is_none_or(|link| now.saturating_sub(link.heard) > STALL) {
                self.note("no word from the controller for 1.5 s: letting go here");
                self.leave_here();
                self.stalled = true;
            }
        }
    }

    /// Sends packets that are due: right away when `urgent`, otherwise when something is
    /// waiting or the link has been quiet long enough.
    fn flush(&mut self, now: u64, urgent: bool) {
        let away = match self.cursor {
            Cursor::Away { peer, .. } => Some(peer),
            Cursor::Home => None,
        };
        let motion = self.pending_motion.take();
        let here = self.here;
        let Some(socket) = self.socket.clone() else { return };
        let mut notes = Vec::new();
        for (id, session) in &mut self.sessions {
            let addr = session.addr;
            let pointing = self.pointer_over.is_some_and(|(over, _)| over == *id);
            let busy = away == Some(*id) || (here && session.side == Side::Controlled) || pointing;
            match &mut session.phase {
                Phase::Pairing { link, .. } | Phase::Live { link } => {
                    let mine = away == Some(*id);
                    let since = now.saturating_sub(link.sent);
                    let paced = urgent || since >= COALESCE;
                    let every = if busy { BUSY_EVERY } else { IDLE_EVERY };
                    let still = mine && self.moving && motion.is_none() && since >= STILL;
                    let send = (mine && motion.is_some() && paced)
                        || (link.pointer.is_some() && paced)
                        || (urgent && mine)
                        || link.due(now)
                        || since >= every
                        || still;
                    let moved = if mine && send { motion } else { None };
                    if mine && motion.is_some() && !send {
                        self.pending_motion = motion;
                    }
                    if mine && send {
                        self.moving = moved.is_some();
                    }
                    if moved.is_some() {
                        self.gaps.record(now);
                    }
                    if send {
                        let packet = link.packet(now, moved);
                        notes.extend(priority::mark(socket2::SockRef::from(&*socket), Some(addr), Traffic::Voice));
                        let _ = socket.send_to(&packet, addr);
                    }
                }
                Phase::Offered { ephemeral, sent } if now.saturating_sub(*sent) >= OFFER_EVERY => {
                    *sent = now;
                    let offer = Packet::Offer {
                        id: self.identity.id,
                        key: self.identity.key,
                        session: session.id,
                        commit: ephemeral.commitment(),
                    };
                    let _ = socket.send_to(&offer.encode(), addr);
                }
                _ => {}
            }
        }
        for note in notes {
            self.note(&note);
        }
    }

    // ---------------------------------------------------------------- discovery

    fn beacon(&mut self, to: Option<SocketAddr>) {
        let Some(socket) = self.socket.clone() else { return };
        let (displays, ppu) = self.real_screens();
        let busy = self.sessions.values().any(|s| s.side == Side::Controlled && s.is_live());
        let beacon = Packet::Beacon(Beacon {
            id: self.identity.id,
            key: self.identity.key,
            port: self.port,
            os: Os::here(),
            open: self.prefs.open,
            busy,
            lends: self.displays.lends(),
            draws_pointer: self.displays.draws_pointer(),
            fast_pointer: self.displays.draws_pointer(),
            udp_pictures: self.displays.lends(),
            ppu: ppu as f32,
            displays,
            name: self.name(),
        })
        .encode();
        if let Some(to) = to {
            let _ = socket.send_to(&beacon, to);
            return;
        }
        let mut targets = vec![
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, PORT)),
            SocketAddr::V4(SocketAddrV4::new(GROUP, PORT)),
        ];
        // Each network's own broadcast address as well: with a VPN or several networks, the
        // all-networks one only leaves through one of them.
        for &(ip, broadcast) in &self.networks {
            if self.joined.insert(ip) {
                let _ = socket.join_multicast_v4(&GROUP, &ip);
            }
            targets.push(SocketAddr::V4(SocketAddrV4::new(broadcast, PORT)));
        }
        targets.extend(self.typed.iter().copied());
        targets.extend(self.peers.values().map(|peer| peer.addr));
        targets.sort();
        targets.dedup();
        let mut sent = 0;
        let mut refused = 0;
        for target in &targets {
            match socket.send_to(&beacon, target) {
                Ok(_) => sent += 1,
                Err(err) if blocked(&err) => refused += 1,
                Err(_) => {}
            }
        }
        // Everything refused: the system is keeping Roam off the network.
        self.blocked = sent == 0 && refused > 0;
    }

    fn name(&self) -> String {
        let chosen = self.prefs.name.trim();
        if !chosen.is_empty() {
            return chosen.to_owned();
        }
        self.host_name.clone()
    }

    fn heard_beacon(&mut self, beacon: Beacon, from: SocketAddr, now: u64) {
        if beacon.id == self.identity.id || crypto::id_of(&beacon.key) != beacon.id {
            return;
        }
        let addr = SocketAddr::new(from.ip(), beacon.port);
        let new = !self.peers.contains_key(&beacon.id);
        let refused = self.peers.get(&beacon.id).and_then(|p| p.refused);
        let peer = Peer {
            key: beacon.key,
            name: if beacon.name.trim().is_empty() { "Computer".into() } else { beacon.name },
            os: beacon.os,
            addr,
            open: beacon.open,
            busy: beacon.busy,
            lends: beacon.lends,
            draws_pointer: beacon.draws_pointer,
            fast_pointer: beacon.fast_pointer,
            udp_pictures: beacon.udp_pictures,
            displays: beacon.displays,
            ppu: f64::from(beacon.ppu),
            seen: now,
            refused,
        };
        let renamed = self.paired.get(&beacon.id).filter(|p| p.name != peer.name && p.key == hex(&peer.key)).is_some();
        if renamed && let Some(saved) = self.paired.get_mut(&beacon.id) {
            saved.name = peer.name.clone();
        }
        self.peers.insert(beacon.id, peer);
        if renamed {
            self.save();
        }
        if new {
            // Answer straight away so both see each other within a moment.
            self.beacon(Some(addr));
            self.offer_to_desk(now);
        }
    }

    fn placed(&self) -> HashSet<Id> {
        self.placements.iter().filter_map(|p| unhex::<8>(&p.id)).filter(|id| *id != self.identity.id).collect()
    }

    /// Starts a session with each computer on the desk that is around and willing.
    fn offer_to_desk(&mut self, now: u64) {
        for id in self.placed() {
            if self.sessions.contains_key(&id) {
                continue;
            }
            let Some(peer) = self.peers.get(&id) else { continue };
            let refused_lately =
                peer.refused.is_some_and(|(why, at)| why == Refusal::Declined || now.saturating_sub(at) < 3 * SECOND);
            if now.saturating_sub(peer.seen) > GONE || !peer.open || refused_lately {
                continue;
            }
            let session = crypto::random::<4>();
            let session = u32::from_le_bytes(session) | 1;
            let addr = peer.addr;
            self.sessions.insert(
                id,
                Session {
                    id: session,
                    side: Side::Controller,
                    addr,
                    phase: Phase::Offered { ephemeral: Ephemeral::generate(), sent: 0 },
                    started: now,
                    revealed: None,
                },
            );
        }
    }

    // ---------------------------------------------------------------- the handshake

    fn packet(&mut self, bytes: &[u8], from: SocketAddr, now: u64) {
        let Some(packet) = Packet::decode(bytes) else { return };
        match packet {
            Packet::Beacon(beacon) => self.heard_beacon(beacon, from, now),
            Packet::Offer { id, key, session, commit } => self.offered(id, key, session, commit, from, now),
            Packet::Answer { id, key, session, ephemeral } => self.answered(id, key, session, ephemeral, from, now),
            Packet::Reveal { session, ephemeral } => self.revealed(session, ephemeral, from, now),
            Packet::Refuse { session, why } => {
                let found = self.sessions.iter().find(|(_, s)| s.id == session && s.side == Side::Controller);
                if let Some((&id, _)) = found {
                    self.sessions.remove(&id);
                    if let Some(peer) = self.peers.get_mut(&id) {
                        peer.refused = Some((why, now));
                    }
                }
            }
            Packet::Sealed { session, seq, body } => self.sealed(session, seq, &body, from, now),
            Packet::Reset { session } => {
                let found = self.sessions.iter().find(|(_, s)| s.id == session).map(|(id, _)| *id);
                if let Some(id) = found
                    && self.sessions.get(&id).is_some_and(|s| s.side == Side::Controller && from == s.addr)
                {
                    self.note(&format!("{} has no such session (it restarted?): starting over", self.who(id)));
                    self.end_session(id, false);
                    self.offer_to_desk(now);
                }
            }
        }
    }

    fn send(&self, packet: &Packet, to: SocketAddr) {
        if let Some(socket) = &self.socket {
            let _ = socket.send_to(&packet.encode(), to);
        }
    }

    fn offered(&mut self, id: Id, key: Key, session: u32, commitment: [u8; 32], from: SocketAddr, now: u64) {
        if crypto::id_of(&key) != id || id == self.identity.id {
            return;
        }
        if !self.prefs.open {
            return self.send(&Packet::Refuse { session, why: Refusal::Closed }, from);
        }
        // Already in this handshake: answer again (the answer may have been lost).
        if let Some(existing) = self.sessions.get(&id) {
            if existing.id == session && existing.side == Side::Controlled {
                if let Phase::Answered { ephemeral, .. } = &existing.phase {
                    let answer = Packet::Answer {
                        id: self.identity.id,
                        key: self.identity.key,
                        session,
                        ephemeral: ephemeral.key,
                    };
                    self.send(&answer, from);
                }
                return;
            }
            // We control them and hear from them: an offer now is a stray one, and taking it
            // would swap who controls whom mid-use.
            if existing.side == Side::Controller
                && let Phase::Live { link } = &existing.phase
                && now.saturating_sub(link.heard) < STALL
            {
                self.note(&format!("ignored an offer from {}, which we control", self.who(id)));
                return;
            }
            // We were offering to them at the same time: the lower id gets to control.
            if existing.side == Side::Controller && !existing.is_live() && self.identity.id < id {
                return;
            }
        }
        let other_controller = self.sessions.iter().any(|(other, s)| {
            *other != id && s.side == Side::Controlled && matches!(s.phase, Phase::Live { .. } | Phase::Pairing { .. })
        });
        if other_controller {
            return self.send(&Packet::Refuse { session, why: Refusal::Busy }, from);
        }
        if self.sessions.get(&id).is_some_and(|s| s.side == Side::Controlled) {
            // A new session from the same controller replaces the old one (it restarted).
            self.note(&format!("{} started a new session: replacing the old one", self.who(id)));
            self.end_session(id, false);
        }
        self.sessions.remove(&id);
        let ephemeral = Ephemeral::generate();
        let answer = Packet::Answer { id: self.identity.id, key: self.identity.key, session, ephemeral: ephemeral.key };
        self.send(&answer, from);
        let phase = Phase::Answered { ephemeral, commit: commitment, peer_key: key };
        self.sessions.insert(
            id,
            Session { id: session, side: Side::Controlled, addr: from, phase, started: now, revealed: None },
        );
    }

    fn answered(&mut self, id: Id, key: Key, session: u32, peer_ephemeral: Key, from: SocketAddr, now: u64) {
        if crypto::id_of(&key) != id {
            return;
        }
        let Some(existing) = self.sessions.get_mut(&id) else { return };
        if existing.id != session || existing.side != Side::Controller {
            return;
        }
        let Phase::Offered { .. } = existing.phase else {
            // Answer repeated after we moved on: reveal again.
            return self.reveal_again(id, from);
        };
        let Phase::Offered { ephemeral, .. } = std::mem::replace(&mut existing.phase, Phase::Moving) else {
            return;
        };
        let handshake = Handshake {
            identity: &self.identity,
            ephemeral: &ephemeral,
            peer_key: key,
            peer_ephemeral,
            session,
            controller: true,
        };
        let Some(agreed) = handshake.agree() else {
            self.sessions.remove(&id);
            return;
        };
        self.send(&Packet::Reveal { session, ephemeral: ephemeral.key }, from);
        let code = agreed.code;
        let link = Link::new(session, agreed, now);
        let trusted = self.trusts(id, &key);
        if let Some(existing) = self.sessions.get_mut(&id) {
            existing.addr = from;
            existing.phase = Phase::Pairing { link, code, mine: trusted, theirs: false };
            existing.revealed = Some(ephemeral.key);
        }
        if trusted && let Some(link) = self.sessions.get_mut(&id).and_then(Session::link) {
            link.queue(Message::Allowed);
        }
    }

    fn reveal_again(&self, id: Id, to: SocketAddr) {
        let Some(session) = self.sessions.get(&id) else { return };
        if let Some(ephemeral) = session.revealed {
            self.send(&Packet::Reveal { session: session.id, ephemeral }, to);
        }
    }

    fn revealed(&mut self, session: u32, peer_ephemeral: Key, from: SocketAddr, now: u64) {
        let Some((&id, existing)) =
            self.sessions.iter_mut().find(|(_, s)| s.id == session && s.side == Side::Controlled)
        else {
            return;
        };
        if existing.addr.ip() != from.ip() {
            return;
        }
        let Phase::Answered { commit: commitment, peer_key, .. } = &existing.phase else { return };
        if commit(&peer_ephemeral) != *commitment {
            return;
        }
        let peer_key = *peer_key;
        let Phase::Answered { ephemeral, .. } = std::mem::replace(&mut existing.phase, Phase::Moving) else {
            return;
        };
        let handshake = Handshake {
            identity: &self.identity,
            ephemeral: &ephemeral,
            peer_key,
            peer_ephemeral,
            session,
            controller: false,
        };
        let Some(agreed) = handshake.agree() else {
            self.sessions.remove(&id);
            return;
        };
        let code = agreed.code;
        let trusted = self.trusts(id, &peer_key);
        let mut link = Link::new(session, agreed, now);
        if trusted {
            link.queue(Message::Allowed);
        }
        if let Some(existing) = self.sessions.get_mut(&id) {
            existing.phase = Phase::Pairing { link, code, mine: trusted, theirs: false };
        }
        // Make sure we know who this is, even if their beacons don't reach us.
        if !self.peers.contains_key(&id) {
            self.peers.insert(
                id,
                Peer {
                    key: peer_key,
                    name: self.paired.get(&id).map_or_else(|| "Computer".into(), |p| p.name.clone()),
                    os: Os::Other,
                    addr: from,
                    open: false,
                    busy: false,
                    lends: false,
                    draws_pointer: false,
                    fast_pointer: false,
                    udp_pictures: false,
                    displays: Vec::new(),
                    ppu: 1.0,
                    seen: now,
                    refused: None,
                },
            );
            self.beacon(Some(from));
        }
    }

    fn trusts(&self, id: Id, key: &Key) -> bool {
        self.paired.get(&id).is_some_and(|saved| saved.key == hex(key))
    }

    fn sealed(&mut self, session: u32, seq: u64, body: &[u8], from: SocketAddr, now: u64) {
        let Some((&id, existing)) = self.sessions.iter_mut().find(|(_, s)| s.id == session) else {
            return self.send(&Packet::Reset { session }, from);
        };
        let side = existing.side;
        let Some(link) = existing.link() else { return };
        let Some(incoming) = link.receive(now, seq, body) else { return };
        // Follow the other computer if its address changed (it moved to another network).
        existing.addr = from;
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.addr = SocketAddr::new(from.ip(), peer.addr.port());
            peer.seen = now;
        }
        let controlled = side == Side::Controlled && existing.is_live();
        if let Some(pointer) = incoming.pointer {
            self.lent_pointer(id, pointer);
        }
        let mut motion = incoming.motion.map(|m| m.at);
        if controlled && self.here && motion.is_none() {
            // A packet without movement: the controller's mouse is still.
            self.smoother.heard(now, false);
        }
        // The cursor went home after a hiccup but the controller kept going: pick it back up.
        if controlled && !self.here && self.stalled && motion.is_some() {
            self.note("the controller is back: picking the cursor up again");
            self.stalled = false;
            self.arrive();
        }
        for message in incoming.messages {
            // A click or key lands where the cursor is now, not where the buffer has it.
            if controlled
                && self.here
                && matches!(message, Message::Key { .. } | Message::Button { .. } | Message::Wheel { .. })
            {
                if let Some(at) = motion.take() {
                    self.move_to(incoming.time, now, at);
                }
                if let Some(at) = self.smoother.catch_up() {
                    self.play_move(at);
                }
            }
            self.message(id, side, message, now);
        }
        if controlled
            && self.here
            && let Some(at) = motion
        {
            self.gaps.record(now);
            self.move_to(incoming.time, now, at);
        }
    }

    fn move_to(&mut self, sent: u32, now: u64, at: [f32; 2]) {
        if let Some(at) = self.smoother.push(sent, now, at) {
            self.play_move(at);
        }
    }

    fn play_move(&mut self, at: [f32; 2]) {
        self.played_at = [f64::from(at[0]), f64::from(at[1])];
        self.platform.play(Play::Move(self.played_at));
    }

    /// Another computer's cursor arrives here.
    fn arrive(&mut self) {
        self.here = true;
        self.smoother.reset();
        self.smoother.set_on(self.prefs.smoothing);
        self.platform.arrive();
    }

    fn message(&mut self, id: Id, side: Side, message: Message, now: u64) {
        let live = self.sessions.get(&id).is_some_and(Session::is_live);
        match message {
            Message::Allowed => self.allowed(id, false, now),
            Message::Bye => {
                self.note(&format!("{} said goodbye", self.who(id)));
                self.end_session(id, true);
            }
            Message::Lent { display, pixels, hz, scale } if side == Side::Controller && live => {
                self.lent(id, display, pixels, hz, scale);
            }
            Message::Unlend { display, why } if side == Side::Controller => {
                if self.extended.get(&id).is_some_and(|e| e.display == display) {
                    let failed = self.extended.get(&id).is_some_and(|e| e.state != extend::State::Running);
                    self.note(&format!("{} stopped lending its display {display}: {why}", self.who(id)));
                    if let Some(entry) = self.extended.remove(&id) {
                        if let Some(feed) = &entry.feed {
                            feed.stop();
                        }
                        if failed {
                            self.extended.insert(
                                id,
                                Extended { state: extend::State::Failed(why), feed: None, place: None, ..entry },
                            );
                        }
                    }
                }
            }
            Message::Lend { display } if side == Side::Controlled && live => self.lend(id, display),
            Message::Unlend { display, why } if side == Side::Controlled => {
                if self.lending.as_ref().is_some_and(|l| l.to == id && l.display == display) {
                    self.note(&format!("{} no longer shows its picture here: {why}", self.who(id)));
                    self.stop_lending(None);
                }
            }
            Message::Screens { ppu, displays } if side == Side::Controller => {
                let words = screens_words(&displays, f64::from(ppu));
                if let Some(peer) = self.peers.get_mut(&id) {
                    let changed = peer.displays != displays;
                    peer.ppu = f64::from(ppu).clamp(0.25, 8.0);
                    peer.displays = displays;
                    if changed {
                        self.note(&format!("{}'s screens: {words}", self.who(id)));
                    }
                }
            }
            _ if !live => {}
            // From here on: the controlled side, hearing its controller.
            _ if side != Side::Controlled => match message {
                Message::Clip { .. } => self.clip(message, side),
                Message::Dragging
                    if matches!(self.cursor, Cursor::Away { peer, .. } if peer == id) && self.held_buttons & 1 != 0 =>
                {
                    self.peer_dragging = Some(id);
                }
                _ => {}
            },
            Message::Enter { at } => {
                self.note(&format!("{}'s cursor arrived", self.who(id)));
                self.stalled = false;
                self.drag_held = false;
                self.drag_out = None;
                self.arrive();
                self.play_move(at);
            }
            Message::Leave => {
                self.note(&format!("{}'s cursor left", self.who(id)));
                self.stalled = false;
                // Files dragged here go back with the cursor: the drag here ends without a drop.
                let dragged = self.drag_out.take().filter(|_| self.played_buttons & 1 != 0);
                if dragged.is_some() {
                    self.platform.cancel_drag();
                }
                self.drag_held = false;
                self.leave_here();
                self.platform.depart();
                if let Some(paths) = dragged {
                    self.note(&format!("taking {} dragged item(s) back to {}", paths.len(), self.who(id)));
                    self.send_bulk(id, true, move || Some(Content::Dragged(paths)));
                }
            }
            Message::Drag => self.drag_held = true,
            Message::Edge => {
                if self.here
                    && self.played_buttons & 1 != 0
                    && let Some(paths) = self.platform.dragged_files(self.played_at)
                {
                    if self.drag_out.is_none() {
                        self.note(&format!("{} item(s) are being dragged here", paths.len()));
                    }
                    self.drag_out = Some(paths);
                    if let Some(link) = self.sessions.get_mut(&id).and_then(Session::live) {
                        link.queue(Message::Dragging);
                    }
                }
            }
            Message::Key { hid, down } => {
                if down {
                    self.played_keys.insert(hid);
                } else {
                    self.played_keys.remove(&hid);
                }
                self.platform.play(Play::Key { hid, down });
            }
            Message::Button { button, down } if button < BUTTONS => {
                if button == 0 {
                    if !down {
                        self.drag_held = false;
                        self.drag_out = None;
                    }
                    self.platform.left_button(down);
                }
                if down {
                    self.played_buttons |= 1 << button;
                } else if self.played_buttons & (1 << button) == 0 {
                    // Pressed before the cursor came over: it was never pressed here.
                    return;
                } else {
                    self.played_buttons &= !(1 << button);
                }
                self.platform.play(Play::Button { button, down });
            }
            Message::Wheel { dx, dy } => self.platform.play(Play::Wheel { dx, dy }),
            message @ Message::Clip { .. } => self.clip(message, side),
            _ => {}
        }
    }

    /// One side said yes: ours (`mine`) or theirs.
    fn allowed(&mut self, id: Id, mine: bool, now: u64) {
        let Some(session) = self.sessions.get_mut(&id) else { return };
        let Phase::Pairing { mine: our_yes, theirs, .. } = &mut session.phase else { return };
        if mine {
            if *our_yes {
                return;
            }
            *our_yes = true;
            if let Some(link) = session.link() {
                link.queue(Message::Allowed);
            }
        } else {
            *theirs = true;
        }
        let Phase::Pairing { mine: true, theirs: true, .. } = session.phase else { return };
        if let Phase::Pairing { link, .. } = std::mem::replace(&mut session.phase, Phase::Moving) {
            session.phase = Phase::Live { link };
        }
        let side = session.side;
        let role = if side == Side::Controller { "we control it" } else { "it controls us" };
        let addr = session.addr;
        self.note(&format!("connected to {} at {addr}: {role}", self.who(id)));
        // Remember them for next time.
        if let Some(peer) = self.peers.get(&id) {
            let saved = SavedPeer { key: hex(&peer.key), name: peer.name.clone() };
            let changed = self.paired.get(&id).is_none_or(|old| old.key != saved.key);
            self.paired.insert(id, saved);
            if changed {
                self.save();
            }
        }
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.refused = None;
        }
        if side == Side::Controlled {
            let (displays, ppu) = self.real_screens();
            if let Some(link) = self.sessions.get_mut(&id).and_then(Session::live) {
                link.queue(Message::Screens { ppu: ppu as f32, displays });
            }
        }
        self.flush(now, true);
    }

    fn end_session(&mut self, id: Id, they_ended: bool) {
        if let Cursor::Away { peer, .. } = self.cursor
            && peer == id
        {
            self.go_home(None);
        }
        self.session_ended_displays(id);
        let Some(mut session) = self.sessions.remove(&id) else { return };
        if session.side == Side::Controlled {
            self.stalled = false;
            self.leave_here();
            self.platform.reveal();
        }
        if !they_ended && let Some(link) = session.link() {
            link.queue(Message::Bye);
            let packet = link.packet(self.now(), None);
            if let Some(socket) = &self.socket {
                let _ = socket.send_to(&packet, session.addr);
            }
        }
        self.sent_keys.remove(&id);
    }

    // ---------------------------------------------------------------- the controller's cursor

    fn listen_if_needed(&mut self) {
        let wanted = self.sessions.values().any(|s| s.side == Side::Controller && s.is_live());
        if wanted == self.listening {
            return;
        }
        let now = self.now();
        if wanted && now < self.listen_at {
            return;
        }
        if wanted {
            let events = self.events.clone();
            let clock = self.clock;
            let sink = Box::new(move |input: Input| {
                let now = clock.elapsed().as_micros() as u64 + 1;
                let _ = events.send(Event::Input(input, now));
            });
            match self.platform.listen(sink) {
                Ok(()) => {
                    self.listening = true;
                    self.note("listening to this computer's mouse and keyboard");
                }
                Err(err) => {
                    self.note(&format!("can't listen to the mouse and keyboard: {err}"));
                    self.problem = Some(err);
                    // Try again in a while, in case the permission was given.
                    self.listen_at = now + 2 * SECOND;
                    return;
                }
            }
        } else {
            self.go_home(None);
            self.platform.stop_listening();
            self.listening = false;
            self.held_buttons = 0;
            self.note("stopped listening to the mouse and keyboard");
        }
        self.problem = None;
    }

    /// The desk as it stands: this computer and every connected computer on it.
    fn desk(&self) -> (Vec<Computer>, Vec<Option<Id>>) {
        let mut desk = Vec::new();
        let mut ids = Vec::new();
        let mine = hex(&self.identity.id);
        let my_spot = self.placements.iter().find(|p| p.id == mine).map_or([0.0, 0.0], |p| p.at);
        let me = self.me_on_desk(my_spot);
        desk.push(me.clone());
        ids.push(None);
        for placement in &self.placements {
            let Some(id) = unhex::<8>(&placement.id) else { continue };
            if id == self.identity.id
                || !self.sessions.get(&id).is_some_and(|s| s.side == Side::Controller && s.is_live())
            {
                continue;
            }
            let Some(peer) = self.peers.get(&id) else { continue };
            if peer.displays.is_empty() {
                continue;
            }
            let Some(computer) =
                self.peer_on_desk(id, placement.id.clone(), peer.displays.clone(), peer.ppu, placement.at, &me)
            else {
                continue;
            };
            desk.push(computer);
            ids.push(Some(id));
        }
        (desk, ids)
    }

    /// Says in the log, now and then, why pushing against the edge of the screen at home went
    /// nowhere.
    fn edge_unused(&mut self, at: [f64; 2], delta: [f64; 2], why: &str, now: u64) {
        if self.edge_told_at.is_some_and(|at| now < at + EDGE_TOLD_EVERY) {
            return;
        }
        let (displays, ppu) = self.screens.clone();
        if !layout::pushing(&Computer::new("me", displays, ppu, [0.0, 0.0]), at, delta) {
            return;
        }
        self.edge_told_at = Some(now);
        self.note(&format!("pushed against the edge at ({:.0}, {:.0}), but {why}", at[0], at[1]));
    }

    /// Lets go of buttons the mouse itself no longer holds: their release went somewhere Roam
    /// couldn't hear it, such as an app running as administrator.
    fn check_buttons(&mut self, now: u64) {
        // Files dragged home are held by a press made while the cursor was away, which the
        // system never saw.
        if self.held_buttons == 0
            || self.drag_home
            || !self.listening
            || self.cursor != Cursor::Home
            || now < self.buttons_checked_at + BUTTONS_CHECK_EVERY
        {
            return;
        }
        self.buttons_checked_at = now;
        let Some(real) = self.platform.buttons_held() else { return };
        let stale = self.held_buttons & !real;
        if stale != 0 {
            self.note(&format!("buttons {stale:05b} were let go without Roam hearing it: no longer held"));
            self.held_buttons &= real;
            if stale & 1 != 0 {
                self.peer_dragging = None;
                self.drag_home = false;
                self.platform.left_button(false);
            }
        }
    }

    fn corner_for(&self, os: Os) -> f64 {
        if self.prefs.corners && os == Os::Mac { CORNER } else { 0.0 }
    }

    fn input(&mut self, input: Input, now: u64) {
        match input {
            Input::Button { button, down } if button < BUTTONS => {
                if down {
                    self.held_buttons |= 1 << button;
                } else {
                    self.held_buttons &= !(1 << button);
                }
                if button == 0 {
                    if !down {
                        self.peer_dragging = None;
                        self.drag_home = false;
                    }
                    if self.cursor == Cursor::Home {
                        self.platform.left_button(down);
                    }
                }
            }
            Input::Key { hid, down } => {
                if down {
                    self.held_keys.insert(hid);
                } else {
                    self.held_keys.remove(&hid);
                }
            }
            _ => {}
        }
        match self.cursor {
            Cursor::Home => {
                let Input::Move { at, delta } = input else { return };
                self.quick_pointer(Some(at));
                if !self.prefs.roam {
                    return;
                }
                // Only the left button may be held across, and only while dragging files.
                let dragging = self.held_buttons == 1;
                if (self.held_buttons != 0 && !dragging) || self.locked {
                    let why = if self.locked {
                        "the cursor is locked here".to_owned()
                    } else {
                        format!("a mouse button is held (buttons {:05b})", self.held_buttons)
                    };
                    return self.edge_unused(at, delta, &why, now);
                }
                let (desk, ids) = self.desk();
                if desk.len() < 2 {
                    let placed = self.placed().len();
                    let why = format!("no connected computer on the desk ({placed} other computer(s) placed)");
                    return self.edge_unused(at, delta, &why, now);
                }
                let corner = self.corner_for(Os::here());
                let step = layout::push(&desk, 0, at, delta, corner);
                if step.is_none() && !dragging {
                    let why = "no other computer's screen is across from it on the desk";
                    self.edge_unused(at, delta, why, now);
                }
                if let Some(Step::Cross { to, at: arrive }) = step
                    && let Some(&Some(peer)) = ids.get(to)
                {
                    if !dragging {
                        return self.enter(peer, arrive, at, now);
                    }
                    if now < self.drag_checked_at + DRAG_CHECK_EVERY {
                        return;
                    }
                    self.drag_checked_at = now;
                    let Some(paths) = self.platform.dragged_files(at) else { return };
                    // The files go across instead of landing here.
                    self.platform.cancel_drag();
                    self.enter(peer, arrive, at, now);
                    self.note(&format!("dragging {} item(s) over to {}", paths.len(), self.who(peer)));
                    if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
                        link.queue(Message::Drag);
                    }
                    self.send_bulk(peer, true, move || Some(Content::Dragged(paths)));
                }
            }
            Cursor::Away { peer, at, left_from } => self.away_input(input, peer, at, left_from, now),
        }
    }

    fn away_input(&mut self, input: Input, peer: Id, at: [f64; 2], left_from: [f64; 2], now: u64) {
        match input {
            Input::Move { delta, .. } => {
                let (desk, ids) = self.desk();
                let Some(on) = ids.iter().position(|id| *id == Some(peer)) else {
                    return self.go_home(None);
                };
                let mine = desk[0].ppu;
                let theirs = desk[on].ppu;
                let scaled = [delta[0] / mine * theirs, delta[1] / mine * theirs];
                let os = self.peers.get(&peer).map_or(Os::Other, |p| p.os);
                let step = layout::step(&desk, on, at, scaled, self.corner_for(os));
                let step = match step {
                    // Files dragged there may come back home with the cursor.
                    Step::Cross { to, .. }
                        if self.held_buttons == 1
                            && !self.locked
                            && self.peer_dragging == Some(peer)
                            && ids.get(to).copied().flatten().is_none() =>
                    {
                        self.drag_home = true;
                        step
                    }
                    // Just arrived: it stays, rather than bouncing straight back.
                    Step::Cross { .. } if now < self.entered_at + SETTLE_AFTER_CROSSING => {
                        Step::Stay(desk[on].onto_displays([at[0] + scaled[0], at[1] + scaled[1]]))
                    }
                    // No dragging across otherwise: a held button keeps the cursor where the drag
                    // started.
                    Step::Cross { to, .. } if self.held_buttons != 0 || self.locked => {
                        let home = ids.get(to).copied().flatten().is_none();
                        if home
                            && self.held_buttons == 1
                            && !self.locked
                            && now >= self.drag_checked_at + DRAG_CHECK_EVERY
                        {
                            // Ask whether it's files being dragged there.
                            self.drag_checked_at = now;
                            if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
                                link.queue(Message::Edge);
                            }
                            self.flush(now, true);
                        }
                        let to = [at[0] + scaled[0], at[1] + scaled[1]];
                        Step::Stay(desk[on].onto_displays(to))
                    }
                    step => step,
                };
                match step {
                    Step::Stay(next) => {
                        self.cursor = Cursor::Away { peer, at: next, left_from };
                        if next != at {
                            // Sent as soon as the pacing allows (see `flush`).
                            self.pending_motion = Some([next[0] as f32, next[1] as f32]);
                        }
                    }
                    Step::Cross { to, at: arrive } => match ids.get(to).copied().flatten() {
                        None => self.go_home(Some(arrive)),
                        Some(next) => {
                            self.leave_peer(peer);
                            self.cursor = Cursor::Home;
                            self.enter(next, arrive, left_from, now);
                        }
                    },
                }
            }
            Input::Key { hid, down } => {
                let ctrl = self.held_keys.contains(&keys::LEFT_CTRL) || self.held_keys.contains(&keys::RIGHT_CTRL);
                let alt = self.held_keys.contains(&keys::LEFT_ALT) || self.held_keys.contains(&keys::RIGHT_ALT);
                let shift = self.held_keys.contains(&keys::LEFT_SHIFT) || self.held_keys.contains(&keys::RIGHT_SHIFT);
                if down && ctrl && alt && shift && hid == keys::H {
                    return self.go_home(None);
                }
                if down && ctrl && alt && shift && hid == keys::L {
                    self.locked = !self.locked;
                    return;
                }
                let them = self.peers.get(&peer).map_or(Os::Other, |p| p.os);
                let differ = them != Os::here() && them != Os::Other && Os::here() != Os::Other;
                let hid = if differ { self.prefs.modifiers.apply(hid) } else { hid };
                let sent = self.sent_keys.entry(peer).or_default();
                if down {
                    sent.insert(hid);
                } else if !sent.remove(&hid) {
                    return; // pressed before the cursor came over: it was let go here
                }
                self.queue_to(peer, Message::Key { hid, down }, now);
            }
            Input::Button { button, down } => self.queue_to(peer, Message::Button { button, down }, now),
            Input::Wheel { dx, dy } => {
                let speed = self.scroll_speed(peer);
                self.queue_to(peer, Message::Wheel { dx: dx * speed, dy: dy * speed }, now);
            }
            Input::Swipe(swipe) => {
                let os = self.peers.get(&peer).map_or(Os::Other, |p| p.os);
                self.note(&format!("trackpad swipe {swipe:?} for {}", self.who(peer)));
                for (hid, down) in swipe_keys(swipe, os) {
                    self.queue_to(peer, Message::Key { hid, down }, now);
                }
            }
        }
    }

    /// The scroll multiplier set for a computer on the desk.
    fn scroll_speed(&self, peer: Id) -> f32 {
        let id = hex(&peer);
        self.placements.iter().find(|p| p.id == id).map_or(1.0, |p| p.scroll as f32)
    }

    fn queue_to(&mut self, peer: Id, message: Message, now: u64) {
        if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
            link.queue(message);
        }
        self.flush(now, true);
    }

    fn enter(&mut self, peer: Id, arrive: [f64; 2], left_from: [f64; 2], now: u64) {
        // Off this computer's screens, virtual ones too.
        self.quick_pointer(None);
        let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) else { return };
        link.queue(Message::Enter { at: [arrive[0] as f32, arrive[1] as f32] });
        let rtt = link.rtt().map_or(-1.0, |us| us / 1000.0);
        self.note(&format!(
            "cursor over to {} at ({:.0}, {:.0}) from ({:.0}, {:.0}) (round trip {rtt:.1} ms)",
            self.who(peer),
            arrive[0],
            arrive[1],
            left_from[0],
            left_from[1]
        ));
        if self.cursor == Cursor::Home {
            self.platform.capture(true);
        }
        self.entered_at = now;
        self.cursor = Cursor::Away { peer, at: arrive, left_from };
        self.flush(now, true);
        if self.prefs.clipboard {
            self.read_clipboard(Purpose::Entering(peer));
        }
    }

    fn leave_peer(&mut self, peer: Id) {
        self.sent_keys.remove(&peer);
        if let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) {
            link.queue(Message::Leave);
        }
    }

    /// Brings the cursor back to this computer, at `at` or where it left.
    fn go_home(&mut self, at: Option<[f64; 2]>) {
        let Cursor::Away { peer, left_from, at: was } = self.cursor else { return };
        let home = at.unwrap_or(left_from);
        self.note(&format!(
            "cursor home from {} at ({:.0}, {:.0}), back at ({:.0}, {:.0})",
            self.who(peer),
            was[0],
            was[1],
            home[0],
            home[1]
        ));
        self.leave_peer(peer);
        self.cursor = Cursor::Home;
        self.pending_motion = None;
        self.moving = false;
        self.platform.warp(home);
        self.platform.capture(false);
        let now = self.now();
        self.flush(now, true);
    }

    // ---------------------------------------------------------------- the controlled side

    fn leave_here(&mut self) {
        if !self.here {
            return;
        }
        self.here = false;
        self.smoother.reset();
        self.release_played();
        if self.prefs.clipboard {
            self.read_clipboard(Purpose::Leaving);
        }
    }

    /// Lets go of everything the controller was holding down here.
    fn release_played(&mut self) {
        for hid in std::mem::take(&mut self.played_keys) {
            self.platform.play(Play::Key { hid, down: false });
        }
        for button in 0..BUTTONS {
            if self.played_buttons & (1 << button) != 0 {
                self.platform.play(Play::Button { button, down: false });
            }
        }
        self.played_buttons = 0;
    }

    // ---------------------------------------------------------------- the clipboard

    fn read_clipboard(&self, purpose: Purpose) {
        let events = self.events.clone();
        let _ = std::thread::Builder::new().name("roam-clipboard".into()).spawn(move || {
            let copied = input::clipboard().map(|copied| {
                let digest = copied.digest();
                (copied, digest)
            });
            let _ = events.send(Event::Clipboard(copied, purpose));
        });
    }

    fn clipboard_read(&mut self, copied: Option<(Copied, [u8; 32])>, purpose: Purpose) {
        let Some((copied, digest)) = copied else { return };
        if self.clip_seen == Some(digest) {
            return;
        }
        let to = match purpose {
            Purpose::Entering(peer) if matches!(self.cursor, Cursor::Away { peer: on, .. } if on == peer) => Some(peer),
            Purpose::Leaving => {
                self.sessions.iter().find(|(_, s)| s.side == Side::Controlled && s.is_live()).map(|(id, _)| *id)
            }
            _ => None,
        };
        let Some(to) = to else { return };
        if !self.sessions.get(&to).is_some_and(Session::is_live) {
            return;
        }
        self.clip_seen = Some(digest);
        match copied {
            // Short text goes on the link itself: there's no connection to make.
            Copied::Text(text) if text.len() <= SHORT_TEXT => {
                let id = self.next_copy(false);
                let Some(link) = self.sessions.get_mut(&to).and_then(Session::live) else { return };
                let chunks: Vec<&[u8]> = text.as_bytes().chunks(CLIP_CHUNK).collect();
                let total = chunks.len() as u16;
                for (index, chunk) in chunks.into_iter().enumerate() {
                    link.queue(Message::Clip { id, index: index as u16, total, bytes: chunk.to_vec() });
                }
            }
            copied => self.send_bulk(to, false, move || {
                Some(match copied {
                    Copied::Text(text) => Content::Text(text),
                    Copied::Image { width, height, rgba } => Content::Image(input::png(width, height, &rgba)?),
                    Copied::Files(paths) => Content::Files(paths),
                })
            }),
        }
    }

    /// Numbers a new copy going out. A clipboard stops an older one that's still going over; a
    /// drag is marked as one, so no clipboard stops it.
    fn next_copy(&mut self, drag: bool) -> u32 {
        let id = (self.clip_next & !DRAG_ID) | if drag { DRAG_ID } else { 0 };
        self.clip_next = self.clip_next.wrapping_add(1);
        if !drag {
            self.sending.store(id, Ordering::Relaxed);
        }
        id
    }

    /// Sends what `make` makes to a computer in a live session, over TCP on a thread of its own.
    fn send_bulk(&mut self, to: Id, drag: bool, make: impl FnOnce() -> Option<Content> + Send + 'static) {
        let id = self.next_copy(drag);
        let newest = if drag { Arc::new(AtomicU32::new(id)) } else { self.sending.clone() };
        let who = self.who(to);
        let Some(session) = self.sessions.get_mut(&to) else { return };
        let addr = session.addr;
        let session_id = session.id;
        let Some(link) = session.live() else { return };
        let key = link.bulk_send();
        let events = self.events.clone();
        let _ = std::thread::Builder::new().name("roam-send".into()).spawn(move || {
            let Some(content) = make() else { return };
            let what = match &content {
                Content::Text(_) => "text",
                Content::Image(_) => "an image",
                Content::Files(_) => "files",
                Content::Dragged(_) => "dragged files",
            };
            let started = Instant::now();
            let mut progress = reporter(&events, false, id, who.clone(), &newest);
            let sent = bulk::send(addr, Hello { session: session_id, id }, key, &content, &mut progress);
            let line = match sent {
                Ok(()) => format!("sent {what} to {who} in {} ms", started.elapsed().as_millis()),
                Err(err) => format!("couldn't send {what} to {who}: {err}"),
            };
            let _ = events.send(Event::Moving { incoming: false, id, transfer: None });
            let _ = events.send(Event::Note(line));
        });
    }

    /// Takes big clipboards from the computers in a session with this one, over TCP on the
    /// same port number as the UDP socket.
    fn listen_for_bulk(&mut self) {
        let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, self.port)) {
            Ok(listener) => listener,
            Err(err) => return self.note(&format!("images and files can't come in: {err}")),
        };
        if listener.set_nonblocking(true).is_err() {
            return;
        }
        let events = self.events.clone();
        let running = self.running.clone();
        let newest = self.receiving.clone();
        let inbox = self.data.join("received");
        let _ = std::thread::Builder::new().name("roam-bulk".into()).spawn(move || {
            while running.load(Ordering::Relaxed) {
                let Ok((stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                };
                let events = events.clone();
                let inbox = inbox.clone();
                let newest = newest.clone();
                let _ = std::thread::Builder::new().name("roam-receive".into()).spawn(move || {
                    let _ = receive_bulk(stream, &events, &inbox, &newest);
                });
            }
        });
    }

    /// Puts what came over on the clipboard, or picks up files dragged across while the button is
    /// still held. `told` names the computer it came from over TCP, and whether it took long
    /// enough to say it's here.
    fn arrived(&mut self, arrived: Arrived, told: Option<(String, bool)>) {
        let arrived = match arrived {
            Arrived::Dragged(paths) => {
                let held = if self.here {
                    self.drag_held && self.played_buttons & 1 == 0
                } else {
                    self.cursor == Cursor::Home && self.drag_home && self.held_buttons & 1 != 0
                };
                let played = self.here;
                self.drag_held = false;
                self.drag_home = false;
                if held && self.platform.drag_in(paths.clone(), played) {
                    if played {
                        self.played_buttons |= 1;
                    }
                    self.note(&format!("picked up {} dragged item(s)", paths.len()));
                    // Their copy is still there to paste if the drop goes wrong.
                    return;
                }
                // The button was let go before they arrived: they're ready to paste instead.
                Arrived::Files(paths)
            }
            arrived => arrived,
        };
        let what = match &arrived {
            Arrived::Text(_) => "Text".to_owned(),
            Arrived::Image(_) => "An image".to_owned(),
            Arrived::Files(paths) | Arrived::Dragged(paths) => {
                let names: Vec<String> =
                    paths.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                bulk::label_of(&names)
            }
        };
        let notice = match (&arrived, told) {
            (Arrived::Files(_), Some((from, _))) | (_, Some((from, true))) => {
                Some(format!("{what} from {from} is ready to paste"))
            }
            _ => None,
        };
        let copied = match arrived {
            Arrived::Text(text) => Copied::Text(text),
            Arrived::Image(png) => match input::unpng(&png) {
                Some(image) => image,
                None => return,
            },
            Arrived::Files(paths) | Arrived::Dragged(paths) => Copied::Files(paths),
        };
        // Not sent back when the cursor leaves, even before the clipboard is set.
        self.clip_seen = Some(copied.digest());
        let events = self.events.clone();
        let inbox = self.data.join("received");
        let _ = std::thread::Builder::new().name("roam-paste".into()).spawn(move || {
            if let Some(digest) = input::set_clipboard(&copied) {
                let _ = events.send(Event::ClipSet { digest, notice });
            }
            bulk::tidy(&inbox, KEEP_RECEIVED);
        });
    }

    fn clip(&mut self, message: Message, _side: Side) {
        let Message::Clip { id, index, total, bytes } = message else { return };
        if total == 0 || usize::from(total) * CLIP_CHUNK > MOST_CLIPBOARD + CLIP_CHUNK {
            return;
        }
        if self.clip_in.id != id || self.clip_in.total != total {
            self.clip_in = Clip { id, total, parts: vec![None; usize::from(total)] };
        }
        if let Some(part) = self.clip_in.parts.get_mut(usize::from(index)) {
            *part = Some(bytes);
        }
        if self.clip_in.parts.iter().all(Option::is_some) {
            let bytes: Vec<u8> = std::mem::take(&mut self.clip_in.parts).into_iter().flatten().flatten().collect();
            self.clip_in = Clip::default();
            // Newer than any big copy still coming in, which stops.
            self.receiving.store(id, Ordering::Relaxed);
            if let Ok(text) = String::from_utf8(bytes) {
                self.arrived(Arrived::Text(text), None);
            }
        }
    }

    // ---------------------------------------------------------------- what the notch shows

    fn control(&mut self, control: Control) {
        let now = self.now();
        match control {
            Control::Prefs(prefs) => {
                let closed = self.prefs.open && !prefs.open;
                if !prefs.roam && matches!(self.cursor, Cursor::Away { .. }) {
                    self.note("roaming turned off: the cursor comes home");
                    self.go_home(None);
                }
                self.smoother.set_on(prefs.smoothing);
                let retype = self.prefs.addresses != prefs.addresses || self.prefs.name != prefs.name;
                self.prefs = prefs;
                if retype {
                    self.look_up();
                }
                if closed {
                    let controlled: Vec<Id> =
                        self.sessions.iter().filter(|(_, s)| s.side == Side::Controlled).map(|(id, _)| *id).collect();
                    for id in controlled {
                        self.end_session(id, false);
                    }
                }
                self.beacon(None);
            }
            Control::Desk(placements) => {
                if placements != self.placements {
                    let spots: Vec<String> = placements
                        .iter()
                        .map(|p| {
                            let who = match unhex::<8>(&p.id) {
                                Some(id) if id == self.identity.id => "this computer".to_owned(),
                                Some(id) => self.who(id),
                                None => p.id.clone(),
                            };
                            format!("{who} at ({:.0}, {:.0})", p.at[0], p.at[1])
                        })
                        .collect();
                    self.note(&format!("the desk: {}", spots.join(", ")));
                }
                self.placements = placements;
                self.place_extended();
                let placed = self.placed();
                let gone: Vec<Id> = self
                    .sessions
                    .iter()
                    .filter(|(id, s)| s.side == Side::Controller && !placed.contains(*id))
                    .map(|(id, _)| *id)
                    .collect();
                for id in gone {
                    self.end_session(id, false);
                }
                self.offer_to_desk(now);
            }
            Control::Allow(id) => {
                if let Some(id) = unhex::<8>(&id) {
                    self.allowed(id, true, now);
                }
            }
            Control::Decline(id) => {
                if let Some(id) = unhex::<8>(&id) {
                    let side = self.sessions.get(&id).map(|s| (s.side, s.id, s.addr));
                    if let Some((side, session, addr)) = side {
                        if side == Side::Controlled {
                            self.send(&Packet::Refuse { session, why: Refusal::Declined }, addr);
                        }
                        self.sessions.remove(&id);
                        if side == Side::Controller
                            && let Some(peer) = self.peers.get_mut(&id)
                        {
                            peer.refused = Some((Refusal::Declined, now));
                        }
                    }
                }
            }
            Control::Retry(id) => {
                if let Some(peer) = unhex::<8>(&id).and_then(|id| self.peers.get_mut(&id)) {
                    peer.refused = None;
                }
                self.offer_to_desk(now);
            }
            Control::Forget(id) => {
                if let Some(id) = unhex::<8>(&id) {
                    self.end_session(id, false);
                    if self.paired.remove(&id).is_some() {
                        self.save();
                    }
                }
            }
            Control::Home => self.go_home(None),
            Control::Extend { id, display } => {
                if let Some(id) = unhex::<8>(&id) {
                    self.extend(id, display);
                }
            }
            Control::Unextend(id) => {
                if let Some(id) = unhex::<8>(&id) {
                    let why = format!("Disconnected on {}", self.name());
                    self.unextend(id, &why);
                }
            }
            Control::StopLending => {
                let why = format!("Stopped on {}", self.name());
                self.stop_lending(Some(&why));
            }
            Control::AskPermission => {
                self.platform.ask_permission();
                self.listen_at = 0;
            }
            #[cfg(test)]
            Control::Deaf(deaf) => self.deaf = deaf,
            #[cfg(test)]
            Control::CutStream => self.cut_lent_stream(),
            Control::Release => {
                let controlled: Vec<Id> =
                    self.sessions.iter().filter(|(_, s)| s.side == Side::Controlled).map(|(id, _)| *id).collect();
                for id in controlled {
                    self.end_session(id, false);
                }
            }
        }
        self.status_at = 0;
    }

    fn status(&mut self, now: u64) -> Status {
        let (displays, ppu) = self.real_screens();
        let me = PeerView {
            id: hex(&self.identity.id),
            name: self.name(),
            os: Os::here(),
            displays,
            ppu,
            open: self.prefs.open,
            lends: self.displays.lends(),
            nearby: true,
            paired: true,
            link: LinkState::None,
            rtt: None,
        };
        let mut peers: Vec<PeerView> = Vec::new();
        let mut asks = Vec::new();
        let mut controller = None;
        let mut ids: Vec<Id> = self.peers.keys().copied().collect();
        ids.sort_by_key(|id| self.peers.get(id).map(|p| p.name.to_lowercase()));
        for id in ids {
            let Some(peer) = self.peers.get(&id) else { continue };
            let session = self.sessions.get_mut(&id);
            let (link, rtt) = match session {
                None => match peer.refused {
                    Some((Refusal::Declined, _)) => (LinkState::Declined, None),
                    Some((Refusal::Busy, _)) => (LinkState::Busy, None),
                    Some((Refusal::Closed, _)) => (LinkState::Closed, None),
                    None => (LinkState::None, None),
                },
                Some(session) => {
                    let side = session.side;
                    let state = match &mut session.phase {
                        Phase::Offered { .. } | Phase::Answered { .. } | Phase::Moving => LinkState::Connecting,
                        Phase::Pairing { code, mine, .. } => {
                            if !*mine {
                                asks.push(Ask {
                                    id: hex(&id),
                                    name: peer.name.clone(),
                                    code: crypto::format_code(*code),
                                    controlling: side == Side::Controller,
                                });
                            }
                            LinkState::Pairing
                        }
                        Phase::Live { .. } => {
                            if side == Side::Controlled {
                                controller = Some(peer.name.clone());
                            }
                            LinkState::Connected
                        }
                    };
                    let rtt = session.link().and_then(|l| l.rtt()).map(|us| (us / 1000.0) as f32);
                    (state, rtt)
                }
            };
            let nearby = now.saturating_sub(peer.seen) <= GONE;
            if !nearby && link == LinkState::None {
                continue;
            }
            peers.push(PeerView {
                id: hex(&id),
                name: peer.name.clone(),
                os: peer.os,
                displays: peer.displays.clone(),
                ppu: peer.ppu,
                open: peer.open && !peer.busy,
                lends: peer.lends,
                nearby,
                paired: self.paired.contains_key(&id),
                link,
                rtt,
            });
        }
        let away_on = match self.cursor {
            Cursor::Away { peer, .. } => self.peers.get(&peer).map(|p| p.name.clone()),
            Cursor::Home => None,
        };
        let problem = self.problem.clone().or_else(|| self.platform.missing_permission()).or_else(|| {
            self.blocked.then(|| {
                if cfg!(target_os = "macos") {
                    "Allow notchbase in Privacy & Security › Local Network so Roam can find your other computers".into()
                } else {
                    "Roam can't reach the network. Check that a firewall allows notchbase".into()
                }
            })
        });
        let addresses =
            self.networks.iter().map(|(ip, _)| ip.to_string()).filter(|ip| !ip.starts_with("169.254.")).collect();
        Status {
            me: Some(me),
            peers,
            controller,
            away_on,
            here: self.here,
            locked: self.locked,
            asks,
            problem,
            addresses,
            transfer: self.transfer.as_ref().map(|(_, _, t)| t.clone()),
            landed: self.landed.clone(),
            ability: self.ability.clone(),
            extended: self.extended_views(),
            lending: self.lending_view(),
        }
    }
}

/// roam.log in the data folder, started afresh when it grows past a megabyte.
fn open_log(data: &std::path::Path) -> Option<std::fs::File> {
    let path = data.join("roam.log");
    let big = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20);
    if big {
        let _ = std::fs::rename(&path, data.join("roam.old.log"));
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

fn load(data: &std::path::Path) -> Saved {
    std::fs::read_to_string(data.join("roam.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// This computer's IPv4 networks: its address on each and that network's broadcast address.
fn local_networks() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else { return Vec::new() };
    interfaces
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() && v4.netmask != Ipv4Addr::BROADCAST => {
                let broadcast =
                    v4.broadcast.unwrap_or_else(|| Ipv4Addr::from(u32::from(v4.ip) | !u32::from(v4.netmask)));
                Some((v4.ip, broadcast))
            }
            _ => None,
        })
        .collect()
}

/// Whether a send failed because the system won't let Roam on the network.
fn blocked(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::HostUnreachable)
        || matches!(err.raw_os_error(), Some(1 | 13 | 65))
}

/// The shortcut that does what a trackpad swipe does on a Mac, on a computer of kind `os`: the
/// keys pressed in order and let go in reverse.
fn swipe_keys(swipe: Swipe, os: Os) -> Vec<(u16, bool)> {
    use keys::{D, DOWN, LEFT, LEFT_CTRL, LEFT_GUI, RIGHT, TAB, UP};
    let held: &[u16] = match (os, swipe) {
        // Windows' own touchpad: three fingers up is Task View, down shows the desktop, and
        // four fingers sideways switch desktops.
        (Os::Windows, Swipe::Up) => &[LEFT_GUI, TAB],
        (Os::Windows, Swipe::Down) => &[LEFT_GUI, D],
        (Os::Windows, Swipe::Left) => &[LEFT_CTRL, LEFT_GUI, LEFT],
        (Os::Windows, Swipe::Right) => &[LEFT_CTRL, LEFT_GUI, RIGHT],
        // Another Mac: the keyboard shortcuts for the same things.
        (_, Swipe::Up) => &[LEFT_CTRL, UP],
        (_, Swipe::Down) => &[LEFT_CTRL, DOWN],
        (_, Swipe::Left) => &[LEFT_CTRL, LEFT],
        (_, Swipe::Right) => &[LEFT_CTRL, RIGHT],
    };
    held.iter().map(|&hid| (hid, true)).chain(held.iter().rev().map(|&hid| (hid, false))).collect()
}

/// Screens for the log: each one's place and size, and the units per point.
fn screens_words(displays: &[Rect], ppu: f64) -> String {
    let each: Vec<String> =
        displays.iter().map(|d| format!("{:.0}x{:.0} at ({:.0}, {:.0})", d.w, d.h, d.x, d.y)).collect();
    format!("{} at {ppu}x", if each.is_empty() { "none".to_owned() } else { each.join(", ") })
}

/// Copies quicker than this aren't shown going over.
const SHOW_AFTER: Duration = Duration::from_millis(400);
/// How often the engine hears how a copy is going.
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// Tells the engine how a copy is going, now and then, and stops it once a newer one has
/// started (`newest` is no longer `id`).
fn reporter<'a>(
    events: &'a Sender<Event>,
    incoming: bool,
    id: u32,
    with: String,
    newest: &'a AtomicU32,
) -> impl FnMut(u64, u64, &str) -> bool + 'a {
    let started = Instant::now();
    let mut told: Option<Instant> = None;
    move |done, total, what| {
        let due = match told {
            Some(at) => at.elapsed() >= REPORT_EVERY,
            None => started.elapsed() >= SHOW_AFTER,
        };
        if due {
            told = Some(Instant::now());
            let transfer = Transfer { incoming, with: with.clone(), what: what.to_owned(), done, total };
            let _ = events.send(Event::Moving { incoming, id, transfer: Some(transfer) });
        }
        newest.load(Ordering::Relaxed) == id
    }
}

/// One incoming transfer, on its own thread: checks it belongs to a live session, then takes
/// it in.
fn receive_bulk(
    mut stream: std::net::TcpStream,
    events: &Sender<Event>,
    inbox: &std::path::Path,
    newest: &AtomicU32,
) -> std::io::Result<()> {
    use std::io::Read as _;
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut header = [0u8; 12];
    stream.read_exact(&mut header)?;
    if let Some(hello) = display::stream::Hello::parse(&header) {
        return extend::view(stream, hello, events);
    }
    let hello = Hello::parse(&header)?;
    let (answer, key) = mpsc::channel();
    events.send(Event::Bulk { session: hello.session, answer }).map_err(std::io::Error::other)?;
    let Ok(Some((key, from))) = key.recv_timeout(Duration::from_secs(2)) else { return Ok(()) };
    // The newest copy wins: an older one still coming in stops. Dragged files aren't a copy.
    let drag = hello.id & DRAG_ID != 0;
    let own = AtomicU32::new(hello.id);
    let newest = if drag { &own } else { newest };
    newest.store(hello.id, Ordering::Relaxed);
    let started = Instant::now();
    let mut progress = reporter(events, true, hello.id, from.clone(), newest);
    let received = bulk::receive(stream, hello, key, inbox, &mut progress);
    let _ = events.send(Event::Moving { incoming: true, id: hello.id, transfer: None });
    match received {
        Ok(arrived) if newest.load(Ordering::Relaxed) == hello.id => {
            let took = started.elapsed();
            let _ = events.send(Event::Note(format!("a clipboard came in from {from}, in {} ms", took.as_millis())));
            let _ = events.send(Event::Arrived { arrived, from, slow: took >= Duration::from_secs(1) });
        }
        Ok(_) => {}
        Err(err) => {
            let _ = events.send(Event::Note(format!("a clipboard didn't come in whole: {err}")));
        }
    }
    Ok(())
}

fn open_socket(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // Lets Roam start again straight away after a restart. Windows frees UDP ports at once
    // and would let another program share the port, so it isn't asked to there.
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.set_broadcast(true)?;
    // Expedited forwarding: Wi-Fi puts these packets in its voice queue, ahead of downloads
    // (where the system honours it; `priority` does the rest as each packet goes).
    let _ = socket.set_tos_v4(Traffic::Voice.tos());
    let _ = socket.set_recv_buffer_size(1 << 20);
    socket.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    let socket: UdpSocket = socket.into();
    let _ = socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED);
    let _ = socket.set_multicast_loop_v4(false);
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(socket)
}

/// The computer's name as people know it: "John's MacBook Pro" rather than
/// "Johns-MacBook-Pro.local".
pub fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]).output()
        && output.status.success()
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let host = host.trim_end_matches(".local").trim_end_matches(".lan");
    let words = host.replace(['-', '_'], " ");
    if words.trim().is_empty() { "Computer".into() } else { words.trim().to_owned() }
}

#[cfg(test)]
mod tests;
