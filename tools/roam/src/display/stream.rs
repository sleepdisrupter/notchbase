//! The connection that carries a lent screen's pictures. The host connects to the lending
//! computer over TCP, on the port Roam's big clipboards use, and says so with its own magic.
//! Pictures go one way and the lending computer's mouse and keyboard the other, each sealed
//! with its own key, derived from the session's: only the computer in that session can open
//! them or send them.
//!
//! Every message is `[length u32][sealed]`; inside, a kind and what goes with it.

use std::io::{self, Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

use super::{Picture, PointerAt, PointerShape, Remote};
use crate::crypto::hash;

pub const MAGIC: &[u8; 4] = b"RDS1";
/// The most one message may hold: a key picture of a busy 4K screen, with room to spare.
const MOST: usize = 24 * 1024 * 1024;

const SIZE: u8 = 1;
const PICTURE: u8 = 2;
const BYE: u8 = 3;
const INPUT: u8 = 4;
const KEY_PLEASE: u8 = 5;
// Only to a lending computer that says it draws the pointer: an older one ends the stream at
// a kind it doesn't know.
const POINTER: u8 = 6;
const SHAPE: u8 = 7;

/// The clear start of the connection: the session, and a number for this stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hello {
    pub session: u32,
    pub stream: u32,
}

impl Hello {
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let mut header = [0u8; 12];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&self.session.to_le_bytes());
        header[8..].copy_from_slice(&self.stream.to_le_bytes());
        out.write_all(&header)
    }

    /// From the 12 bytes after which a connection says what it is, if they say it's a stream.
    pub fn parse(header: &[u8; 12]) -> Option<Hello> {
        if &header[..4] != MAGIC {
            return None;
        }
        let word = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
        Some(Hello { session: word(4), stream: word(8) })
    }
}

/// The keys for both directions, from the session's key for big clipboards from host to viewer.
pub fn keys(bulk: [u8; 32]) -> Keys {
    Keys { pictures: hash(&[b"roam display pictures", &bulk]), input: hash(&[b"roam display input", &bulk]) }
}

#[derive(Clone, Copy)]
pub struct Keys {
    pub pictures: [u8; 32],
    pub input: [u8; 32],
}

/// What goes from host to viewer.
#[derive(Clone, Debug, PartialEq)]
pub enum Down {
    /// The pictures that follow are this many pixels.
    Size([u32; 2]),
    Picture(Picture),
    /// The host's pointer moved.
    Pointer(PointerAt),
    /// The host's pointer changed shape.
    Shape(PointerShape),
    Bye,
}

/// What goes from viewer to host.
#[derive(Clone, Debug, PartialEq)]
pub enum Up {
    Input(Remote),
    /// A picture didn't decode: start again from a key picture.
    KeyPlease,
    Bye,
}

fn nonce(stream: u32, count: u64) -> Nonce {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&stream.to_le_bytes());
    nonce[4..].copy_from_slice(&count.to_le_bytes());
    nonce.into()
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_owned())
}

/// Seals messages in order.
pub struct Sealer {
    aead: ChaCha20Poly1305,
    stream: u32,
    count: u64,
}

impl Sealer {
    pub fn new(key: [u8; 32], stream: u32) -> Self {
        Self { aead: ChaCha20Poly1305::new(&key.into()), stream, count: 0 }
    }

    fn send(&mut self, out: &mut impl Write, plain: &[u8]) -> io::Result<()> {
        let sealed = self
            .aead
            .encrypt(&nonce(self.stream, self.count), Payload { msg: plain, aad: MAGIC })
            .map_err(|_| io::Error::other("couldn't seal"))?;
        self.count += 1;
        let mut framed = Vec::with_capacity(sealed.len() + 4);
        framed.extend_from_slice(&(sealed.len() as u32).to_le_bytes());
        framed.extend_from_slice(&sealed);
        out.write_all(&framed)
    }

    pub fn down(&mut self, out: &mut impl Write, message: &Down) -> io::Result<()> {
        let plain = match message {
            Down::Size([w, h]) => [&[SIZE][..], &w.to_le_bytes(), &h.to_le_bytes()].concat(),
            Down::Picture(picture) => {
                let mut plain = Vec::with_capacity(picture.data.len() + 2);
                plain.push(PICTURE);
                plain.push(u8::from(picture.key));
                plain.extend_from_slice(&picture.data);
                plain
            }
            Down::Pointer(PointerAt { at: [x, y], visible }) => {
                [&[POINTER][..], &x.to_le_bytes(), &y.to_le_bytes(), &[u8::from(*visible)]].concat()
            }
            Down::Shape(PointerShape { size: [w, h], rgba }) => {
                [&[SHAPE][..], &w.to_le_bytes(), &h.to_le_bytes(), rgba].concat()
            }
            Down::Bye => vec![BYE],
        };
        self.send(out, &plain)
    }

    pub fn up(&mut self, out: &mut impl Write, message: &Up) -> io::Result<()> {
        let mut plain = Vec::with_capacity(16);
        match message {
            Up::Input(remote) => {
                plain.push(INPUT);
                match *remote {
                    Remote::Move([x, y]) => {
                        plain.push(1);
                        plain.extend_from_slice(&x.to_le_bytes());
                        plain.extend_from_slice(&y.to_le_bytes());
                    }
                    Remote::Button { button, down } => plain.extend_from_slice(&[2, button, u8::from(down)]),
                    Remote::Wheel { dx, dy } => {
                        plain.push(3);
                        plain.extend_from_slice(&dx.to_le_bytes());
                        plain.extend_from_slice(&dy.to_le_bytes());
                    }
                    Remote::Key { hid, down } => {
                        plain.push(4);
                        plain.extend_from_slice(&hid.to_le_bytes());
                        plain.push(u8::from(down));
                    }
                }
            }
            Up::KeyPlease => plain.push(KEY_PLEASE),
            Up::Bye => plain.push(BYE),
        }
        self.send(out, &plain)
    }
}

/// Opens messages in order: a message out of place or tampered with ends the stream.
pub struct Opener {
    aead: ChaCha20Poly1305,
    stream: u32,
    count: u64,
}

impl Opener {
    pub fn new(key: [u8; 32], stream: u32) -> Self {
        Self { aead: ChaCha20Poly1305::new(&key.into()), stream, count: 0 }
    }

    fn open(&mut self, input: &mut impl Read) -> io::Result<Vec<u8>> {
        let mut length = [0u8; 4];
        input.read_exact(&mut length)?;
        let length = u32::from_le_bytes(length) as usize;
        if length > MOST + 64 {
            return Err(invalid("a message too big"));
        }
        let mut sealed = vec![0u8; length];
        input.read_exact(&mut sealed)?;
        let plain = self
            .aead
            .decrypt(&nonce(self.stream, self.count), Payload { msg: &sealed, aad: MAGIC })
            .map_err(|_| invalid("a message that isn't genuine"))?;
        self.count += 1;
        if plain.is_empty() {
            return Err(invalid("an empty message"));
        }
        Ok(plain)
    }

    pub fn down(&mut self, input: &mut impl Read) -> io::Result<Down> {
        let plain = self.open(input)?;
        let word = |at: usize| plain.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        Ok(match plain[0] {
            SIZE => Down::Size([word(1).ok_or_else(|| invalid("short"))?, word(5).ok_or_else(|| invalid("short"))?]),
            PICTURE if plain.len() >= 2 => Down::Picture(Picture { key: plain[1] != 0, data: plain[2..].to_vec() }),
            POINTER => {
                let short = || invalid("short");
                let at = |at: usize| word(at).map(|w| w as i32).ok_or_else(short);
                Down::Pointer(PointerAt { at: [at(1)?, at(5)?], visible: *plain.get(9).ok_or_else(short)? != 0 })
            }
            SHAPE => {
                let size = [word(1).ok_or_else(|| invalid("short"))?, word(5).ok_or_else(|| invalid("short"))?];
                let shape = PointerShape { size, rgba: plain.get(9..).unwrap_or_default().to_vec() };
                if !shape.sound() {
                    return Err(invalid("a pointer that doesn't add up"));
                }
                Down::Shape(shape)
            }
            BYE => Down::Bye,
            _ => return Err(invalid("an unknown message")),
        })
    }

    pub fn up(&mut self, input: &mut impl Read) -> io::Result<Up> {
        let plain = self.open(input)?;
        let float = |at: usize| plain.get(at..at + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let short = || invalid("short");
        Ok(match plain[0] {
            INPUT => Up::Input(match plain.get(1) {
                Some(1) => Remote::Move([float(2).ok_or_else(short)?, float(6).ok_or_else(short)?]),
                Some(2) => Remote::Button {
                    button: *plain.get(2).ok_or_else(short)?,
                    down: *plain.get(3).ok_or_else(short)? != 0,
                },
                Some(3) => Remote::Wheel { dx: float(2).ok_or_else(short)?, dy: float(6).ok_or_else(short)? },
                Some(4) => Remote::Key {
                    hid: u16::from_le_bytes([*plain.get(2).ok_or_else(short)?, *plain.get(3).ok_or_else(short)?]),
                    down: *plain.get(4).ok_or_else(short)? != 0,
                },
                _ => return Err(invalid("an unknown input")),
            }),
            KEY_PLEASE => Up::KeyPlease,
            BYE => Up::Bye,
            _ => return Err(invalid("an unknown message")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hellos_are_told_apart_from_clipboards() {
        let mut out = Vec::new();
        Hello { session: 7, stream: 9 }.write(&mut out).unwrap();
        let header: [u8; 12] = out.try_into().unwrap();
        assert_eq!(Hello::parse(&header), Some(Hello { session: 7, stream: 9 }));
        let mut clipboard = header;
        clipboard[..4].copy_from_slice(b"RBK1");
        assert_eq!(Hello::parse(&clipboard), None);
    }

    #[test]
    fn both_directions_round_trip() {
        let keys = keys([3; 32]);
        let mut wire = Vec::new();
        let mut sealer = Sealer::new(keys.pictures, 5);
        let downs = [
            Down::Size([2560, 1440]),
            Down::Picture(Picture { key: true, data: vec![0, 0, 0, 1, 0x67, 9, 9] }),
            Down::Picture(Picture { key: false, data: vec![] }),
            Down::Pointer(PointerAt { at: [-3, 1079], visible: true }),
            Down::Pointer(PointerAt { at: [0, 0], visible: false }),
            Down::Shape(PointerShape { size: [2, 1], rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] }),
            Down::Bye,
        ];
        for down in &downs {
            sealer.down(&mut wire, down).unwrap();
        }
        let mut reading = &wire[..];
        let mut opener = Opener::new(keys.pictures, 5);
        for down in &downs {
            assert_eq!(&opener.down(&mut reading).unwrap(), down);
        }
        let ups = [
            Up::Input(Remote::Move([0.25, 0.75])),
            Up::Input(Remote::Button { button: 1, down: true }),
            Up::Input(Remote::Wheel { dx: 0.0, dy: -120.0 }),
            Up::Input(Remote::Key { hid: 0x1234, down: false }),
            Up::KeyPlease,
            Up::Bye,
        ];
        let mut wire = Vec::new();
        let mut sealer = Sealer::new(keys.input, 5);
        for up in &ups {
            sealer.up(&mut wire, up).unwrap();
        }
        let mut reading = &wire[..];
        let mut opener = Opener::new(keys.input, 5);
        for up in &ups {
            assert_eq!(&opener.up(&mut reading).unwrap(), up);
        }
    }

    #[test]
    fn a_pointer_shape_short_of_its_pixels_is_refused() {
        let keys = keys([3; 32]);
        let mut wire = Vec::new();
        let mut sealer = Sealer::new(keys.pictures, 5);
        sealer.send(&mut wire, &[&[SHAPE][..], &4u32.to_le_bytes(), &4u32.to_le_bytes(), &[0; 63]].concat()).unwrap();
        assert!(Opener::new(keys.pictures, 5).down(&mut &wire[..]).is_err());
    }

    #[test]
    fn the_wrong_key_or_a_replay_is_refused() {
        let keys = keys([3; 32]);
        let mut wire = Vec::new();
        Sealer::new(keys.pictures, 5).down(&mut wire, &Down::Bye).unwrap();
        assert!(Opener::new(keys.input, 5).down(&mut &wire[..]).is_err(), "the other direction's key");
        assert!(Opener::new(keys.pictures, 6).down(&mut &wire[..]).is_err(), "another stream");
        let twice = [wire.clone(), wire].concat();
        let mut opener = Opener::new(keys.pictures, 5);
        let mut reading = &twice[..];
        assert!(opener.down(&mut reading).is_ok());
        assert!(opener.down(&mut reading).is_err(), "replayed");
    }
}
