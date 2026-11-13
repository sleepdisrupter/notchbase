use std::io::{Cursor, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crate::assistant::call::{self, Inbound, Request};
use crate::assistant::wire::{self, MAX_LINE};
use crate::assistant::{endpoint, timing};

use super::{TempDir, eventually, running};

#[test]
fn show_call_wakes_the_running_app() {
    let app = running(timing::hold);
    assert_eq!(app.assistant.shows(), 0);
    assert!(call::send(app.dir.root(), Request::Show));
    eventually("the show", || (app.assistant.shows() == 1).then_some(()));
    assert!(app.wakes.load(Ordering::SeqCst) >= 1);
    assert!(app.assistant.prompts().is_empty(), "a show is not a prompt");
}

#[test]
fn show_with_wrong_token_is_ignored() {
    let app = running(timing::hold);
    let endpoint = endpoint::read(app.dir.root()).unwrap();
    let mut stream = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port))).unwrap();
    stream.write_all(b"{\"token\":\"nope\",\"call\":{\"type\":\"show\"}}\n").unwrap();
    drop(stream);
    assert!(call::send(app.dir.root(), Request::Show));
    eventually("the real show", || (app.assistant.shows() >= 1).then_some(()));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(app.assistant.shows(), 1);
}

#[test]
fn show_fails_with_nothing_running() {
    let dir = TempDir::new();
    assert!(!call::send(dir.root(), Request::Show));
}

#[test]
fn calls_and_events_share_line_format() {
    let line = b"{\"token\":\"t\",\"call\":{\"type\":\"show\"}}\n".to_vec();
    let Some(Inbound::Call(decoded)) = wire::decode(&mut Cursor::new(line), MAX_LINE) else {
        panic!("a call");
    };
    assert_eq!((decoded.token.as_str(), decoded.call), ("t", Request::Show));
    let hook = br#"{"token":"h","event":{"provider":"claude","session":"s","agent":null,"agent_type":null,"cwd":"/x","started_ms":1,"signal":{"type":"turn_started"}}}
"#;
    let Some(Inbound::Hook(envelope)) = wire::decode(&mut Cursor::new(hook.to_vec()), MAX_LINE) else {
        panic!("a hook event");
    };
    assert_eq!(envelope.token, "h");
    let unknown = b"{\"token\":\"t\",\"call\":{\"type\":\"explode\"}}\n".to_vec();
    assert!(wire::decode::<Inbound>(&mut Cursor::new(unknown), MAX_LINE).is_none());
}
