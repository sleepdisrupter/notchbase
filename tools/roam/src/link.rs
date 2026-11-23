//! One session's traffic, the way a multiplayer game sends it.
//!
//! - The cursor position goes in every packet and the newest wins: a lost packet costs nothing,
//!   because the next one, a few milliseconds later, says where the cursor is now.
//! - Key presses, clicks and scrolls must arrive, in order. Rather than waiting to notice a loss
//!   (TCP waits 200 ms or more), every packet repeats each small message the other side hasn't
//!   acknowledged yet. Packets go out every few milliseconds while you use the mouse, so a lost
//!   key press is usually repaired by the very next packet.
//! - Each packet carries the sender's clock and echoes the other's, which gives the round trip
//!   time without extra packets.

use std::collections::{BTreeMap, VecDeque};

use crate::crypto::{Agreed, Cipher};
use crate::wire::{Body, MAX_PACKET, Message, Motion, Packet};

/// Messages at most this big are repeated in every packet until acknowledged.
const SMALL: usize = 24;
/// How far ahead of what was delivered a message may be numbered and still be kept.
const AHEAD: u32 = 4096;
/// How many waiting messages a packet looks at.
const LOOK: usize = 96;

struct Waiting {
    seq: u32,
    message: Message,
    size: usize,
    sent: Option<u64>,
}

pub struct Incoming {
    pub motion: Option<Motion>,
    /// The sender's clock when it sent this, in microseconds (wrapping).
    pub time: u32,
    pub messages: Vec<Message>,
}

pub struct Link {
    send: Cipher,
    receive: Cipher,
    bulk: ([u8; 32], [u8; 32]),
    session: u32,
    seq: u64,
    highest: u64,
    seen: u64,
    numbered: u32,
    waiting: VecDeque<Waiting>,
    expect: u32,
    early: BTreeMap<u32, Message>,
    motion: u32,
    motion_in: Option<u32>,
    peer_time: Option<(u32, u64)>,
    rtt: Option<f64>,
    pub heard: u64,
    pub sent: u64,
    /// When the link started.
    pub born: u64,
    /// How long the other side had gone without hearing this one when it sent its latest
    /// packet; `None` until it has heard this one at all.
    pub their_wait: Option<u64>,
    /// Until when packets also take the detour (see the engine), and when one last did.
    pub detour_until: u64,
    pub detoured: u64,
}

fn newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

impl Link {
    pub fn new(session: u32, agreed: Agreed, now: u64) -> Self {
        Self {
            send: agreed.send,
            receive: agreed.receive,
            bulk: (agreed.bulk_send, agreed.bulk_receive),
            session,
            seq: 1,
            highest: 0,
            seen: 0,
            numbered: 1,
            waiting: VecDeque::new(),
            expect: 1,
            early: BTreeMap::new(),
            motion: 0,
            motion_in: None,
            peer_time: None,
            rtt: None,
            heard: now,
            sent: 0,
            born: now,
            their_wait: None,
            detour_until: 0,
            detoured: 0,
        }
    }

    pub fn session(&self) -> u32 {
        self.session
    }

    /// The key for big clipboards this side sends.
    pub fn bulk_send(&self) -> [u8; 32] {
        self.bulk.0
    }

    /// The key for big clipboards the other side sends.
    pub fn bulk_receive(&self) -> [u8; 32] {
        self.bulk.1
    }

    /// The smoothed round trip, in microseconds.
    pub fn rtt(&self) -> Option<f64> {
        self.rtt
    }

    fn resend_after(&self) -> u64 {
        let rtt = self.rtt.unwrap_or(40_000.0);
        (rtt * 1.5 + 4_000.0).clamp(8_000.0, 500_000.0) as u64
    }

    pub fn queue(&mut self, message: Message) {
        let size = message.size() + 4;
        self.waiting.push_back(Waiting { seq: self.numbered, message, size, sent: None });
        self.numbered = self.numbered.wrapping_add(1);
    }

    /// How many messages are still waiting to be acknowledged.
    pub fn backlog(&self) -> usize {
        self.waiting.len()
    }

    /// Whether a message is waiting to go out for the first time or again.
    pub fn due(&self, now: u64) -> bool {
        let after = self.resend_after();
        self.waiting.iter().take(LOOK).any(|w| w.sent.is_none_or(|sent| now.saturating_sub(sent) >= after))
    }

    /// The next packet, with the cursor at `at` if it moved.
    pub fn packet(&mut self, now: u64, at: Option<[f32; 2]>) -> Vec<u8> {
        let motion = at.map(|at| {
            self.motion = self.motion.wrapping_add(1);
            Motion { seq: self.motion, at }
        });
        let (echo, held) = match self.peer_time {
            Some((time, received)) => (time, now.saturating_sub(received).min(u64::from(u32::MAX)) as u32),
            None => (0, 0),
        };
        let mut body = Body { ack: self.expect, time: now as u32, echo, held, motion, messages: Vec::new() };
        let mut room = MAX_PACKET - 16 - 16 - 32;
        let after = self.resend_after();
        for waiting in self.waiting.iter_mut().take(LOOK) {
            if waiting.size > room {
                break;
            }
            let fresh = waiting.sent.is_none_or(|sent| now.saturating_sub(sent) >= after);
            if waiting.size <= SMALL || fresh {
                room -= waiting.size;
                waiting.sent = Some(now);
                body.messages.push((waiting.seq, waiting.message.clone()));
            }
        }
        let seq = self.seq;
        self.seq += 1;
        self.sent = now;
        let sealed = self.send.seal(seq, &body.encode());
        Packet::Sealed { session: self.session, seq, body: sealed }.encode()
    }

    /// Opens a sealed packet. `None` when it isn't genuine or was seen before.
    pub fn receive(&mut self, now: u64, seq: u64, sealed: &[u8]) -> Option<Incoming> {
        if !self.fresh(seq) {
            return None;
        }
        let body = Body::decode(&self.receive.open(seq, sealed)?)?;
        self.mark(seq);
        self.heard = now;
        self.peer_time = Some((body.time, now));
        self.their_wait = (body.echo != 0).then_some(u64::from(body.held));
        if body.echo != 0 {
            let rtt = f64::from((now as u32).wrapping_sub(body.echo).wrapping_sub(body.held));
            if rtt < 2_000_000.0 {
                self.rtt = Some(self.rtt.map_or(rtt, |smooth| smooth * 0.875 + rtt * 0.125));
            }
        }
        while self.waiting.front().is_some_and(|w| newer(body.ack, w.seq)) {
            self.waiting.pop_front();
        }
        for (number, message) in body.messages {
            if !newer(number.wrapping_add(1), self.expect) || newer(number, self.expect.wrapping_add(AHEAD)) {
                continue; // already delivered, or absurdly far ahead
            }
            self.early.insert(number, message);
        }
        let mut messages = Vec::new();
        while let Some(message) = self.early.remove(&self.expect) {
            messages.push(message);
            self.expect = self.expect.wrapping_add(1);
        }
        let motion = body.motion.filter(|motion| self.motion_in.is_none_or(|last| newer(motion.seq, last)));
        if let Some(motion) = motion {
            self.motion_in = Some(motion.seq);
        }
        Some(Incoming { motion, time: body.time, messages })
    }

    fn fresh(&self, seq: u64) -> bool {
        if seq > self.highest {
            return true;
        }
        let back = self.highest - seq;
        back < 64 && self.seen & (1 << back) == 0
    }

    fn mark(&mut self, seq: u64) {
        if seq > self.highest {
            let shift = seq - self.highest;
            self.seen = if shift >= 64 { 0 } else { self.seen << shift };
            self.highest = seq;
            self.seen |= 1;
        } else {
            self.seen |= 1 << (self.highest - seq);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{Ephemeral, Handshake, Identity};

    fn linked() -> (Link, Link) {
        let (a, b) = (Identity::generate(), Identity::generate());
        let (ae, be) = (Ephemeral::generate(), Ephemeral::generate());
        let ours = Handshake {
            identity: &a,
            ephemeral: &ae,
            peer_key: b.key,
            peer_ephemeral: be.key,
            session: 3,
            controller: true,
        };
        let theirs = Handshake {
            identity: &b,
            ephemeral: &be,
            peer_key: a.key,
            peer_ephemeral: ae.key,
            session: 3,
            controller: false,
        };
        (Link::new(3, ours.agree().unwrap(), 0), Link::new(3, theirs.agree().unwrap(), 0))
    }

    fn open(packet: Vec<u8>) -> (u64, Vec<u8>) {
        match Packet::decode(&packet) {
            Some(Packet::Sealed { seq, body, .. }) => (seq, body),
            other => panic!("{other:?}"),
        }
    }

    fn deliver(to: &mut Link, now: u64, packet: Vec<u8>) -> Option<Incoming> {
        let (seq, body) = open(packet);
        to.receive(now, seq, &body)
    }

    #[test]
    fn a_lost_key_press_comes_with_the_next_packet() {
        let (mut a, mut b) = linked();
        a.queue(Message::Key { hid: 4, down: true });
        let _lost = a.packet(1_000, Some([1.0, 1.0]));
        a.queue(Message::Key { hid: 4, down: false });
        let next = deliver(&mut b, 3_000, a.packet(2_000, Some([2.0, 1.0]))).unwrap();
        assert_eq!(next.messages, vec![Message::Key { hid: 4, down: true }, Message::Key { hid: 4, down: false }]);
        assert_eq!(next.motion.unwrap().at, [2.0, 1.0]);
        // Once acknowledged they stop being repeated.
        deliver(&mut a, 4_000, b.packet(3_500, None)).unwrap();
        assert_eq!(a.backlog(), 0);
        assert!(deliver(&mut b, 5_000, a.packet(4_500, None)).unwrap().messages.is_empty());
    }

    #[test]
    fn messages_are_delivered_once_and_in_order() {
        let (mut a, mut b) = linked();
        a.queue(Message::Button { button: 0, down: true });
        let first = a.packet(1_000, None);
        a.queue(Message::Button { button: 0, down: false });
        let second = a.packet(2_000, None);
        // The second overtakes the first; it carries both, so the first adds nothing.
        let got = deliver(&mut b, 2_500, second).unwrap();
        assert_eq!(got.messages.len(), 2);
        assert!(deliver(&mut b, 2_600, first).unwrap().messages.is_empty());
    }

    #[test]
    fn old_cursor_positions_never_move_it_back() {
        let (mut a, mut b) = linked();
        let old = a.packet(1_000, Some([1.0, 0.0]));
        let new = a.packet(2_000, Some([5.0, 0.0]));
        assert!(deliver(&mut b, 2_100, new).unwrap().motion.is_some());
        assert!(deliver(&mut b, 2_200, old).unwrap().motion.is_none());
    }

    #[test]
    fn replays_and_forgeries_are_dropped() {
        let (mut a, mut b) = linked();
        a.queue(Message::Key { hid: 5, down: true });
        let packet = a.packet(1_000, None);
        assert!(deliver(&mut b, 1_100, packet.clone()).is_some());
        assert!(deliver(&mut b, 1_200, packet.clone()).is_none(), "replayed");
        let (seq, mut body) = open(a.packet(2_000, None));
        body[3] ^= 0x40;
        assert!(b.receive(2_100, seq, &body).is_none(), "tampered");
    }

    #[test]
    fn round_trip_time_is_measured_from_the_echo() {
        let (mut a, mut b) = linked();
        deliver(&mut b, 10_000, a.packet(6_000, None)).unwrap();
        // b holds it 1 ms, then answers; a hears back 8 ms after sending.
        deliver(&mut a, 14_000, b.packet(11_000, None)).unwrap();
        assert_eq!(a.rtt(), Some(7_000.0));
    }

    #[test]
    fn big_messages_wait_for_a_resend_time() {
        let (mut a, mut b) = linked();
        let chunk = Message::Clip { id: 1, index: 0, total: 1, bytes: vec![7; 900] };
        a.queue(chunk.clone());
        let first = a.packet(1_000, None);
        assert!(!a.due(2_000));
        let again = deliver(&mut b, 3_000, a.packet(2_000, None)).unwrap();
        assert!(again.messages.is_empty(), "not repeated straight away");
        assert!(a.due(100_000));
        assert_eq!(deliver(&mut b, 3_100, first).unwrap().messages, vec![chunk]);
    }

    #[test]
    fn heavy_loss_still_gets_every_key_through_in_order() {
        let (mut a, mut b) = linked();
        let mut got = Vec::new();
        let mut now = 0;
        for i in 0..200u16 {
            a.queue(Message::Key { hid: i, down: true });
            now += 4_000;
            let packet = a.packet(now, Some([f32::from(i), 0.0]));
            // Lose two packets in three.
            if i % 3 == 0 {
                got.extend(deliver(&mut b, now + 1_000, packet).unwrap().messages);
            }
            if i % 5 == 0 {
                let back = b.packet(now + 1_500, None);
                deliver(&mut a, now + 2_000, back);
            }
        }
        while a.backlog() > 0 {
            now += 4_000;
            let packet = a.packet(now, None);
            got.extend(deliver(&mut b, now + 1_000, packet).unwrap().messages);
            deliver(&mut a, now + 2_000, b.packet(now + 1_500, None));
        }
        let expected: Vec<_> = (0..200u16).map(|i| Message::Key { hid: i, down: true }).collect();
        assert_eq!(got, expected);
    }
}
