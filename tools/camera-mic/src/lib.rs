//! Camera and mic: a live activity while either is in use, with the app's name and a mute button.

use notchbase_sdk::prelude::*;

pub const ID: &str = "camera-mic";
const CAMERA: &str = "camera";
const MICROPHONE: &str = "microphone";
const DOT: f32 = 8.0;
const DOT_GAP: f32 = 4.0;
const GLYPH: f32 = 13.0;
const CONTROL_GAP: f32 = 6.0;
const LONGEST: usize = 18;
const CAMERA_ON: &str = "Camera";
const MICROPHONE_ON: &str = "Microphone";
const BOTH_ON: &str = "Camera & Mic";
const MUTED: &str = "Muted";

pub struct CameraMic {
    sensors: Sensors,
    prefs: Prefs,
    hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefs {
    camera: bool,
    microphone: bool,
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self { camera: settings.bool(CAMERA), microphone: settings.bool(MICROPHONE) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Sensors(Sensors),
    Mute(bool),
    Hide,
    Configured(Settings),
}

impl Tool for CameraMic {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Camera & Microphone")
            .background()
            .description("Shows when the camera or microphone is in use")
            .version(env!("CARGO_PKG_VERSION"))
            .setting(SettingSpec::toggle(CAMERA, "Camera", true).detail("Show a green dot while the camera is on"))
            .setting(
                SettingSpec::toggle(MICROPHONE, "Microphone", true)
                    .detail("Show an orange dot while an app is listening"),
            )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = Prefs::from(context.settings());
        (Self { sensors: Sensors::default(), prefs, hidden: false }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Sensors(sensors) => {
                self.sensors = sensors;
                self.hidden &= self.camera() || self.microphone();
                Command::none()
            }
            Msg::Mute(muted) => {
                self.sensors.muted = muted;
                Command::mute_microphone(muted)
            }
            Msg::Hide => {
                self.hidden = true;
                Command::none()
            }
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::sensors(Msg::Sensors), Subscription::settings(Msg::Configured)])
    }

    fn live(&self) -> Option<Live<Msg>> {
        let (camera, microphone) = (self.camera(), self.microphone());
        if self.hidden || !(camera || microphone) {
            return None;
        }
        let dots = [camera.then_some(Tone::Green), microphone.then_some(Tone::Orange)];
        let leading = row(dots.into_iter().flatten().map(|tone| dot(tone, DOT))).spacing(DOT_GAP);
        let muted = microphone && self.sensors.muted;
        let words = text(self.words()).callout().secondary();
        let named: Element<Msg> = match muted {
            true => row![words, text(MUTED).callout().tone(Tone::Orange)].spacing(CONTROL_GAP).into(),
            false => words.into(),
        };
        let listening = if muted { (Icon::MicrophoneMuted, Tone::Orange) } else { (Icon::Microphone, Tone::Secondary) };
        let marks = [camera.then_some((Icon::Video, Tone::Secondary)), microphone.then_some(listening)];
        let marked = row(marks.into_iter().flatten().map(|(glyph, tone)| icon(glyph).size(GLYPH).tone(tone).into()))
            .spacing(CONTROL_GAP);
        let trailing = adaptive(marked, named);
        let mute = microphone.then(|| {
            let glyph = if muted { Icon::MicrophoneMuted } else { Icon::Microphone };
            button("").icon(glyph).on_press(Msg::Mute(!muted)).into()
        });
        let hide = button("").icon(Icon::Xmark).on_press(Msg::Hide).into();
        Some(Live::new(leading, trailing).controls(row(mute.into_iter().chain([hide])).spacing(CONTROL_GAP)))
    }
}

impl CameraMic {
    fn camera(&self) -> bool {
        self.prefs.camera && self.sensors.camera
    }

    fn microphone(&self) -> bool {
        self.prefs.microphone && self.sensors.microphone
    }

    fn words(&self) -> String {
        match (self.sensors.apps.as_slice(), self.camera(), self.microphone()) {
            ([app], _, _) => shortened(app),
            ([app, others @ ..], _, _) => format!("{} +{}", shortened(app), others.len()),
            ([], true, true) => BOTH_ON.to_owned(),
            ([], true, false) => CAMERA_ON.to_owned(),
            ([], false, _) => MICROPHONE_ON.to_owned(),
        }
    }
}

fn shortened(name: &str) -> String {
    match name.char_indices().nth(LONGEST) {
        Some((at, _)) => format!("{}\u{2026}", name[..at].trim_end()),
        None => name.to_owned(),
    }
}

notchbase_sdk::export!(CameraMic);

#[cfg(test)]
mod tests;
