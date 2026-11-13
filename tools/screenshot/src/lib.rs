//! Screenshot: captures an area of the screen, copies or saves it, and offers to annotate it.

use std::path::PathBuf;
use std::time::Duration;

use notchbase_sdk::prelude::*;

pub const ID: &str = "screenshot";
const CHECK: f32 = 12.0;
const CAPTURE: &str = "capture";
const COPY: &str = "copy";
const SAVE: &str = "save";
const FOLDER: &str = "folder";
const FORMAT: &str = "format";
const PREFIX: &str = "Screenshot";
const DEFAULT_FOLDER: &str = "Pictures/Screenshots";
const ANNOTATE: &str = "Annotate";
const SHOWN_FOR: Duration = Duration::from_secs(5); // long enough to reach the Annotate button
const WIDE_ASPECT: f32 = 1.5;

#[derive(Clone, Debug, PartialEq)]
struct Prefs {
    copy: bool,
    save: bool,
    folder: Option<PathBuf>,
    format: ImageFormat,
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self {
            copy: settings.bool(COPY),
            save: settings.bool(SAVE),
            folder: settings.path(FOLDER),
            format: if settings.text(FORMAT) == "jpeg" { ImageFormat::Jpeg } else { ImageFormat::Png },
        }
    }
}

pub struct Screenshot {
    prefs: Prefs,
    capturing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Capture,
    Captured(Option<Capture>),
    Annotate(ImageId),
    Saved(Result<String, String>),
    Configured(Settings),
}

impl Tool for Screenshot {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Screenshot")
            .description("Capture a region of the screen")
            .version(env!("CARGO_PKG_VERSION"))
            .shortcut(CAPTURE, "Screenshot", Some("Ctrl+Alt+S"))
            .setting(SettingSpec::toggle(COPY, "Copy screenshots", true).detail("Put each screenshot on the clipboard"))
            .setting(SettingSpec::toggle(SAVE, "Save screenshots", false).detail("Also keep a file of each one"))
            .setting(SettingSpec::folder(FOLDER, "Screenshot folder").placeholder(DEFAULT_FOLDER).shown_when(SAVE))
            .setting(
                SettingSpec::choice(FORMAT, "File format", [("png", "PNG"), ("jpeg", "JPEG")], "png").shown_when(SAVE),
            )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        (Self { prefs: Prefs::from(context.settings()), capturing: false }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Capture if self.capturing => {
                self.capturing = false;
                Command::cancel_capture()
            }
            Msg::Capture => {
                self.capturing = true;
                Command::capture_screen(Msg::Captured)
            }
            Msg::Captured(capture) => {
                self.capturing = false;
                capture.map_or_else(Command::none, |capture| self.keep(capture))
            }
            Msg::Annotate(image) => Command::dismiss_notice().and(Command::annotate(image)),
            Msg::Saved(Err(err)) => Command::notify(err),
            Msg::Saved(Ok(_)) => Command::none(),
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::shortcut(CAPTURE, Msg::Capture), Subscription::settings(Msg::Configured)])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        Some(Tile::new("Screenshot").icon(Icon::Viewfinder).shortcut(CAPTURE).on_press(Msg::Capture))
    }
}

impl Screenshot {
    fn keep(&self, capture: Capture) -> Command<Msg> {
        // Some capture tools put the image on the clipboard themselves. A capture that isn't copied
        // is always saved, so it isn't lost.
        let copy = self.prefs.copy && !capture.on_clipboard;
        let copied = self.prefs.copy || capture.on_clipboard;
        let shown = notice(&capture, if copied { "Copied" } else { "Saved" });
        let annotate = button(ANNOTATE).on_press(Msg::Annotate(capture.image));
        let mut commands = vec![Command::notice_with_controls(shown, annotate)];
        if copy {
            commands.push(Command::copy_image(capture.image));
        }
        if self.prefs.save || !copied {
            let folder = self.prefs.folder.as_deref();
            commands.push(Command::save_image(capture.image, folder, PREFIX, self.prefs.format, Msg::Saved));
        }
        Command::batch(commands)
    }
}

fn notice(capture: &Capture, done: &str) -> Notice {
    let aspect = capture.width as f32 / capture.height.max(1) as f32;
    // Wide shots get 30 pt of width, the rest 20 pt of height.
    let [width, height] = if aspect >= WIDE_ASPECT { [30.0, 30.0 / aspect] } else { [20.0 * aspect, 20.0] };
    let thumbnail = image(ImageSource::Capture(capture.image)).size(width, height).radius(5.0);
    let size = text(format!("{}×{}", capture.width, capture.height)).value().secondary();
    let island = row![text(done).callout(), size].spacing(6.0);
    let notch = icon(Icon::Checkmark).size(CHECK).tone(Tone::Green);
    Notice::new(thumbnail, adaptive(notch, island)).pop().size(NoticeSize::Large).duration(SHOWN_FOR)
}

notchbase_sdk::export!(Screenshot);

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Event, Node, Reply, Request, Start, Surface, UiValue, Values};
    use notchbase_sdk::{Driver, Runtime, Waker};

    use super::*;

    fn started(save: bool) -> Runtime<Screenshot> {
        let mut runtime = Runtime::new(Waker::noop());
        let mut settings = Values::new();
        settings.insert(SAVE.into(), save.into());
        runtime.start(Start { settings, ..Start::default() });
        runtime
    }

    fn captured(runtime: &mut Runtime<Screenshot>, on_clipboard: bool) -> Vec<Request> {
        let effects = runtime.send(Msg::Capture);
        let [Request::CaptureScreen { reply }] = effects.requests[..] else { panic!("{:?}", effects.requests) };
        let capture = Capture { image: ImageId(7), width: 2560, height: 1600, on_clipboard };
        runtime.handle(vec![Event::Reply { id: reply, reply: Reply::Capture { capture: Some(capture) } }]).requests
    }

    #[test]
    fn capture_is_copied_and_shown() {
        let requests = captured(&mut started(false), false);
        assert!(matches!(requests[0], Request::Notice { .. }));
        assert_eq!(requests[1], Request::CopyImage { image: ImageId(7) });
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn notice_offers_annotate() {
        let requests = captured(&mut started(false), false);
        let Request::Notice { notice } = &requests[0] else { panic!("{:?}", requests) };
        assert!(matches!(&notice.controls, Some(Node::Button { label, on_press: Some(_), .. }) if label == ANNOTATE));
        assert_eq!(notice.duration_ms, 5_000);
    }

    #[test]
    fn annotate_asks_the_host() {
        let mut runtime = started(false);
        let requests = captured(&mut runtime, false);
        let Request::Notice { notice } = &requests[0] else { panic!("{:?}", requests) };
        let Some(Node::Button { on_press: Some(handler), .. }) = notice.controls else { panic!("{notice:?}") };
        runtime.views();
        let pressed = Event::Ui { surface: Surface::Notice, handler, value: UiValue::Press };
        let answered = runtime.handle(vec![pressed]).requests;
        assert_eq!(answered, [Request::DismissNotice, Request::Annotate { image: ImageId(7) }]);
    }

    #[test]
    fn saved_only_offers_annotate() {
        let mut runtime = Runtime::<Screenshot>::new(Waker::noop());
        let mut settings = Values::new();
        settings.insert(COPY.into(), false.into());
        runtime.start(Start { settings, ..Start::default() });
        let requests = captured(&mut runtime, false);
        assert!(matches!(&requests[0], Request::Notice { notice } if notice.controls.is_some()));
        assert!(matches!(&requests[1], Request::SaveImage { .. }));
    }

    #[test]
    fn clipboard_capture_saved_if_asked() {
        assert_eq!(captured(&mut started(false), true).len(), 1);
        let requests = captured(&mut started(true), true);
        assert!(matches!(&requests[1], Request::SaveImage { prefix, .. } if prefix == PREFIX));
    }
}
