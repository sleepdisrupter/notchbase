//! An extended display's pictures over UDP, the way game streaming sends them.
//!
//! Over TCP, one packet lost on Wi-Fi holds up every picture behind it until it's resent, a
//! freeze of 200 ms or more. Here each picture is cut into packets with spare repair packets
//! (Reed-Solomon, a fifth extra) so a few lost ones are rebuilt on arrival with no wait. A
//! picture that still can't be rebuilt is skipped, and the lending computer asks for a key
//! picture to start clean from, instead of the stream stopping.
//!
//! Every packet is sealed with the stream's own key and a number that never repeats.
//!
//! A packet: `[MAGIC][count u64][sealed]`, and inside the seal: the picture's number (u32), its
//! length (u32), whether it's a key picture (u8), which block of it this is and how many there
//! are (u8, u8), the block's data and repair shard counts (u8, u8), this shard's place in the
//! block (u8), then the shard.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use reed_solomon_erasure::galois_8::ReedSolomon;

use super::Picture;
use crate::crypto::hash;

const MAGIC: u8 = 0xD5;
/// The most picture bytes in one packet: with the headers and seal it stays under 1,280 bytes,
/// which every network carries without splitting.
pub const SHARD: usize = 1_180;
/// The most data shards in a block: a block is rebuilt on its own, and the repair work grows
/// with the block, so a big key picture is many small blocks rather than one huge one.
const BLOCK_SHARDS: usize = 32;
const BLOCK_BYTES: usize = SHARD * BLOCK_SHARDS;
/// The most blocks a picture has; a bigger picture goes by the TCP stream instead.
const MOST_BLOCKS: usize = 255;
pub const MOST_BYTES: usize = BLOCK_BYTES * MOST_BLOCKS;
const HEADER: usize = 4 + 4 + 1 + 1 + 1 + 1 + 1 + 1;
/// A picture still missing pieces this long after a newer one is whole is given up on.
const LATE: Duration = Duration::from_millis(40);
/// And any picture still missing pieces after this long.
const STALE: Duration = Duration::from_millis(250);
/// The most pictures waiting to be put together, so junk can't fill the memory.
const MOST_WAITING: usize = 64;

/// The key for the packets, apart from the stream's TCP key so no nonce is used twice.
pub fn key(pictures: [u8; 32]) -> [u8; 32] {
    hash(&[b"roam display pictures over udp", &pictures])
}

fn nonce(stream: u32, count: u64) -> Nonce {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&stream.to_le_bytes());
    nonce[4..].copy_from_slice(&count.to_le_bytes());
    nonce.into()
}

/// The repair shards a block of `data` shards gets: a fifth, at least one, and for a key picture
/// two fifths. A lost key picture costs another one, bigger than any other, into a network that
/// is already losing packets.
fn repairs(data: usize, key: bool) -> usize {
    if key { (data * 2).div_ceil(5).max(1) } else { data.div_ceil(5).max(1) }
}

/// A picture's packets go out this many at a time, a moment apart: a key picture sent all at
/// once can overflow the Wi-Fi's queue and lose its own packets.
const BURST: usize = 48;
const BURST_GAP: Duration = Duration::from_micros(500);

/// Reed-Solomon codecs by shape, which are costly to make.
#[derive(Default)]
struct Codecs(HashMap<(usize, usize), ReedSolomon>);

impl Codecs {
    fn get(&mut self, data: usize, parity: usize) -> Option<&ReedSolomon> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.0.entry((data, parity)) {
            slot.insert(ReedSolomon::new(data, parity).ok()?);
        }
        self.0.get(&(data, parity))
    }
}

/// Cuts pictures into sealed packets and sends them.
pub struct Sender {
    socket: UdpSocket,
    to: SocketAddr,
    aead: ChaCha20Poly1305,
    stream: u32,
    count: u64,
    picture: u32,
    codecs: Codecs,
}

impl Sender {
    pub fn new(socket: UdpSocket, to: SocketAddr, key: [u8; 32], stream: u32) -> Self {
        Self {
            socket,
            to,
            aead: ChaCha20Poly1305::new(&key.into()),
            stream,
            count: 0,
            picture: 0,
            codecs: Codecs::default(),
        }
    }

    /// The sealed packets for `picture`, numbered as the next one.
    pub fn packets(&mut self, picture: &Picture) -> Vec<Vec<u8>> {
        let number = self.picture;
        self.picture = self.picture.wrapping_add(1);
        let data = &picture.data;
        let blocks = data.len().div_ceil(BLOCK_BYTES).max(1);
        let mut by_block: Vec<Vec<Vec<u8>>> = Vec::with_capacity(blocks);
        for block in 0..blocks {
            let mut packets = Vec::new();
            let bytes = &data[(block * BLOCK_BYTES).min(data.len())..((block + 1) * BLOCK_BYTES).min(data.len())];
            let shards = bytes.len().div_ceil(SHARD).max(1);
            let width = bytes.len().div_ceil(shards).max(1);
            let parity = repairs(shards, picture.key);
            let mut all: Vec<Vec<u8>> = (0..shards + parity)
                .map(|i| {
                    let mut shard = vec![0u8; width];
                    if i < shards {
                        let from = (i * width).min(bytes.len());
                        let to = ((i + 1) * width).min(bytes.len());
                        shard[..to - from].copy_from_slice(&bytes[from..to]);
                    }
                    shard
                })
                .collect();
            if let Some(codec) = self.codecs.get(shards, parity) {
                let _ = codec.encode(&mut all);
            }
            for (index, shard) in all.iter().enumerate() {
                let mut plain = Vec::with_capacity(HEADER + shard.len());
                plain.extend_from_slice(&number.to_le_bytes());
                plain.extend_from_slice(&(data.len() as u32).to_le_bytes());
                plain.push(u8::from(picture.key));
                plain.push(block as u8);
                plain.push(blocks as u8);
                plain.push(shards as u8);
                plain.push(parity as u8);
                plain.push(index as u8);
                plain.extend_from_slice(shard);
                packets.push(self.seal(&plain));
            }
            by_block.push(packets);
        }
        // The blocks' packets take turns: Wi-Fi loses packets in runs, and a run spread over
        // several blocks leaves each one few enough losses to rebuild.
        let mut packets = Vec::with_capacity(by_block.iter().map(Vec::len).sum());
        let most = by_block.iter().map(Vec::len).max().unwrap_or(0);
        for turn in 0..most {
            for block in &mut by_block {
                if let Some(packet) = block.get_mut(turn) {
                    packets.push(std::mem::take(packet));
                }
            }
        }
        packets
    }

    fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let count = self.count;
        self.count += 1;
        let sealed =
            self.aead.encrypt(&nonce(self.stream, count), Payload { msg: plain, aad: &[MAGIC] }).unwrap_or_default();
        let mut packet = Vec::with_capacity(9 + sealed.len());
        packet.push(MAGIC);
        packet.extend_from_slice(&count.to_le_bytes());
        packet.extend_from_slice(&sealed);
        packet
    }

    /// Sends `picture`. A full send buffer drops the rest of it: the network is behind, and
    /// the lending computer asks for a fresh start.
    pub fn send(&mut self, picture: &Picture) -> io::Result<()> {
        for (index, packet) in self.packets(picture).iter().enumerate() {
            if index > 0 && index % BURST == 0 {
                std::thread::sleep(BURST_GAP);
            }
            match self.socket.send_to(packet, self.to) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }
}

/// One block of a picture on its way.
struct Block {
    shards: Vec<Option<Vec<u8>>>,
    data: usize,
    have: usize,
    done: Option<Vec<u8>>,
}

/// A picture on its way.
struct Coming {
    length: usize,
    key: bool,
    blocks: Vec<Option<Block>>,
    whole: Option<Instant>,
    first: Instant,
}

impl Coming {
    fn whole(&self) -> bool {
        self.whole.is_some()
    }
}

/// What came of the packets so far.
#[derive(Debug, PartialEq)]
pub enum Arrival {
    Picture(Picture),
    /// A picture couldn't be put together: what follows needs a key picture to start from.
    Lost,
}

/// Puts the pictures back together, in order.
pub struct Receiver {
    aead: ChaCha20Poly1305,
    stream: u32,
    coming: BTreeMap<u32, Coming>,
    /// The number of the next picture to show, once the first has come.
    next: Option<u32>,
    /// After a loss, pictures wait for a key picture.
    need_key: bool,
    codecs: Codecs,
    /// Pictures rebuilt from repair packets, and given up on, since the last look.
    pub repaired: u32,
    pub lost: u32,
}

fn after(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

impl Receiver {
    pub fn new(key: [u8; 32], stream: u32) -> Self {
        Self {
            aead: ChaCha20Poly1305::new(&key.into()),
            stream,
            coming: BTreeMap::new(),
            next: None,
            need_key: false,
            codecs: Codecs::default(),
            repaired: 0,
            lost: 0,
        }
    }

    /// Takes one packet (ignored unless genuine), then answers what can be shown now.
    pub fn take(&mut self, packet: &[u8], now: Instant) -> Vec<Arrival> {
        self.add(packet, now);
        self.ready(now)
    }

    fn add(&mut self, packet: &[u8], now: Instant) {
        if packet.len() < 9 || packet[0] != MAGIC {
            return;
        }
        let count = u64::from_le_bytes(packet[1..9].try_into().unwrap_or_default());
        let Ok(plain) = self.aead.decrypt(&nonce(self.stream, count), Payload { msg: &packet[9..], aad: &[MAGIC] })
        else {
            return;
        };
        if plain.len() <= HEADER {
            return;
        }
        let word = |at: usize| u32::from_le_bytes([plain[at], plain[at + 1], plain[at + 2], plain[at + 3]]);
        let (number, length, key) = (word(0), word(4) as usize, plain[8] != 0);
        let (block, blocks, data, parity, index) =
            (plain[9] as usize, plain[10] as usize, plain[11] as usize, plain[12] as usize, plain[13] as usize);
        let shard = &plain[HEADER..];
        if blocks == 0 || block >= blocks || data == 0 || index >= data + parity || length > MOST_BYTES {
            return;
        }
        if self.next.is_some_and(|next| after(next, number)) {
            return; // shown or given up on already
        }
        if !self.coming.contains_key(&number) && self.coming.len() >= MOST_WAITING {
            return;
        }
        let coming = self.coming.entry(number).or_insert_with(|| Coming {
            length,
            key,
            blocks: (0..blocks).map(|_| None).collect(),
            whole: None,
            first: now,
        });
        if coming.whole() || coming.blocks.len() != blocks || coming.length != length {
            return;
        }
        let slot = coming.blocks[block].get_or_insert_with(|| Block {
            shards: vec![None; data + parity],
            data,
            have: 0,
            done: None,
        });
        if slot.done.is_some() || slot.shards.len() != data + parity || slot.data != data {
            return;
        }
        if slot.shards[index].is_none() {
            slot.shards[index] = Some(shard.to_vec());
            slot.have += 1;
        }
        if slot.have >= data {
            let rebuilt = slot.shards[..data].iter().any(Option::is_none);
            let mut shards = std::mem::take(&mut slot.shards);
            let ok = !rebuilt
                || self.codecs.get(data, parity).is_some_and(|codec| codec.reconstruct_data(&mut shards).is_ok());
            if ok {
                if rebuilt {
                    self.repaired += 1;
                }
                let bytes: Vec<u8> = shards[..data].iter().flatten().flatten().copied().collect();
                // The block's own length: the last one is what's left of the picture.
                let own = length.saturating_sub(block * BLOCK_BYTES).min(BLOCK_BYTES);
                let mut bytes = bytes;
                bytes.truncate(own);
                // Back through the map: the codec borrow above ended.
                if let Some(coming) = self.coming.get_mut(&number)
                    && let Some(Some(slot)) = coming.blocks.get_mut(block)
                {
                    slot.done = Some(bytes);
                    if coming.blocks.iter().all(|b| b.as_ref().is_some_and(|b| b.done.is_some())) {
                        coming.whole = Some(now);
                    }
                }
            } else if let Some(coming) = self.coming.get_mut(&number)
                && let Some(Some(slot)) = coming.blocks.get_mut(block)
            {
                slot.shards = shards;
            }
        }
        if self.next.is_none() {
            self.next = Some(number);
        }
    }

    /// The pictures that can be shown now, in order, and any losses.
    pub fn ready(&mut self, now: Instant) -> Vec<Arrival> {
        let mut out = Vec::new();
        while let Some(next) = self.next {
            if self.need_key {
                // Start again from the first whole key picture there is.
                let key = self.coming.iter().find(|(_, c)| c.whole() && c.key).map(|(n, _)| *n);
                match key {
                    Some(key) => {
                        self.coming.retain(|n, _| !after(key, *n));
                        self.next = Some(key);
                        self.need_key = false;
                        continue;
                    }
                    None => {
                        // Nothing to show until it comes; keep only what may still be the key.
                        let old: Vec<u32> = self
                            .coming
                            .iter()
                            .filter(|(_, c)| c.whole() && !c.key || now.duration_since(c.first) > STALE)
                            .map(|(n, _)| *n)
                            .collect();
                        for n in old {
                            self.coming.remove(&n);
                        }
                        break;
                    }
                }
            }
            match self.coming.get(&next) {
                Some(coming) if coming.whole() => {
                    let coming = self.coming.remove(&next).unwrap_or_else(|| unreachable!());
                    let data =
                        coming.blocks.into_iter().flatten().filter_map(|b| b.done).flatten().collect::<Vec<u8>>();
                    out.push(Arrival::Picture(Picture { key: coming.key, data }));
                    self.next = Some(next.wrapping_add(1));
                }
                waiting => {
                    // Given up on when a newer picture has been whole a while, or it's old.
                    let newer_whole = self
                        .coming
                        .iter()
                        .filter(|(n, _)| after(**n, next))
                        .filter_map(|(_, c)| c.whole)
                        .min()
                        .is_some_and(|whole| now.duration_since(whole) >= LATE);
                    let stale = waiting.is_some_and(|c| now.duration_since(c.first) >= STALE);
                    if !(newer_whole || stale) {
                        break;
                    }
                    self.coming.remove(&next);
                    self.next = Some(next.wrapping_add(1));
                    self.lost += 1;
                    self.need_key = true;
                    out.push(Arrival::Lost);
                }
            }
        }
        out
    }

    /// When `ready` should be asked again though no packet came, if pictures are waiting.
    pub fn check_in(&self) -> Option<Duration> {
        (!self.coming.is_empty()).then_some(LATE / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Sender, Receiver) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let to = socket.local_addr().unwrap();
        let key = key([7; 32]);
        (Sender::new(socket, to, key, 5), Receiver::new(key, 5))
    }

    fn picture(key: bool, size: usize, seed: u8) -> Picture {
        Picture { key, data: (0..size).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect() }
    }

    #[test]
    fn pictures_come_back_whole_and_in_order() {
        let (mut sender, mut receiver) = pair();
        let now = Instant::now();
        let pictures = [picture(true, 300_000, 1), picture(false, 20, 2), picture(false, 5_000, 3)];
        let mut out = Vec::new();
        for p in &pictures {
            for packet in sender.packets(p) {
                out.extend(receiver.take(&packet, now));
            }
        }
        assert_eq!(out, pictures.iter().cloned().map(Arrival::Picture).collect::<Vec<_>>());
    }

    #[test]
    fn a_few_lost_packets_are_rebuilt() {
        let (mut sender, mut receiver) = pair();
        let now = Instant::now();
        let p = picture(true, 100_000, 9);
        let packets = sender.packets(&p);
        let mut out = Vec::new();
        // One packet in seven lost, spread over the blocks.
        for (i, packet) in packets.iter().enumerate() {
            if i % 7 != 3 {
                out.extend(receiver.take(packet, now));
            }
        }
        assert_eq!(out, vec![Arrival::Picture(p)]);
        assert!(receiver.repaired > 0);
    }

    #[test]
    fn a_picture_that_cant_be_rebuilt_is_skipped_until_a_key_picture() {
        let (mut sender, mut receiver) = pair();
        let start = Instant::now();
        let first = picture(true, 2_000, 1);
        let broken = picture(false, 30_000, 2);
        let after_it = picture(false, 2_000, 3);
        let key = picture(true, 4_000, 4);
        let mut out = Vec::new();
        for packet in sender.packets(&first) {
            out.extend(receiver.take(&packet, start));
        }
        // Half of this one never comes.
        for (i, packet) in sender.packets(&broken).iter().enumerate() {
            if i % 2 == 0 {
                out.extend(receiver.take(packet, start));
            }
        }
        for packet in sender.packets(&after_it) {
            out.extend(receiver.take(&packet, start));
        }
        assert_eq!(out, vec![Arrival::Picture(first.clone())], "the next waits a moment for the missing one");
        let later = start + LATE * 2;
        out.extend(receiver.ready(later));
        assert_eq!(out.last(), Some(&Arrival::Lost));
        for packet in sender.packets(&key) {
            out.extend(receiver.take(&packet, later));
        }
        assert_eq!(out, vec![Arrival::Picture(first), Arrival::Lost, Arrival::Picture(key)]);
    }

    #[test]
    fn forged_and_replayed_packets_do_nothing() {
        let (mut sender, mut receiver) = pair();
        let now = Instant::now();
        let p = picture(false, 10, 1);
        let mut packet = sender.packets(&p).remove(0);
        assert_eq!(receiver.take(&packet, now), vec![Arrival::Picture(p)]);
        assert_eq!(receiver.take(&packet, now), vec![], "a replay is an old picture");
        let last = packet.len() - 1;
        packet[last] ^= 1;
        assert_eq!(receiver.take(&packet, now), vec![]);
    }

    #[test]
    fn sending_goes_over_the_network() {
        let (mut sender, mut receiver) = pair();
        let socket = sender.socket().try_clone().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let p = picture(true, 50_000, 3);
        sender.send(&p).unwrap();
        let mut buffer = [0u8; 2048];
        let mut out = Vec::new();
        while out.is_empty() {
            let (n, _) = socket.recv_from(&mut buffer).unwrap();
            out.extend(receiver.take(&buffer[..n], Instant::now()));
        }
        assert_eq!(out, vec![Arrival::Picture(p)]);
    }
}
