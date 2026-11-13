//! Color picker: pick any color on screen and copy it as hex, RGB or HSL.

mod format;

use std::time::Duration;

use notchbase_sdk::prelude::*;

use format::Format;

pub const ID: &str = "color-picker";
const PICK: &str = "pick";
const FORMAT: &str = "format";
const COPIED_FOR: Duration = Duration::from_millis(2200);

#[derive(Default)]
pub struct ColorPicker {
    format: Format,
    picking: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Pick,
    Picked(Option<Color>),
    Configured(Settings),
}

impl Tool for ColorPicker {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Pick Color")
            .description("Pick any color on screen and copy it")
            .version(env!("CARGO_PKG_VERSION"))
            .shortcut(PICK, "Pick Color", Some("Ctrl+Shift+C"))
            .setting(
                SettingSpec::choice(FORMAT, "Color format", Format::CHOICES, "hex")
                    .detail("How picked colors are copied"),
            )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        (Self { format: Format::named(context.settings().text(FORMAT)), picking: false }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Pick if self.picking => {
                self.picking = false;
                Command::cancel_capture()
            }
            Msg::Pick => {
                self.picking = true;
                Command::pick_color(Msg::Picked)
            }
            Msg::Picked(color) => {
                self.picking = false;
                color.map_or_else(Command::none, |color| self.copied(color))
            }
            Msg::Configured(settings) => {
                self.format = Format::named(settings.text(FORMAT));
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::shortcut(PICK, Msg::Pick), Subscription::settings(Msg::Configured)])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        Some(Tile::new("Pick Color").icon(Icon::Pipette).shortcut(PICK).on_press(Msg::Pick))
    }
}

impl ColorPicker {
    fn copied(&self, color: Color) -> Command<Msg> {
        let written = self.format.write(color);
        let notice = Notice::new(adaptive(swatch(color).size(18.0), swatch(color).size(20.0)), text(&written).value())
            .pop()
            .duration(COPIED_FOR);
        Command::batch([Command::copy_text(written), Command::notice(notice)])
    }
}

notchbase_sdk::export!(ColorPicker);

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Event, Reply, Request, Start, Values};
    use notchbase_sdk::{Driver, Runtime, Waker};

    use super::*;

    fn started(format: &str) -> Runtime<ColorPicker> {
        let mut runtime = Runtime::new(Waker::noop());
        let mut settings = Values::new();
        settings.insert(FORMAT.into(), format.into());
        runtime.start(Start { settings, ..Start::default() });
        runtime
    }

    #[test]
    fn picked_color_is_copied() {
        let mut runtime = started("rgb");
        let effects = runtime.send(Msg::Pick);
        let [Request::PickColor { reply }] = effects.requests[..] else { panic!("{:?}", effects.requests) };
        let color = Some(Color::rgb(1, 2, 3));
        let effects = runtime.handle(vec![Event::Reply { id: reply, reply: Reply::Color { color } }]);
        assert!(matches!(&effects.requests[0], Request::CopyText { text } if text == "rgb(1, 2, 3)"));
        assert!(matches!(&effects.requests[1], Request::Notice { notice } if notice.pop));
    }

    #[test]
    fn shortcut_again_cancels() {
        let mut runtime = started("hex");
        runtime.handle(vec![Event::Shortcut { id: PICK.into() }]);
        let effects = runtime.handle(vec![Event::Shortcut { id: PICK.into() }]);
        assert_eq!(effects.requests, vec![Request::CancelCapture]);
        let effects = runtime.send(Msg::Picked(None));
        assert!(effects.requests.is_empty());
    }
}
