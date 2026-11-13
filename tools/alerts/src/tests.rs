use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::{Event, Node, Request, Start, Values};
use notchbase_sdk::{Driver, Runtime, Waker};

use super::*;
use crate::alert::{Alert, Stay};

const WAIT: Duration = Duration::from_secs(5);
const KNOCK: Duration = Duration::from_millis(200);
const PROBE: &str = "GET /ready HTTP/1.1\r\n\r\n";

fn values(enabled: bool, port: u16, token: &str) -> Values {
    let mut values = Values::new();
    values.insert(config::ENABLED.into(), enabled.into());
    values.insert(config::PORT.into(), port.into());
    values.insert(config::TOKEN.into(), token.into());
    values
}

fn started(enabled: bool, port: u16) -> Runtime<Alerts> {
    let mut runtime = Runtime::new(Waker::noop());
    runtime.start(Start { settings: values(enabled, port, ""), ..Start::default() });
    runtime
}

fn quiet() -> Runtime<Alerts> {
    started(false, config::DEFAULT_PORT)
}

fn alert(title: &str) -> Alert {
    let stay = Stay::For(Duration::from_millis(4000));
    Alert { title: title.into(), message: None, icon: Icon::Bell, tone: Tone::Blue, stay, shake: true }
}

fn show(runtime: &mut Runtime<Alerts>, alert: Alert) -> Vec<Request> {
    runtime.send(Msg::Webhook(Action::Show(alert))).requests
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

fn eventually(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn settle(runtime: &mut Runtime<Alerts>) -> Vec<Request> {
    let mut requests = Vec::new();
    let arrived = eventually(|| {
        requests.extend(runtime.handle(Vec::new()).requests);
        !requests.is_empty()
    });
    assert!(arrived, "gave up waiting for a request");
    requests
}

fn answer(port: u16, raw: &str) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), KNOCK).ok()?;
    stream.set_read_timeout(Some(WAIT)).ok()?;
    stream.write_all(raw.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn call(port: u16, raw: &str) -> String {
    answer(port, raw).unwrap_or_else(|| panic!("nothing answered on port {port}"))
}

fn listening(port: u16) {
    assert!(eventually(|| answer(port, PROBE).is_some()), "port {port} never opened");
}

fn closed(port: u16) -> bool {
    eventually(|| TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok())
}

#[test]
fn manifest_has_quiet_settings() {
    let manifest = Alerts::manifest();
    assert_eq!((manifest.id.as_str(), manifest.page.as_deref()), (ID, None));
    let keys: Vec<_> = manifest.settings.iter().map(|setting| (setting.key.as_str(), setting.visible)).collect();
    assert_eq!(keys, [(config::ENABLED, true), (config::PORT, true), (config::TOKEN, false)]);
    assert_eq!(manifest.defaults()[config::PORT], 47832.0);
    let views = quiet().views();
    assert!(views.tile.is_none() && views.page.is_none() && views.live.is_none());
}

#[test]
fn alert_becomes_a_banner() {
    let mut runtime = quiet();
    let requests = show(&mut runtime, Alert { message: Some("All green".into()), ..alert("Deploy") });
    let [Request::Notice { notice }] = &requests[..] else { panic!("{requests:?}") };
    assert!(notice.banner && notice.shake && notice.pop && notice.below.is_some());
    assert_eq!(notice.duration_ms, 4000);
    assert!(runtime.handle(Vec::new()).wants.every_ms.is_empty());
}

#[test]
fn sticky_alert_waits_for_dismiss() {
    let mut runtime = quiet();
    let requests = show(&mut runtime, Alert { stay: Stay::UntilClicked, shake: false, ..alert("Server down") });
    let [Request::Notice { notice }] = &requests[..] else { panic!("{requests:?}") };
    assert!(notice.sticky() && notice.pop && !notice.shake);
    assert_eq!(runtime.send(Msg::Webhook(Action::Dismiss)).requests, vec![Request::DismissNotice]);
}

#[test]
fn real_request_becomes_a_notice() {
    let port = free_port();
    let mut runtime = started(true, port);
    listening(port);
    let body =
        r#"{"title":"Deploy finished","message":"All green","icon":"checkmark","tone":"green","duration_ms":6000}"#;
    let response = call(
        port,
        &format!(
            "POST /alert HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n") && response.ends_with("{\"ok\":true}"), "{response}");
    let requests = settle(&mut runtime);
    let [Request::Notice { notice }] = &requests[..] else { panic!("{requests:?}") };
    assert_eq!((notice.duration_ms, notice.shake, notice.banner), (6000, true, true));
    assert!(matches!(&notice.trailing, Node::Text(title) if title.text == "Deploy finished"));
    assert!(call(port, "DELETE /alert HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 200"));
    assert_eq!(settle(&mut runtime), vec![Request::DismissNotice]);
}

#[test]
fn port_change_moves_listener() {
    let first = free_port();
    let second = std::iter::repeat_with(free_port).find(|port| *port != first).unwrap();
    let mut runtime = started(true, first);
    listening(first);
    runtime.handle(vec![Event::Settings { values: values(true, second, "") }]);
    listening(second);
    assert!(call(second, "GET /alert?title=Moved HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 200"));
    assert!(closed(first));
    assert!(matches!(&settle(&mut runtime)[..], [Request::Notice { .. }]));
}

#[test]
fn new_token_restarts_listener() {
    let port = free_port();
    let mut runtime = started(true, port);
    listening(port);
    assert!(call(port, "GET /alert?title=Open HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 200"));
    runtime.handle(vec![Event::Settings { values: values(true, port, "s3cret") }]);
    let refused = |response: Option<String>| response.is_some_and(|response| response.starts_with("HTTP/1.1 401"));
    assert!(eventually(|| refused(answer(port, "GET /alert?title=Nope HTTP/1.1\r\n\r\n"))));
    let authorized = "GET /alert?title=Yes HTTP/1.1\r\nAuthorization: Bearer s3cret\r\n\r\n";
    assert!(call(port, authorized).starts_with("HTTP/1.1 200"));
}

#[test]
fn turning_off_stops_listening() {
    let port = free_port();
    let mut runtime = started(true, port);
    listening(port);
    let generation = runtime.tool().unwrap().generation;
    runtime.handle(vec![Event::Settings { values: values(true, port, "") }]);
    assert_eq!(runtime.tool().unwrap().generation, generation, "unchanged settings keep the server");
    runtime.handle(vec![Event::Settings { values: values(false, port, "") }]);
    assert!(runtime.tool().unwrap().server.is_none());
    assert!(closed(port));
}

#[test]
fn drop_stops_the_server() {
    let port = free_port();
    let runtime = started(true, port);
    listening(port);
    drop(runtime);
    assert!(closed(port));
}

#[test]
fn taken_port_is_reported_once() {
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let mut runtime = started(true, port);
    let requests = settle(&mut runtime);
    assert_eq!(requests, vec![Request::Notify { text: format!("Alerts couldn't listen on port {port}") }]);
    assert!(runtime.tool().unwrap().server.is_none());
    assert!(runtime.send(Msg::Unreachable { generation: 0, port }).requests.is_empty());
}
