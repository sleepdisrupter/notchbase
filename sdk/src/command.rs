//! Commands: what a tool asks for when it returns from `init` or `update`.

use std::path::Path;
use std::sync::Arc;

use crate::driver::Emitter;
use crate::element::Element;
use crate::protocol::{
    Capture, Color, EventDraft, HttpRequest, HttpResponse, ImageFormat, ImageId, MediaControl, RecordingOptions, Reply,
    Request, Value,
};
use crate::surface::Notice;

type Ask<Msg> = Box<dyn FnOnce(Reply) -> Option<Msg>>;
type Task<Msg> = Box<dyn FnOnce(Emitter<Msg>) + Send>;

pub(crate) enum Effect<Msg> {
    Message(Msg),
    Request(Box<Request>),
    Ask { build: Box<dyn FnOnce(u64) -> Request>, answer: Ask<Msg> },
    Notice { notice: Box<Notice>, controls: Box<Element<Msg>> },
    Task(Task<Msg>),
}

/// Work for the host or for a background thread. Combine several with `batch` or `and`.
pub struct Command<Msg> {
    pub(crate) effects: Vec<Effect<Msg>>,
}

impl<Msg: Send + 'static> Default for Command<Msg> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Msg: Send + 'static> Command<Msg> {
    pub fn none() -> Self {
        Self { effects: Vec::new() }
    }

    pub fn batch(commands: impl IntoIterator<Item = Command<Msg>>) -> Self {
        Self { effects: commands.into_iter().flat_map(|command| command.effects).collect() }
    }

    pub fn and(mut self, other: Command<Msg>) -> Self {
        self.effects.extend(other.effects);
        self
    }

    pub fn message(msg: Msg) -> Self {
        Self::effect(Effect::Message(msg))
    }

    /// Runs `task` in the background and turns its result into a message.
    pub fn perform<T: 'static>(
        task: impl FnOnce() -> T + Send + 'static,
        map: impl FnOnce(T) -> Msg + Send + 'static,
    ) -> Self {
        Self::spawn(move |emitter| emitter.send(map(task())))
    }

    /// Runs `task` in the background. It can send any number of messages through the emitter.
    /// Portable (WebAssembly) builds have no threads, so there the task runs right away.
    pub fn spawn(task: impl FnOnce(Emitter<Msg>) + Send + 'static) -> Self {
        Self::effect(Effect::Task(Box::new(task)))
    }

    /// Shows a short message in the notch.
    pub fn notify(text: impl Into<String>) -> Self {
        Self::request(Request::Notify { text: text.into() })
    }

    pub fn notice(notice: Notice) -> Self {
        Self::request(Request::Notice { notice: notice.into_node() })
    }

    /// A notice with controls, such as buttons, whose messages come back to this tool.
    pub fn notice_with_controls(notice: Notice, controls: impl Into<Element<Msg>>) -> Self {
        Self::effect(Effect::Notice { notice: Box::new(notice), controls: Box::new(controls.into()) })
    }

    pub fn dismiss_notice() -> Self {
        Self::request(Request::DismissNotice)
    }

    pub fn copy_text(text: impl Into<String>) -> Self {
        Self::request(Request::CopyText { text: text.into() })
    }

    pub fn copy_image(image: ImageId) -> Self {
        Self::request(Request::CopyImage { image })
    }

    /// Opens the image in the screenshot editor.
    pub fn annotate(image: ImageId) -> Self {
        Self::request(Request::Annotate { image })
    }

    pub fn save_image(
        image: ImageId,
        folder: Option<&Path>,
        prefix: impl Into<String>,
        format: ImageFormat,
        map: impl FnOnce(Result<String, String>) -> Msg + 'static,
    ) -> Self {
        let folder = folder.map(|folder| folder.to_string_lossy().into_owned());
        let prefix = prefix.into();
        Self::ask(
            move |reply| Request::SaveImage { image, folder, prefix, format, reply: Some(reply) },
            move |reply| match reply {
                Reply::Saved { result } => Some(map(result)),
                _ => None,
            },
        )
    }

    pub fn pick_color(map: impl FnOnce(Option<Color>) -> Msg + 'static) -> Self {
        Self::ask(
            |reply| Request::PickColor { reply },
            move |reply| match reply {
                Reply::Color { color } => Some(map(color)),
                _ => None,
            },
        )
    }

    pub fn capture_screen(map: impl FnOnce(Option<Capture>) -> Msg + 'static) -> Self {
        Self::ask(
            |reply| Request::CaptureScreen { reply },
            move |reply| match reply {
                Reply::Capture { capture } => Some(map(capture)),
                _ => None,
            },
        )
    }

    pub fn fetch(request: HttpRequest, map: impl FnOnce(Result<HttpResponse, String>) -> Msg + 'static) -> Self {
        Self::ask(
            |reply| Request::Fetch { request, reply },
            move |reply| match reply {
                Reply::Fetched { result } => Some(map(result)),
                _ => None,
            },
        )
    }

    /// Reads a text file. A relative path is inside the tool's data folder, and `~/` starts at home.
    pub fn read_file(path: impl Into<String>, map: impl FnOnce(Result<String, String>) -> Msg + 'static) -> Self {
        let path = path.into();
        Self::ask(
            |reply| Request::ReadFile { path, reply },
            move |reply| match reply {
                Reply::Read { result } => Some(map(result)),
                _ => None,
            },
        )
    }

    /// Writes a text file, with paths read the same way as `read_file`.
    pub fn write_file(
        path: impl Into<String>,
        text: impl Into<String>,
        map: impl FnOnce(Result<String, String>) -> Msg + 'static,
    ) -> Self {
        let (path, text) = (path.into(), text.into());
        Self::ask(
            |reply| Request::WriteFile { path, text, reply: Some(reply) },
            move |reply| match reply {
                Reply::Saved { result } => Some(map(result)),
                _ => None,
            },
        )
    }

    pub fn start_recording(options: RecordingOptions) -> Self {
        Self::request(Request::StartRecording { options })
    }

    pub fn stop_recording() -> Self {
        Self::request(Request::StopRecording)
    }

    pub fn cancel_capture() -> Self {
        Self::request(Request::CancelCapture)
    }

    pub fn save_setting(key: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::request(Request::SaveSetting { key: key.into(), value: value.into() })
    }

    /// Puts files on the Shelf.
    pub fn shelve(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Self {
        let paths = paths.into_iter().map(|path| path.as_ref().to_string_lossy().into_owned()).collect();
        Self::request(Request::Shelve { paths })
    }

    pub fn reveal(path: impl AsRef<Path>) -> Self {
        Self::request(Request::Reveal { path: path.as_ref().to_string_lossy().into_owned() })
    }

    pub fn open(target: impl Into<String>) -> Self {
        Self::request(Request::Open { target: target.into() })
    }

    pub fn open_page() -> Self {
        Self::request(Request::OpenPage)
    }

    pub fn collapse() -> Self {
        Self::request(Request::Collapse)
    }

    /// Keeps the notch open until `release`.
    pub fn hold_open() -> Self {
        Self::request(Request::Hold { held: true })
    }

    pub fn release() -> Self {
        Self::request(Request::Hold { held: false })
    }

    pub fn media(control: MediaControl) -> Self {
        Self::request(Request::Media { control })
    }

    /// Plays the notchbase chime.
    pub fn chime() -> Self {
        Self::request(Request::Chime)
    }

    pub fn open_settings() -> Self {
        Self::request(Request::OpenSettings)
    }

    pub fn save_event(event: EventDraft) -> Self {
        Self::request(Request::SaveEvent { event })
    }

    pub fn delete_event(id: impl Into<String>) -> Self {
        Self::request(Request::DeleteEvent { id: id.into() })
    }

    pub fn keep_awake(awake: bool) -> Self {
        Self::request(Request::KeepAwake { awake })
    }

    pub fn mute_microphone(muted: bool) -> Self {
        Self::request(Request::MuteMicrophone { muted })
    }

    /// Writes a line to notchbase's log.
    pub fn log(text: impl Into<String>) -> Self {
        Self::request(Request::Log { text: text.into() })
    }

    /// Sends any request as it is, for the ones without a helper of their own.
    pub fn request(request: Request) -> Self {
        Self::effect(Effect::Request(Box::new(request)))
    }

    pub fn map<Outer: Send + 'static>(self, map: impl Fn(Msg) -> Outer + Send + Sync + 'static) -> Command<Outer> {
        let map = Arc::new(map);
        let effects = self
            .effects
            .into_iter()
            .map(|effect| match effect {
                Effect::Message(msg) => Effect::Message(map(msg)),
                Effect::Request(request) => Effect::Request(request),
                Effect::Ask { build, answer } => {
                    let map = map.clone();
                    Effect::Ask { build, answer: Box::new(move |reply| answer(reply).map(|msg| map(msg))) }
                }
                Effect::Notice { notice, controls } => {
                    let map = map.clone();
                    Effect::Notice { notice, controls: Box::new(controls.map(move |msg| map(msg))) }
                }
                Effect::Task(task) => {
                    let map = map.clone();
                    Effect::Task(Box::new(move |emitter: Emitter<Outer>| task(emitter.map(move |msg| map(msg)))))
                }
            })
            .collect();
        Command { effects }
    }

    fn ask(build: impl FnOnce(u64) -> Request + 'static, answer: impl FnOnce(Reply) -> Option<Msg> + 'static) -> Self {
        Self::effect(Effect::Ask { build: Box::new(build), answer: Box::new(answer) })
    }

    fn effect(effect: Effect<Msg>) -> Self {
        Self { effects: vec![effect] }
    }
}
