//! The local server: a TCP listener on 127.0.0.1 with a published port and token. Each hook gets a thread, at most 64
//! at a time, and a sweep expires prompts and runs once a second.

use std::io::{BufReader, ErrorKind, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::board::{Board, Held};
use super::call::{Inbound, Request};
use super::endpoint::{self, Endpoint};
use super::prompt::Provider;
use super::token;
use super::wire::{self, Answer};
use crate::poll::{Wake, lock};

const MAX_CONNECTIONS: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(200);
const SWEEP_EVERY: Duration = Duration::from_secs(1);
const ALIVE_EVERY: Duration = Duration::from_secs(5);
const UNBLOCK_TIMEOUT: Duration = Duration::from_millis(300);

pub(super) type HoldFor = fn(Provider, bool) -> Duration;

pub(super) struct Server {
    shared: Arc<Shared>,
    dir: PathBuf,
    port: u16,
}

struct Shared {
    board: Mutex<Board>,
    wake: Wake,
    token: String,
    stop: AtomicBool,
    live: AtomicUsize,
    shows: AtomicU64,
    hold_for: HoldFor,
}

impl Server {
    pub(super) fn start(dir: &Path, wake: Wake, hold_for: HoldFor) -> Option<Self> {
        let token = token::fresh()?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        endpoint::publish(dir, &Endpoint { port, token: token.clone() }).ok()?;
        let shared = Arc::new(Shared {
            board: Mutex::default(),
            wake,
            token,
            stop: AtomicBool::new(false),
            live: AtomicUsize::new(0),
            shows: AtomicU64::new(0),
            hold_for,
        });
        let accepting = Arc::clone(&shared);
        let accepted =
            thread::Builder::new().name("assistant-accept".into()).spawn(move || accept(&listener, &accepting));
        if accepted.is_err() {
            endpoint::retract(dir, &shared.token);
            return None;
        }
        let sweeping = Arc::clone(&shared);
        let _ = thread::Builder::new().name("assistant-sweep".into()).spawn(move || sweep(&sweeping));
        Some(Self { shared, dir: dir.to_path_buf(), port })
    }

    pub(super) fn read<R>(&self, look: impl FnOnce(&Board) -> R) -> R {
        look(&lock(&self.shared.board))
    }

    pub(super) fn change<R>(&self, edit: impl FnOnce(&mut Board) -> R) -> R {
        self.shared.change(edit)
    }

    pub(super) fn shows(&self) -> u64 {
        self.shared.shows.load(Ordering::SeqCst)
    }

    pub(super) fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        lock(&self.shared.board).release_all();
        // Connect once to wake the accept loop, so it sees the stop.
        let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)), UNBLOCK_TIMEOUT);
        endpoint::retract(&self.dir, &self.shared.token);
    }
}

impl Shared {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn change<R>(&self, edit: impl FnOnce(&mut Board) -> R) -> R {
        let (result, changed) = {
            let mut board = lock(&self.board);
            let before = board.version();
            let result = edit(&mut board);
            let changed = board.version() != before;
            (result, changed)
        };
        if changed && !self.stopping() {
            (self.wake)();
        }
        result
    }

    fn call(&self, request: Request) {
        match request {
            Request::Show => self.shows.fetch_add(1, Ordering::SeqCst),
        };
        if !self.stopping() {
            (self.wake)();
        }
    }
}

/// A place among the open connections, given back when the hook's thread ends.
struct Seat(Arc<Shared>);

impl Seat {
    fn take(shared: &Arc<Shared>) -> Option<Self> {
        let taken = shared.live.fetch_add(1, Ordering::SeqCst);
        // Made before the check, so dropping it gives the count back either way.
        let seat = Self(Arc::clone(shared));
        if taken >= MAX_CONNECTIONS {
            return None;
        }
        Some(seat)
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept(listener: &TcpListener, shared: &Arc<Shared>) {
    for incoming in listener.incoming() {
        if shared.stopping() {
            return;
        }
        match incoming {
            Ok(stream) => admit(stream, shared),
            Err(_) => thread::sleep(POLL),
        }
    }
}

fn admit(stream: TcpStream, shared: &Arc<Shared>) {
    let Some(seat) = Seat::take(shared) else {
        return;
    };
    let _ = thread::Builder::new().name("assistant-hook".into()).spawn(move || {
        let Seat(shared) = &seat;
        serve(stream, shared);
    });
}

fn sweep(shared: &Shared) {
    let mut looked = Instant::now();
    while !shared.stopping() {
        thread::sleep(SWEEP_EVERY);
        let now = Instant::now();
        let look = due(&mut looked, now, ALIVE_EVERY);
        shared.change(|board| {
            let now_ms = notchbase_sdk::now_ms();
            board.sweep(now);
            board.expire_runs(now_ms);
            if look {
                board.expire_gone(now_ms, crate::platform::process_alive);
            }
        });
    }
}

fn due(last: &mut Instant, now: Instant, every: Duration) -> bool {
    let due = now.duration_since(*last) >= every;
    if due {
        *last = now;
    }
    due
}

fn serve(stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let event = match receive(&stream, &shared.token) {
        Some(Inbound::Hook(envelope)) => envelope.event,
        Some(Inbound::Call(call)) => return shared.call(call.call),
        None => return,
    };
    let hold = (shared.hold_for)(event.provider, event.agent.is_some());
    let now = Instant::now();
    if let Some(held) = shared.change(|board| board.apply(event, now)) {
        attend(&stream, shared, held, now + hold);
    }
}

fn receive(stream: &TcpStream, token: &str) -> Option<Inbound> {
    let inbound: Inbound = wire::decode(&mut BufReader::new(stream), wire::MAX_LINE)?;
    token::same(inbound.token(), token).then_some(inbound)
}

fn attend(mut stream: &TcpStream, shared: &Shared, held: Held, deadline: Instant) {
    let answer = loop {
        match held.answers.recv_timeout(POLL) {
            Ok(answer) => break Some(answer),
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) if shared.stopping() || Instant::now() >= deadline => {
                break Some(Answer::Release);
            }
            Err(RecvTimeoutError::Timeout) if hung_up(stream) => break None,
            Err(RecvTimeoutError::Timeout) => {}
        }
    };
    shared.change(|board| board.forget(held.id));
    if let Some(line) = answer.and_then(|answer| wire::encode(&answer)) {
        let _ = stream.write_all(&line).and_then(|()| stream.flush());
    }
}

/// Whether the hook went away while we held it, so its prompt can leave the board.
fn hung_up(stream: &TcpStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut probe = [0; 1];
    let peeked = stream.peek(&mut probe);
    if stream.set_nonblocking(false).is_err() {
        return true;
    }
    match peeked {
        Ok(read) => read == 0,
        Err(error) => !matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted | ErrorKind::TimedOut),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_check_due_once_per_interval() {
        let start = Instant::now();
        let mut last = start;
        assert!(!due(&mut last, start + Duration::from_secs(4), ALIVE_EVERY));
        assert!(due(&mut last, start + ALIVE_EVERY, ALIVE_EVERY));
        assert_eq!(last, start + ALIVE_EVERY);
        assert!(!due(&mut last, start + Duration::from_secs(9), ALIVE_EVERY));
        assert!(due(&mut last, start + Duration::from_secs(10), ALIVE_EVERY));
    }
}
