//! Keeping what you type private and making sure only computers you paired can type for you.
//!
//! Each computer has a long-lived X25519 key. A session mixes both computers' long-lived keys
//! with fresh ones, so each session has its own keys. The first time two computers meet, both
//! show a six-digit code; the person checks they match. The controlling computer commits to its
//! fresh key before seeing the other's, so someone in the middle can't search for keys that
//! make the codes agree. Traffic is sealed with ChaCha20-Poly1305, numbered so nothing can be
//! replayed.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::wire::{Id, Key, sealed_header};

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    // Without the system's randomness there's no safe key to make; this never fails in practice.
    getrandom::fill(&mut bytes).expect("the system has no source of randomness");
    bytes
}

pub fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u32).to_le_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// The id other computers know this one by: derived from its key, so it can't be claimed by
/// another computer without that key.
pub fn id_of(key: &Key) -> Id {
    let digest = hash(&[b"roam id", key]);
    digest[..8].try_into().unwrap_or_default()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut out = [0; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// This computer's long-lived key.
#[derive(Clone)]
pub struct Identity {
    secret: StaticSecret,
    pub key: Key,
    pub id: Id,
}

impl Identity {
    pub fn from_secret(secret: [u8; 32]) -> Self {
        let secret = StaticSecret::from(secret);
        let key = PublicKey::from(&secret).to_bytes();
        Self { secret, key, id: id_of(&key) }
    }

    pub fn generate() -> Self {
        Self::from_secret(random())
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }
}

/// A fresh key for one handshake.
pub struct Ephemeral {
    secret: StaticSecret,
    pub key: Key,
}

impl Ephemeral {
    pub fn generate() -> Self {
        let secret = StaticSecret::from(random::<32>());
        let key = PublicKey::from(&secret).to_bytes();
        Self { secret, key }
    }

    pub fn commitment(&self) -> [u8; 32] {
        commit(&self.key)
    }
}

pub fn commit(key: &Key) -> [u8; 32] {
    hash(&[b"roam commit", key])
}

/// Both directions' keys and the code to compare, from one side's view.
pub struct Agreed {
    pub send: Cipher,
    pub receive: Cipher,
    /// Keys for big clipboards, which go over TCP (see `bulk`), one each way.
    pub bulk_send: [u8; 32],
    pub bulk_receive: [u8; 32],
    /// Six digits, the same on both computers unless someone is in the middle.
    pub code: u32,
}

/// What both sides know after the handshake. `controller` is the side that sent the offer.
pub struct Handshake<'a> {
    pub identity: &'a Identity,
    pub ephemeral: &'a Ephemeral,
    pub peer_key: Key,
    pub peer_ephemeral: Key,
    pub session: u32,
    pub controller: bool,
}

impl Handshake<'_> {
    pub fn agree(&self) -> Option<Agreed> {
        let peer_static = PublicKey::from(self.peer_key);
        let peer_ephemeral = PublicKey::from(self.peer_ephemeral);
        let both_static = self.identity.secret.diffie_hellman(&peer_static);
        let both_ephemeral = self.ephemeral.secret.diffie_hellman(&peer_ephemeral);
        if !both_static.was_contributory() || !both_ephemeral.was_contributory() {
            return None;
        }
        let (controller_key, other_key) = order(self.controller, &self.identity.key, &self.peer_key);
        let (controller_eph, other_eph) = order(self.controller, &self.ephemeral.key, &self.peer_ephemeral);
        let session = self.session.to_le_bytes();
        let transcript = hash(&[b"roam 1", controller_key, other_key, controller_eph, other_eph, &session]);
        let material = hash(&[b"roam keys", both_static.as_bytes(), both_ephemeral.as_bytes(), &transcript]);
        let outward = hash(&[b"controller to other", &material]);
        let inward = hash(&[b"other to controller", &material]);
        let digits = hash(&[b"roam code", &transcript]);
        let code = u32::from_le_bytes([digits[0], digits[1], digits[2], digits[3]]) % 1_000_000;
        let bulk_out = hash(&[b"bulk controller to other", &material]);
        let bulk_in = hash(&[b"bulk other to controller", &material]);
        let (send, receive) = if self.controller { (outward, inward) } else { (inward, outward) };
        let (bulk_send, bulk_receive) = if self.controller { (bulk_out, bulk_in) } else { (bulk_in, bulk_out) };
        Some(Agreed {
            send: Cipher::new(send, self.session),
            receive: Cipher::new(receive, self.session),
            bulk_send,
            bulk_receive,
            code,
        })
    }
}

fn order<'k>(first: bool, mine: &'k Key, theirs: &'k Key) -> (&'k Key, &'k Key) {
    if first { (mine, theirs) } else { (theirs, mine) }
}

pub fn format_code(code: u32) -> String {
    format!("{:03} {:03}", code / 1000, code % 1000)
}

/// One direction of a session.
pub struct Cipher {
    aead: ChaCha20Poly1305,
    session: u32,
}

impl Cipher {
    fn new(key: [u8; 32], session: u32) -> Self {
        Self { aead: ChaCha20Poly1305::new(&key.into()), session }
    }

    fn nonce(seq: u64) -> Nonce {
        let mut nonce = [0; 12];
        nonce[4..].copy_from_slice(&seq.to_le_bytes());
        nonce.into()
    }

    pub fn seal(&self, seq: u64, body: &[u8]) -> Vec<u8> {
        let header = sealed_header(self.session, seq);
        let sealed = self.aead.encrypt(&Self::nonce(seq), Payload { msg: body, aad: &header });
        sealed.unwrap_or_default()
    }

    pub fn open(&self, seq: u64, sealed: &[u8]) -> Option<Vec<u8>> {
        let header = sealed_header(self.session, seq);
        self.aead.decrypt(&Self::nonce(seq), Payload { msg: sealed, aad: &header }).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(session: u32) -> (Agreed, Agreed) {
        let controller = Identity::generate();
        let other = Identity::generate();
        let controller_eph = Ephemeral::generate();
        let other_eph = Ephemeral::generate();
        let a = Handshake {
            identity: &controller,
            ephemeral: &controller_eph,
            peer_key: other.key,
            peer_ephemeral: other_eph.key,
            session,
            controller: true,
        };
        let b = Handshake {
            identity: &other,
            ephemeral: &other_eph,
            peer_key: controller.key,
            peer_ephemeral: controller_eph.key,
            session,
            controller: false,
        };
        (a.agree().unwrap(), b.agree().unwrap())
    }

    #[test]
    fn both_sides_agree_and_can_talk() {
        let (a, b) = pair(5);
        assert_eq!(a.code, b.code);
        let sealed = a.send.seal(1, b"hello");
        assert_eq!(b.receive.open(1, &sealed).as_deref(), Some(&b"hello"[..]));
        let back = b.send.seal(1, b"hi");
        assert_eq!(a.receive.open(1, &back).as_deref(), Some(&b"hi"[..]));
        // Each direction has its own key.
        assert!(a.receive.open(1, &sealed).is_none());
    }

    #[test]
    fn tampering_and_the_wrong_number_fail() {
        let (a, b) = pair(5);
        let mut sealed = a.send.seal(7, b"type a password");
        assert!(b.receive.open(8, &sealed).is_none());
        sealed[0] ^= 1;
        assert!(b.receive.open(7, &sealed).is_none());
    }

    #[test]
    fn a_man_in_the_middle_gets_a_different_code() {
        let controller = Identity::generate();
        let other = Identity::generate();
        let attacker = Identity::generate();
        let (ce, oe, ae) = (Ephemeral::generate(), Ephemeral::generate(), Ephemeral::generate());
        // The controller talks to the attacker, believing it's the other computer.
        let seen_by_controller = Handshake {
            identity: &controller,
            ephemeral: &ce,
            peer_key: attacker.key,
            peer_ephemeral: ae.key,
            session: 1,
            controller: true,
        };
        let seen_by_other = Handshake {
            identity: &other,
            ephemeral: &oe,
            peer_key: attacker.key,
            peer_ephemeral: ae.key,
            session: 1,
            controller: false,
        };
        assert_ne!(seen_by_controller.agree().unwrap().code, seen_by_other.agree().unwrap().code);
    }

    #[test]
    fn identities_and_hex_round_trip() {
        let identity = Identity::generate();
        let again = Identity::from_secret(identity.secret_bytes());
        assert_eq!(again.key, identity.key);
        assert_eq!(again.id, id_of(&identity.key));
        assert_eq!(unhex::<32>(&hex(&identity.key)), Some(identity.key));
        assert_eq!(unhex::<2>("zz00"), None);
        assert_eq!(format_code(42_007), "042 007");
    }
}
