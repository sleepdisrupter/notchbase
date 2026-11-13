//! The options the setup app passes along with --install.

const NO_LOGIN: &str = "--no-login";
const NO_UPDATES: &str = "--no-updates";
const NO_ASSISTANT: &str = "--no-assistant";
const DESKTOP: &str = "--desktop";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choices {
    pub login: bool,
    pub updates_off: bool,
    pub assistant_off: bool,
    pub desktop: bool,
}

impl Default for Choices {
    fn default() -> Self {
        Self { login: true, updates_off: false, assistant_off: false, desktop: false }
    }
}

impl Choices {
    pub fn from_args(args: &[String]) -> Self {
        let has = |flag: &str| args.iter().any(|arg| arg == flag);
        Self {
            login: !has(NO_LOGIN),
            updates_off: has(NO_UPDATES),
            assistant_off: has(NO_ASSISTANT),
            desktop: has(DESKTOP),
        }
    }

    pub fn to_args(self) -> Vec<&'static str> {
        [
            (!self.login, NO_LOGIN),
            (self.updates_off, NO_UPDATES),
            (self.assistant_off, NO_ASSISTANT),
            (self.desktop, DESKTOP),
        ]
        .into_iter()
        .filter_map(|(on, flag)| on.then_some(flag))
        .collect()
    }

    pub fn changes_settings(&self) -> bool {
        self.updates_off || self.assistant_off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn every_choice() -> impl Iterator<Item = Choices> {
        (0..16u8).map(|bits| Choices {
            login: bits & 1 != 0,
            updates_off: bits & 2 != 0,
            assistant_off: bits & 4 != 0,
            desktop: bits & 8 != 0,
        })
    }

    #[test]
    fn bare_install_keeps_defaults() {
        let choices = Choices::from_args(&args(&["--install"]));
        assert_eq!(choices, Choices::default());
        assert!(!choices.changes_settings());
    }

    #[test]
    fn setup_flags_turn_things_off() {
        let choices = Choices::from_args(&args(&["--install", "--no-login", "--no-updates", "--desktop"]));
        assert_eq!(choices, Choices { login: false, updates_off: true, assistant_off: false, desktop: true });
        assert!(choices.changes_settings());
    }

    #[test]
    fn only_changes_become_flags() {
        assert!(Choices::default().to_args().is_empty());
        let everything = Choices { login: false, updates_off: true, assistant_off: true, desktop: true };
        assert_eq!(everything.to_args(), ["--no-login", "--no-updates", "--no-assistant", "--desktop"]);
        let quiet = Choices { assistant_off: true, ..Choices::default() };
        assert_eq!(quiet.to_args(), ["--no-assistant"]);
    }

    #[test]
    fn choices_round_trip() {
        for choices in every_choice() {
            let written: Vec<String> = choices.to_args().into_iter().map(str::to_owned).collect();
            assert_eq!(Choices::from_args(&written), choices, "{written:?}");
        }
    }

    #[test]
    fn ignores_unknown_and_repeated_args() {
        let choices = Choices::from_args(&args(&["--install", "--frobnicate", "--desktop", "--desktop", "--NO-LOGIN"]));
        assert_eq!(choices, Choices { desktop: true, ..Choices::default() });
        assert_eq!(Choices::from_args(&[]), Choices::default());
    }

    #[test]
    fn updates_or_assistant_change_settings() {
        for choices in every_choice() {
            assert_eq!(choices.changes_settings(), choices.updates_off || choices.assistant_off, "{choices:?}");
        }
    }
}
