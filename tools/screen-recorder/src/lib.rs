//! Record screen: records an area of the screen, with the microphone and desktop audio if wanted.
//! Finished recordings land on the Shelf.

mod options;

use std::time::Duration;

use notchbase_sdk::prelude::*;

use options::Options;

pub const ID: &str = "screen-recorder";
const RECORD: &str = "record";
const CHIP_GAP: f32 = 5.0;
const SAVED_FOR: Duration = Duration::from_millis(2600);
const DOT: f32 = 9.0;
const WELL_DOT: f32 = 12.0;
const STOP: f32 = 9.0;
const DEFAULT_MICROPHONE: &str = "Default Microphone";
const NO_MICROPHONE: &str = "No Microphone";
const MICROPHONE_OFF: usize = 0; // the menu lists "No Microphone" first, then each input

pub struct ScreenRecorder {
    options: Options,
    state: RecordingState,
    inputs: Vec<AudioInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Toggle,
    Recording(RecordingUpdate),
    Inputs(Vec<AudioInput>),
    Microphone(bool),
    DesktopAudio(bool),
    ChooseMicrophone(usize),
    Configured(Settings),
}

impl Tool for ScreenRecorder {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Options::declare(
            Manifest::new(ID, "Record Screen")
                .description("Record an area of the screen with microphone and desktop audio")
                .version(env!("CARGO_PKG_VERSION"))
                .shortcut(RECORD, "Record Screen", Some("Ctrl+Alt+R")),
        )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let recorder =
            Self { options: Options::from(context.settings()), state: RecordingState::Idle, inputs: Vec::new() };
        (recorder, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Toggle => self.toggle(),
            Msg::Recording(update) => self.follow(update),
            Msg::Inputs(inputs) => {
                self.inputs = inputs;
                Command::none()
            }
            Msg::Microphone(on) => {
                self.options.microphone = on;
                Command::save_setting(options::MICROPHONE, on)
            }
            Msg::DesktopAudio(on) => {
                self.options.desktop_audio = on;
                Command::save_setting(options::DESKTOP_AUDIO, on)
            }
            Msg::ChooseMicrophone(MICROPHONE_OFF) => self.update(Msg::Microphone(false)),
            Msg::ChooseMicrophone(choice) => {
                let on = self.update(Msg::Microphone(true));
                match self.inputs.get(choice - 1) {
                    Some(input) => {
                        self.options.microphone_id = Some(input.id.clone());
                        on.and(Command::save_setting(options::MICROPHONE_ID, input.id.clone()))
                    }
                    None => on,
                }
            }
            Msg::Configured(settings) => {
                self.options = Options::from(&settings);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::shortcut(RECORD, Msg::Toggle),
            Subscription::recording(Msg::Recording),
            Subscription::audio_inputs(Msg::Inputs),
            Subscription::settings(Msg::Configured),
        ])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new("Record Screen").shortcut(RECORD).on_press(Msg::Toggle);
        Some(match self.state {
            RecordingState::Idle | RecordingState::Choosing => self.idle_tile(tile),
            RecordingState::Starting => {
                live_tile(tile, "Recording").footer(text("Starting…").subheadline().secondary())
            }
            RecordingState::Recording { since } => {
                live_tile(tile, "Recording").footer(timer(since).style(TextStyle::Display))
            }
            RecordingState::Saving => live_tile(tile, "Saving…").enabled(false),
        })
    }

    fn live(&self) -> Option<Live<Msg>> {
        let RecordingState::Recording { since } = self.state else { return None };
        let stop = button("").icon(Icon::Stop).on_press(Msg::Toggle);
        Some(Live::new(dot(Tone::Red, DOT), timer(since)).controls(stop))
    }
}

impl ScreenRecorder {
    fn toggle(&mut self) -> Command<Msg> {
        match self.state {
            RecordingState::Idle => Command::start_recording(self.options.recording()),
            RecordingState::Choosing => Command::cancel_capture(),
            RecordingState::Starting | RecordingState::Recording { .. } => Command::stop_recording(),
            RecordingState::Saving => Command::none(),
        }
    }

    fn follow(&mut self, update: RecordingUpdate) -> Command<Msg> {
        match update {
            RecordingUpdate::State { state } => {
                self.state = state;
                Command::none()
            }
            RecordingUpdate::Saved { path, seconds, note } => {
                let shown = match note {
                    Some(note) => Command::notify(note),
                    None => Command::notice(saved(seconds)),
                };
                Command::shelve([path]).and(shown)
            }
            RecordingUpdate::Failed { error } => Command::notify(error),
        }
    }

    fn idle_tile(&self, tile: Tile<Msg>) -> Tile<Msg> {
        let desktop =
            chip("", self.options.desktop_audio).icons(Icon::Speaker, Icon::SpeakerMuted).on_toggle(Msg::DesktopAudio);
        let controls = row![self.microphone_menu(), desktop].spacing(CHIP_GAP);
        tile.symbol(dot(Tone::Red, WELL_DOT)).footer(controls)
    }

    fn microphone_menu(&self) -> Element<Msg> {
        let off = [(Icon::MicrophoneMuted, NO_MICROPHONE.to_owned())];
        let names: Vec<(Icon, String)> = if self.inputs.is_empty() {
            vec![(Icon::Microphone, DEFAULT_MICROPHONE.to_owned())]
        } else {
            self.inputs.iter().map(|input| (Icon::Microphone, input.name.clone())).collect()
        };
        let chosen = self.options.microphone.then(|| self.selected().map_or(1, |index| index + 1));
        dropdown(off.into_iter().chain(names), chosen).compact().on_select(Msg::ChooseMicrophone).into()
    }

    fn selected(&self) -> Option<usize> {
        let chosen = self.options.microphone_id.as_deref();
        chosen
            .and_then(|id| self.inputs.iter().position(|input| input.id == id))
            .or_else(|| self.inputs.iter().position(|input| input.is_default))
    }
}

fn live_tile(tile: Tile<Msg>, title: &str) -> Tile<Msg> {
    tile.title(title).symbol(icon(Icon::Stop).size(STOP).tone(Tone::White)).well(Tone::Red).tint(Tone::Red)
}

fn saved(seconds: f64) -> Notice {
    let length = clock(Duration::from_secs_f64(seconds.max(0.0).round()));
    let island = row![text("Saved").callout(), text(&length).value().secondary()].spacing(6.0);
    let notch = text(length).value().secondary();
    let check = adaptive(badge(Icon::Checkmark, Tone::Green, 16.0), badge(Icon::Checkmark, Tone::Green, 18.0));
    Notice::new(check, adaptive(notch, island)).pop().size(NoticeSize::Small).duration(SAVED_FOR)
}

notchbase_sdk::export!(ScreenRecorder);

#[cfg(test)]
mod tests;
