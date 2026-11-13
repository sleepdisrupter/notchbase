use crate::Subscription;
use crate::protocol::{Download, DownloadChange, Event, Focus, Power, Request, Sensors};
use crate::settings::Settings;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Power(Power),
    Locked(bool),
    Sensors(Sensors),
    Download(DownloadChange),
    Focus(Focus),
}

fn listening() -> Subscription<Msg> {
    Subscription::batch([
        Subscription::power(Msg::Power),
        Subscription::screen_lock(Msg::Locked),
        Subscription::sensors(Msg::Sensors),
        Subscription::downloads(Msg::Download),
        Subscription::focus(Msg::Focus),
    ])
}

#[test]
fn system_subs_ask_for_feeds() {
    let wants = listening().wants();
    assert!(wants.power && wants.screen_lock && wants.sensors && wants.downloads && wants.focus);
    let quiet = Subscription::<Msg>::power(Msg::Power).wants();
    assert!(quiet.power && !quiet.screen_lock && !quiet.sensors && !quiet.downloads);
}

#[test]
fn system_events_reach_listener() {
    let settings = Settings::default();
    let power = Power { level: Some(64), plugged: true, charging: true };
    assert_eq!(listening().route(&Event::Power { power }, &settings), [Msg::Power(power)]);
    assert_eq!(listening().route(&Event::ScreenLock { locked: true }, &settings), [Msg::Locked(true)]);
    let sensors = Sensors { microphone: true, apps: vec!["Zoom".into()], ..Sensors::default() };
    assert_eq!(listening().route(&Event::Sensors { sensors: sensors.clone() }, &settings), [Msg::Sensors(sensors)]);
    let download = Download { id: "a".into(), name: "clip.mp4".into(), received: 5, total: Some(10) };
    let change = DownloadChange::Progress { download };
    assert_eq!(listening().route(&Event::Download { change: change.clone() }, &settings), [Msg::Download(change)]);
    let focus = Focus { active: true, name: Some("Work".into()) };
    assert_eq!(listening().route(&Event::Focus { focus: focus.clone() }, &settings), [Msg::Focus(focus)]);
}

#[test]
fn download_share_needs_total() {
    let known = Download { id: "a".into(), name: "a".into(), received: 25, total: Some(100) };
    assert_eq!(known.share(), Some(0.25));
    assert_eq!(Download { total: None, ..known.clone() }.share(), None);
    assert_eq!(Download { total: Some(0), ..known.clone() }.share(), None);
    assert_eq!(Download { received: 400, ..known }.share(), Some(1.0));
}

#[test]
fn old_peers_read_new_messages() {
    let power: Power = serde_json::from_str(r#"{"level":80}"#).unwrap();
    assert_eq!(power, Power { level: Some(80), plugged: false, charging: false });
    let event: Event = serde_json::from_str(r#"{"type":"screen_lock","locked":true}"#).unwrap();
    assert_eq!(event, Event::ScreenLock { locked: true });
    let request = serde_json::to_string(&Request::KeepAwake { awake: true }).unwrap();
    assert_eq!(request, r#"{"type":"keep_awake","awake":true}"#);
    let request = serde_json::to_string(&Request::MuteMicrophone { muted: false }).unwrap();
    assert_eq!(request, r#"{"type":"mute_microphone","muted":false}"#);
}

#[test]
fn tool_uses_host_network_and_files() {
    use crate::protocol::{HttpRequest, HttpResponse, Reply, Request};
    use crate::testing::Harness;
    use crate::{Command, Context, Tool};

    #[derive(Default)]
    struct Reader {
        got: Vec<String>,
    }

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Go,
        Fetched(Result<HttpResponse, String>),
        Read(Result<String, String>),
        Written(Result<String, String>),
    }

    impl Tool for Reader {
        type Msg = Msg;

        fn manifest() -> crate::protocol::Manifest {
            crate::protocol::Manifest::new("reader", "Reader")
        }

        fn init(_: &Context) -> (Self, Command<Msg>) {
            (Self::default(), Command::message(Msg::Go))
        }

        fn update(&mut self, msg: Msg) -> Command<Msg> {
            match msg {
                Msg::Go => Command::fetch(
                    HttpRequest::post("https://example.com/api", "{}").header("X-Key", "1"),
                    Msg::Fetched,
                ),
                Msg::Fetched(Ok(response)) => {
                    self.got.push(format!("{} {}", response.status, response.body));
                    Command::read_file("notes.txt", Msg::Read)
                }
                Msg::Read(Ok(text)) => {
                    self.got.push(text.clone());
                    Command::write_file("copy.txt", text, Msg::Written)
                }
                Msg::Written(Ok(path)) => {
                    self.got.push(path);
                    Command::none()
                }
                Msg::Fetched(Err(err)) | Msg::Read(Err(err)) | Msg::Written(Err(err)) => {
                    self.got.push(err);
                    Command::none()
                }
            }
        }
    }

    let mut tool = Harness::<Reader>::start();
    let Request::Fetch { request, .. } = &tool.requests()[0] else { panic!("{:?}", tool.requests()) };
    assert_eq!((request.method.as_str(), request.url.as_str()), ("POST", "https://example.com/api"));
    assert_eq!(
        (request.headers.clone(), request.body.as_deref()),
        (vec![("X-Key".to_owned(), "1".to_owned())], Some("{}"))
    );
    let read = tool.answer(Reply::Fetched { result: Ok(HttpResponse { status: 200, body: "hi".into() }) }).to_vec();
    assert!(matches!(&read[0], Request::ReadFile { path, .. } if path == "notes.txt"));
    let written = tool.answer(Reply::Read { result: Ok("kept".into()) }).to_vec();
    assert!(
        matches!(&written[0], Request::WriteFile { path, text, reply: Some(_) } if path == "copy.txt" && text == "kept")
    );
    tool.answer(Reply::Saved { result: Ok("/data/copy.txt".into()) });
    assert_eq!(tool.model().got, ["200 hi", "kept", "/data/copy.txt"]);
    let json = HttpRequest::get("https://example.com").json(&vec![1, 2]);
    assert_eq!((json.body.as_deref(), json.headers[0].1.as_str()), (Some("[1,2]"), "application/json"));
    let reply = HttpResponse { status: 404, body: "[1,2]".into() };
    assert_eq!((reply.ok(), reply.json::<Vec<u8>>()), (false, Ok(vec![1, 2])));
}
