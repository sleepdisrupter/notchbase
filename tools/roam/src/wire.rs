//! What goes over the network. Everything is UDP: a lost packet never holds up the ones behind
//! it, which is what makes a TCP link (Deskflow's) stall for a few hundred milliseconds on a
//! busy Wi-Fi network.
//!
//! Every packet starts with `RM`, the protocol version and a kind. Beacons and the handshake are
//! in the clear; everything after the handshake is sealed with the session's keys.

use crate::layout::Rect;

pub const MAGIC: [u8; 2] = *b"RM";
pub const VERSION: u8 = 1;
/// Room for a packet on any network without fragmenting.
pub const MAX_PACKET: usize = 1200;

pub type Id = [u8; 8];
pub type Key = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Other,
}

impl Os {
    pub fn here() -> Self {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Other
        }
    }

    fn code(self) -> u8 {
        match self {
            Os::Mac => 1,
            Os::Windows => 2,
            Os::Other => 0,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Os::Mac,
            2 => Os::Windows,
            _ => Os::Other,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Os::Mac => "Mac",
            Os::Windows => "Windows",
            Os::Other => "Computer",
        }
    }
}

/// A computer saying it's there, about once a second.
#[derive(Clone, Debug, PartialEq)]
pub struct Beacon {
    pub id: Id,
    pub key: Key,
    pub port: u16,
    pub os: Os,
    /// Open to being controlled.
    pub open: bool,
    /// Already controlled by another computer.
    pub busy: bool,
    /// Can lend its screens to another computer as extended displays.
    pub lends: bool,
    /// Draws the host's pointer itself on a screen it lends, from news sent apart.
    pub draws_pointer: bool,
    pub ppu: f32,
    pub displays: Vec<Rect>,
    pub name: String,
}

/// Why a computer said no.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Busy,
    Closed,
    Declined,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Beacon(Beacon),
    /// The controlling computer asks to start a session, committing to its ephemeral key.
    Offer {
        id: Id,
        key: Key,
        session: u32,
        commit: [u8; 32],
    },
    /// The other computer's ephemeral key.
    Answer {
        id: Id,
        key: Key,
        session: u32,
        ephemeral: Key,
    },
    /// The controlling computer reveals the key it committed to.
    Reveal {
        session: u32,
        ephemeral: Key,
    },
    Refuse {
        session: u32,
        why: Refusal,
    },
    /// Sealed session traffic.
    Sealed {
        session: u32,
        seq: u64,
        body: Vec<u8>,
    },
    /// "I don't know that session": start over.
    Reset {
        session: u32,
    },
}

const BEACON: u8 = 1;
const OFFER: u8 = 2;
const ANSWER: u8 = 3;
const REVEAL: u8 = 4;
const REFUSE: u8 = 5;
const SEALED: u8 = 6;
const RESET: u8 = 7;

/// The bytes sealed packets authenticate as associated data: the header up to the body.
pub fn sealed_header(session: u32, seq: u64) -> [u8; 16] {
    let mut header = [0; 16];
    header[..2].copy_from_slice(&MAGIC);
    header[2] = VERSION;
    header[3] = SEALED;
    header[4..8].copy_from_slice(&session.to_le_bytes());
    header[8..16].copy_from_slice(&seq.to_le_bytes());
    header
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Writer::default();
        out.bytes(&MAGIC);
        out.u8(VERSION);
        match self {
            Packet::Beacon(beacon) => {
                out.u8(BEACON);
                out.bytes(&beacon.id);
                out.bytes(&beacon.key);
                out.u16(beacon.port);
                out.u8(beacon.os.code());
                out.u8(u8::from(beacon.open)
                    | (u8::from(beacon.busy) << 1)
                    | (u8::from(beacon.lends) << 2)
                    | (u8::from(beacon.draws_pointer) << 3));
                out.f32(beacon.ppu);
                let displays = &beacon.displays[..beacon.displays.len().min(16)];
                out.u8(displays.len() as u8);
                for display in displays {
                    out.rect(display);
                }
                out.text(&beacon.name, 64);
            }
            Packet::Offer { id, key, session, commit } => {
                out.u8(OFFER);
                out.bytes(id);
                out.bytes(key);
                out.u32(*session);
                out.bytes(commit);
            }
            Packet::Answer { id, key, session, ephemeral } => {
                out.u8(ANSWER);
                out.bytes(id);
                out.bytes(key);
                out.u32(*session);
                out.bytes(ephemeral);
            }
            Packet::Reveal { session, ephemeral } => {
                out.u8(REVEAL);
                out.u32(*session);
                out.bytes(ephemeral);
            }
            Packet::Refuse { session, why } => {
                out.u8(REFUSE);
                out.u32(*session);
                out.u8(match why {
                    Refusal::Busy => 1,
                    Refusal::Closed => 2,
                    Refusal::Declined => 3,
                });
            }
            Packet::Sealed { session, seq, body } => {
                out.u8(SEALED);
                out.u32(*session);
                out.u64(*seq);
                out.bytes(body);
            }
            Packet::Reset { session } => {
                out.u8(RESET);
                out.u32(*session);
            }
        }
        out.0
    }

    pub fn decode(bytes: &[u8]) -> Option<Packet> {
        let mut input = Reader(bytes);
        if input.take(2)? != MAGIC || input.u8()? != VERSION {
            return None;
        }
        let packet = match input.u8()? {
            BEACON => {
                let id = input.array()?;
                let key = input.array()?;
                let port = input.u16()?;
                let os = Os::from_code(input.u8()?);
                let flags = input.u8()?;
                let ppu = input.f32()?;
                let count = input.u8()? as usize;
                let displays = (0..count.min(16)).map(|_| input.rect()).collect::<Option<Vec<_>>>()?;
                let name = input.text()?;
                let ppu = if ppu.is_finite() && ppu > 0.1 && ppu < 10.0 { ppu } else { 1.0 };
                Packet::Beacon(Beacon {
                    id,
                    key,
                    port,
                    os,
                    open: flags & 1 != 0,
                    busy: flags & 2 != 0,
                    lends: flags & 4 != 0,
                    draws_pointer: flags & 8 != 0,
                    ppu,
                    displays,
                    name,
                })
            }
            OFFER => {
                Packet::Offer { id: input.array()?, key: input.array()?, session: input.u32()?, commit: input.array()? }
            }
            ANSWER => Packet::Answer {
                id: input.array()?,
                key: input.array()?,
                session: input.u32()?,
                ephemeral: input.array()?,
            },
            REVEAL => Packet::Reveal { session: input.u32()?, ephemeral: input.array()? },
            REFUSE => {
                let session = input.u32()?;
                let why = match input.u8()? {
                    1 => Refusal::Busy,
                    2 => Refusal::Closed,
                    _ => Refusal::Declined,
                };
                Packet::Refuse { session, why }
            }
            SEALED => Packet::Sealed { session: input.u32()?, seq: input.u64()?, body: input.0.to_vec() },
            RESET => Packet::Reset { session: input.u32()? },
            _ => return None,
        };
        Some(packet)
    }
}

/// Mouse buttons, in the order most mice number them.
pub const BUTTONS: u8 = 5;

/// Something that has to arrive, in order: lost ones are sent again.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The cursor arrives, here (in the receiver's units).
    Enter {
        at: [f32; 2],
    },
    /// The cursor leaves: let go of everything held down.
    Leave,
    Key {
        hid: u16,
        down: bool,
    },
    Button {
        button: u8,
        down: bool,
    },
    /// Scroll, 120 to a notch of a wheel, positive is up and right.
    Wheel {
        dx: f32,
        dy: f32,
    },
    /// One piece of a clipboard's text.
    Clip {
        id: u32,
        index: u16,
        total: u16,
        bytes: Vec<u8>,
    },
    /// The receiver's displays changed.
    Screens {
        ppu: f32,
        displays: Vec<Rect>,
    },
    /// The controlled computer's person allowed it.
    Allowed,
    /// The session is over.
    Bye,
    /// The cursor came over with the left button held, dragging files: they follow, to be
    /// picked up there if the button is still held when they arrive.
    Drag,
    /// The cursor is held against the edge on the way back, with the left button down: say
    /// whether files are being dragged.
    Edge,
    /// Files are being dragged here: the cursor may take them back across.
    Dragging,
    /// From the controller: show its picture on this display of yours, as its extended display.
    /// Only sent to computers whose beacon says they lend their screens.
    Lend {
        display: u8,
    },
    /// The display will show it: its size in pixels, how often it refreshes, and its pixels
    /// per point.
    Lent {
        display: u8,
        pixels: [u16; 2],
        hz: u8,
        scale: f32,
    },
    /// Either side: the display is no longer lent, with why, in a few words.
    Unlend {
        display: u8,
        why: String,
    },
}

const ENTER: u8 = 1;
const LEAVE: u8 = 2;
const KEY: u8 = 3;
const BUTTON: u8 = 4;
const WHEEL: u8 = 5;
const CLIP: u8 = 6;
const SCREENS: u8 = 7;
const ALLOWED: u8 = 8;
const BYE: u8 = 9;
const DRAG: u8 = 10;
const EDGE: u8 = 11;
const DRAGGING: u8 = 12;
const LEND: u8 = 13;
const LENT: u8 = 14;
const UNLEND: u8 = 15;

/// How much of a clipboard goes in one message.
pub const CLIP_CHUNK: usize = 900;

impl Message {
    pub fn encode(&self, out: &mut Vec<u8>) {
        let mut w = Writer(std::mem::take(out));
        match self {
            Message::Enter { at } => {
                w.u8(ENTER);
                w.f32(at[0]);
                w.f32(at[1]);
            }
            Message::Leave => w.u8(LEAVE),
            Message::Key { hid, down } => {
                w.u8(KEY);
                w.u16(*hid);
                w.u8(u8::from(*down));
            }
            Message::Button { button, down } => {
                w.u8(BUTTON);
                w.u8(*button);
                w.u8(u8::from(*down));
            }
            Message::Wheel { dx, dy } => {
                w.u8(WHEEL);
                w.f32(*dx);
                w.f32(*dy);
            }
            Message::Clip { id, index, total, bytes } => {
                w.u8(CLIP);
                w.u32(*id);
                w.u16(*index);
                w.u16(*total);
                let bytes = &bytes[..bytes.len().min(CLIP_CHUNK)];
                w.u16(bytes.len() as u16);
                w.bytes(bytes);
            }
            Message::Screens { ppu, displays } => {
                w.u8(SCREENS);
                w.f32(*ppu);
                let displays = &displays[..displays.len().min(16)];
                w.u8(displays.len() as u8);
                for display in displays {
                    w.rect(display);
                }
            }
            Message::Allowed => w.u8(ALLOWED),
            Message::Bye => w.u8(BYE),
            Message::Drag => w.u8(DRAG),
            Message::Edge => w.u8(EDGE),
            Message::Dragging => w.u8(DRAGGING),
            Message::Lend { display } => {
                w.u8(LEND);
                w.u8(*display);
            }
            Message::Lent { display, pixels, hz, scale } => {
                w.u8(LENT);
                w.u8(*display);
                w.u16(pixels[0]);
                w.u16(pixels[1]);
                w.u8(*hz);
                w.f32(*scale);
            }
            Message::Unlend { display, why } => {
                w.u8(UNLEND);
                w.u8(*display);
                w.text(why, 160);
            }
        }
        *out = w.0;
    }

    pub fn size(&self) -> usize {
        let mut out = Vec::new();
        self.encode(&mut out);
        out.len()
    }

    fn decode(input: &mut Reader) -> Option<Message> {
        Some(match input.u8()? {
            ENTER => Message::Enter { at: [input.f32()?, input.f32()?] },
            LEAVE => Message::Leave,
            KEY => Message::Key { hid: input.u16()?, down: input.u8()? != 0 },
            BUTTON => Message::Button { button: input.u8()?, down: input.u8()? != 0 },
            WHEEL => Message::Wheel { dx: input.f32()?, dy: input.f32()? },
            CLIP => {
                let id = input.u32()?;
                let index = input.u16()?;
                let total = input.u16()?;
                let length = input.u16()? as usize;
                Message::Clip { id, index, total, bytes: input.take(length)?.to_vec() }
            }
            SCREENS => {
                let ppu = input.f32()?;
                let count = input.u8()? as usize;
                let displays = (0..count.min(16)).map(|_| input.rect()).collect::<Option<Vec<_>>>()?;
                Message::Screens { ppu, displays }
            }
            ALLOWED => Message::Allowed,
            BYE => Message::Bye,
            DRAG => Message::Drag,
            EDGE => Message::Edge,
            DRAGGING => Message::Dragging,
            LEND => Message::Lend { display: input.u8()? },
            LENT => Message::Lent {
                display: input.u8()?,
                pixels: [input.u16()?, input.u16()?],
                hz: input.u8()?,
                scale: input.f32()?,
            },
            UNLEND => Message::Unlend { display: input.u8()?, why: input.text()? },
            _ => return None,
        })
    }
}

/// Where the cursor is: sent in every packet while it moves, newest wins, so a lost one is
/// simply replaced by the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub seq: u32,
    pub at: [f32; 2],
}

/// The inside of a sealed packet.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Body {
    /// Every reliable message below this number arrived.
    pub ack: u32,
    /// The sender's clock, in microseconds.
    pub time: u32,
    /// The last `time` the sender heard from us, and how long it held it before this packet.
    pub echo: u32,
    pub held: u32,
    pub motion: Option<Motion>,
    /// Reliable messages, each with its number.
    pub messages: Vec<(u32, Message)>,
}

impl Body {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u32(self.ack);
        w.u32(self.time);
        w.u32(self.echo);
        w.u32(self.held);
        match self.motion {
            Some(motion) => {
                w.u8(1);
                w.u32(motion.seq);
                w.f32(motion.at[0]);
                w.f32(motion.at[1]);
            }
            None => w.u8(0),
        }
        w.u8(self.messages.len().min(255) as u8);
        let mut out = w.0;
        for (seq, message) in self.messages.iter().take(255) {
            out.extend_from_slice(&seq.to_le_bytes());
            message.encode(&mut out);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Body> {
        let mut input = Reader(bytes);
        let ack = input.u32()?;
        let time = input.u32()?;
        let echo = input.u32()?;
        let held = input.u32()?;
        let motion = match input.u8()? {
            0 => None,
            _ => Some(Motion { seq: input.u32()?, at: [input.f32()?, input.f32()?] }),
        };
        let count = input.u8()?;
        let mut messages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let seq = input.u32()?;
            messages.push((seq, Message::decode(&mut input)?));
        }
        Some(Body { ack, time, echo, held, motion, messages })
    }
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn f32(&mut self, value: f32) {
        self.bytes(&value.to_le_bytes());
    }
    fn rect(&mut self, rect: &Rect) {
        for value in [rect.x, rect.y, rect.w, rect.h] {
            self.f32(value as f32);
        }
    }
    fn text(&mut self, text: &str, most: usize) {
        let mut end = text.len().min(most);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.bytes(&text.as_bytes()[..end]);
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (head, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(head)
    }
    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.array()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.array()?))
    }
    fn rect(&mut self) -> Option<Rect> {
        let [x, y, w, h] = [self.f32()?, self.f32()?, self.f32()?, self.f32()?].map(f64::from);
        (x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
            .then_some(Rect::new(x, y, w, h))
    }
    fn text(&mut self) -> Option<String> {
        let length = self.u8()? as usize;
        Some(String::from_utf8_lossy(self.take(length)?).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_round_trip() {
        let packets = [
            Packet::Beacon(Beacon {
                id: [1; 8],
                key: [2; 32],
                port: 47911,
                os: Os::Windows,
                open: true,
                busy: false,
                lends: true,
                draws_pointer: true,
                ppu: 1.5,
                displays: vec![Rect::new(-1920.0, 0.0, 1920.0, 1080.0), Rect::new(0.0, 0.0, 2560.0, 1440.0)],
                name: "Desk PC".into(),
            }),
            Packet::Offer { id: [3; 8], key: [4; 32], session: 9, commit: [5; 32] },
            Packet::Answer { id: [3; 8], key: [4; 32], session: 9, ephemeral: [6; 32] },
            Packet::Reveal { session: 9, ephemeral: [7; 32] },
            Packet::Refuse { session: 9, why: Refusal::Busy },
            Packet::Sealed { session: 9, seq: 77, body: vec![1, 2, 3] },
            Packet::Reset { session: 4 },
        ];
        for packet in packets {
            assert_eq!(Packet::decode(&packet.encode()), Some(packet));
        }
    }

    #[test]
    fn bodies_round_trip() {
        let body = Body {
            ack: 4,
            time: 1000,
            echo: 900,
            held: 30,
            motion: Some(Motion { seq: 12, at: [100.5, -20.25] }),
            messages: vec![
                (4, Message::Enter { at: [1.0, 2.0] }),
                (5, Message::Key { hid: 4, down: true }),
                (6, Message::Button { button: 1, down: false }),
                (7, Message::Wheel { dx: 0.0, dy: -120.0 }),
                (8, Message::Clip { id: 1, index: 0, total: 1, bytes: b"hello".to_vec() }),
                (9, Message::Screens { ppu: 2.0, displays: vec![Rect::new(0.0, 0.0, 10.0, 10.0)] }),
                (10, Message::Leave),
                (11, Message::Allowed),
                (12, Message::Bye),
                (13, Message::Drag),
                (14, Message::Edge),
                (15, Message::Dragging),
                (16, Message::Lend { display: 1 }),
                (17, Message::Lent { display: 1, pixels: [2560, 1440], hz: 60, scale: 1.0 }),
                (18, Message::Unlend { display: 1, why: "Stopped on the Mac".into() }),
            ],
        };
        assert_eq!(Body::decode(&body.encode()), Some(body));
    }

    #[test]
    fn junk_is_ignored() {
        assert_eq!(Packet::decode(b""), None);
        assert_eq!(Packet::decode(b"XX\x01\x01"), None);
        assert_eq!(Packet::decode(&[b'R', b'M', VERSION, BEACON, 1, 2]), None);
        assert_eq!(Body::decode(&[0; 5]), None);
    }

    #[test]
    fn long_names_are_cut_on_a_character() {
        let beacon = Beacon {
            id: [0; 8],
            key: [0; 32],
            port: 1,
            os: Os::Mac,
            open: false,
            busy: true,
            lends: false,
            draws_pointer: false,
            ppu: 1.0,
            displays: vec![],
            name: "é".repeat(40),
        };
        let Some(Packet::Beacon(back)) = Packet::decode(&Packet::Beacon(beacon).encode()) else { panic!() };
        assert_eq!(back.name, "é".repeat(32));
        assert!(back.busy && !back.open);
    }
}
