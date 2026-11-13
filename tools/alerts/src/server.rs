//! The listener thread. It only binds to localhost and handles one request at a time.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use notchbase_sdk::Command;

use crate::Msg;
use crate::http::{self, Reply};
use crate::webhook::{self, Action};

const POLL: Duration = Duration::from_millis(100);
const PATIENCE: Duration = Duration::from_secs(2);
const LINGER: Duration = Duration::from_millis(250);
const BIND_PATIENCE: Duration = Duration::from_millis(1500);
const BIND_RETRY: Duration = Duration::from_millis(50);
const SHORTEST_WAIT: Duration = Duration::from_millis(1);
const DRAIN_LIMIT: usize = 64 * 1024;

pub struct Server {
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub fn start(port: u16, token: Option<String>, generation: u64) -> (Server, Command<Msg>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let command = Command::spawn(move |emitter| match bind(port, &stopped) {
        Ok(listener) => serve(&listener, token.as_deref(), &stopped, |action| emitter.send(Msg::Webhook(action))),
        Err(_) => emitter.send(Msg::Unreachable { generation, port }),
    });
    (Server { stop }, command)
}

/// After a restart the old listener can hold the port for a moment, so this keeps trying briefly.
pub fn bind(port: u16, stop: &AtomicBool) -> io::Result<TcpListener> {
    let deadline = Instant::now() + BIND_PATIENCE;
    loop {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                return Ok(listener);
            }
            Err(err)
                if err.kind() == ErrorKind::AddrInUse && Instant::now() < deadline && !stop.load(Ordering::Acquire) =>
            {
                thread::sleep(BIND_RETRY);
            }
            Err(err) => return Err(err),
        }
    }
}

pub fn serve(listener: &TcpListener, token: Option<&str>, stop: &AtomicBool, deliver: impl Fn(Action)) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => converse(&stream, token, &deliver),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(_) => thread::sleep(POLL),
        }
    }
}

fn converse(stream: &TcpStream, token: Option<&str>, deliver: &impl Fn(Action)) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_write_timeout(Some(PATIENCE));
    let reader = Patient { stream, deadline: Instant::now() + PATIENCE };
    let mut writer = stream;
    let request = http::receive(reader, || {
        let _ = writer.write_all(http::CONTINUE);
    });
    let reply = match request.and_then(|request| webhook::answer(&request, token)) {
        Ok(action) => {
            deliver(action);
            Reply::ok()
        }
        Err(reply) => reply,
    };
    let _ = writer.write_all(&reply.bytes());
    linger(stream);
}

/// Reads whatever the client is still sending before the socket closes. Closing with unread data
/// makes the system send a reset, and the client would lose the reply.
fn linger(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(LINGER));
    let mut sink = [0; 4096];
    let mut drained = 0;
    let mut reader = stream;
    while drained < DRAIN_LIMIT {
        match reader.read(&mut sink) {
            Ok(0) | Err(_) => return,
            Ok(read) => drained += read,
        }
    }
}

/// One deadline for the whole request, so a slow client can't hold up the server.
struct Patient<'a> {
    stream: &'a TcpStream,
    deadline: Instant,
}

impl Read for Patient<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left < SHORTEST_WAIT {
            return Err(ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        let mut stream = self.stream;
        stream.read(buffer)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::thread::JoinHandle;

    use notchbase_sdk::prelude::Tone;

    use super::*;

    struct Running {
        port: u16,
        stop: Arc<AtomicBool>,
        heard: Receiver<Action>,
        thread: JoinHandle<()>,
    }

    fn running(token: Option<&'static str>) -> Running {
        let stop = Arc::new(AtomicBool::new(false));
        let listener = bind(0, &stop).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, heard) = mpsc::channel();
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            serve(&listener, token, &stopped, |action| {
                let _ = sender.send(action);
            });
        });
        Running { port, stop, heard, thread }
    }

    fn call(port: u16, raw: &[u8]) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(raw).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn post(body: &str) -> Vec<u8> {
        format!("POST /alert HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[test]
    fn posted_alert_is_delivered() {
        let server = running(None);
        let response = call(server.port, &post(r#"{"title":"Deploy","tone":"green"}"#));
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.ends_with("{\"ok\":true}"));
        let Ok(Action::Show(alert)) = server.heard.recv_timeout(Duration::from_secs(1)) else { panic!("no alert") };
        assert_eq!((alert.title.as_str(), alert.tone), ("Deploy", Tone::Green));
        let response = call(server.port, b"DELETE /alert HTTP/1.1\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200"));
        assert_eq!(server.heard.recv_timeout(Duration::from_secs(1)), Ok(Action::Dismiss));
    }

    #[test]
    fn refusals_deliver_nothing() {
        let server = running(Some("s3cret"));
        assert!(call(server.port, &post(r#"{"title":"T"}"#)).starts_with("HTTP/1.1 401"));
        assert!(call(server.port, b"GET /elsewhere HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404"));
        assert!(server.heard.try_recv().is_err());
    }

    #[test]
    fn oversized_body_gets_413() {
        let server = running(None);
        let response = call(server.port, &post(&format!(r#"{{"title":"{}"}}"#, "x".repeat(http::BODY_LIMIT))));
        assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    }

    #[test]
    fn curl_is_told_to_continue() {
        let server = running(None);
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, server.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(b"POST /alert HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 13\r\n\r\n").unwrap();
        let mut interim = [0; http::CONTINUE.len()];
        stream.read_exact(&mut interim).unwrap();
        assert_eq!(&interim, http::CONTINUE);
        stream.write_all(br#"{"title":"T"}"#).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }

    #[test]
    fn stalled_client_times_out() {
        let server = running(None);
        let mut stalled = TcpStream::connect((Ipv4Addr::LOCALHOST, server.port)).unwrap();
        stalled.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stalled.write_all(b"POST /alert HTTP/1.1\r\n").unwrap();
        thread::sleep(POLL * 2);
        let started = Instant::now();
        assert!(call(server.port, b"GET /alert?title=Next HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 200"));
        assert!(started.elapsed() < PATIENCE + Duration::from_secs(1), "{:?}", started.elapsed());
        let mut response = String::new();
        stalled.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    }

    #[test]
    fn stopping_frees_the_port() {
        let server = running(None);
        server.stop.store(true, Ordering::Release);
        server.thread.join().unwrap();
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, server.port)).is_ok());
    }

    #[test]
    fn taken_port_gives_up() {
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        let started = Instant::now();
        assert!(bind(port, &AtomicBool::new(false)).is_err());
        assert!(started.elapsed() >= BIND_PATIENCE);
        let started = Instant::now();
        assert!(bind(port, &AtomicBool::new(true)).is_err());
        assert!(started.elapsed() < BIND_PATIENCE);
    }
}
