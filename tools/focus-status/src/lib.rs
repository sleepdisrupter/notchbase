//! Focus status: a short notice when a Focus or Do Not Disturb turns on or off.

use std::time::Duration;

use notchbase_sdk::prelude::*;

pub const ID: &str = "focus-status";
const ANNOUNCE: &str = "announce";
const GLYPH: f32 = 15.0;
const SHOWN: Duration = Duration::from_millis(2600);
const FOCUS_OFF: &str = "Focus off";
const ANY_FOCUS: &str = "Focus";

pub struct FocusStatus {
    focus: Option<Focus>,
    announce: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Changed(Focus),
    Configured(Settings),
}

impl Tool for FocusStatus {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Focus")
            .background()
            .description("Shows when a Focus or Do Not Disturb turns on or off")
            .version(env!("CARGO_PKG_VERSION"))
            .setting(
                SettingSpec::toggle(ANNOUNCE, "Show Focus changes", true)
                    .detail("On a Mac this needs Full Disk Access for notchbase"),
            )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        (Self { focus: None, announce: context.settings().bool(ANNOUNCE) }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Changed(focus) => {
                let before = self.focus.replace(focus.clone());
                match before {
                    Some(before) if self.announce && before != focus => Command::notice(notice(&focus)),
                    _ => Command::none(),
                }
            }
            Msg::Configured(settings) => {
                self.announce = settings.bool(ANNOUNCE);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::focus(Msg::Changed), Subscription::settings(Msg::Configured)])
    }
}

fn notice(focus: &Focus) -> Notice {
    let (tone, words) = match (focus.active, focus.name.as_deref()) {
        (true, name) => (Tone::Purple, text(name.unwrap_or(ANY_FOCUS)).callout()),
        (false, _) => (Tone::Secondary, text(FOCUS_OFF).callout().secondary()),
    };
    Notice::new(icon(Icon::Moon).size(GLYPH).tone(tone), words).pop().duration(SHOWN)
}

notchbase_sdk::export!(FocusStatus);

#[cfg(test)]
mod tests {
    use notchbase_sdk::testing::Harness;

    use super::*;

    fn on(name: &str) -> Focus {
        Focus { active: true, name: Some(name.into()) }
    }

    #[test]
    fn shows_changes_not_first_reading() {
        let mut harness = Harness::<FocusStatus>::start();
        assert!(harness.focus(Focus::default()).is_empty());
        harness.focus(on("Work"));
        assert_eq!(harness.noticed(), ["Work"]);
        assert!(harness.focus(on("Work")).is_empty());
        harness.focus(on("Sleep"));
        assert_eq!(harness.noticed(), ["Sleep"], "switching straight to another Focus is news too");
        harness.focus(Focus::default());
        assert_eq!(harness.noticed(), [FOCUS_OFF]);
    }

    #[test]
    fn can_stay_quiet() {
        let mut harness = Harness::<FocusStatus>::start_with([(ANNOUNCE, false)]);
        harness.focus(Focus::default());
        assert!(harness.focus(on("Work")).is_empty());
    }

    #[test]
    fn unnamed_focus_is_just_focus() {
        let mut harness = Harness::<FocusStatus>::start();
        harness.focus(Focus::default());
        harness.focus(Focus { active: true, name: None });
        assert_eq!(harness.noticed(), [ANY_FOCUS]);
    }
}
