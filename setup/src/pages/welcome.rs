//! The first page: the install options and the main button.

use egui::Id;
use notchbase_paint::canvas::Canvas;
use notchbase_paint::list::{self, Lines};

use crate::install::{Installed, Latest, Options};
use crate::widgets::{self, Choice};

use super::{Intent, Scene, card_rect, footer_row, rows, section, settings};

const MAIN: &str = "main";
const INSTALL: &str = "Install";
const UPDATE: &str = "Update";
const TRY_AGAIN: &str = "Try Again";

pub struct Welcome<'a> {
    pub options: &'a Options,
    pub latest: &'a Latest,
    pub installed: Option<Installed>,
}

pub fn welcome(scene: &Scene<'_>, canvas: &mut Canvas, welcome: &Welcome<'_>) -> Option<Intent> {
    section(scene, canvas, "Options");
    let rects = widgets::card(canvas, card_rect(scene, rows()), rows());
    let mut intent = None;
    for ((setting, title, detail), rect) in settings().into_iter().zip(rects) {
        let text = Lines::new(title, detail);
        let id = Id::new(("setup.setting", setting as u8));
        if list::switch_row(scene.ui, canvas, rect, id, &text, welcome.options.get(setting), scene.live) {
            intent = Some(Intent::Toggle(setting));
        }
    }
    let offers = footer_choices(welcome.latest, welcome.installed);
    let buttons: Vec<Choice<'_>> = offers.iter().map(Offer::choice).collect();
    let pressed = widgets::footer(scene.ui, canvas, footer_row(scene), &buttons, scene.live);
    pressed.map(|index| offers[index].intent).or(intent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offer {
    key: &'static str,
    title: &'static str,
    primary: bool,
    enabled: bool,
    intent: Intent,
}

impl Offer {
    const fn main(title: &'static str, intent: Intent) -> Self {
        Self { key: MAIN, title, primary: true, enabled: true, intent }
    }

    const fn uninstall(primary: bool) -> Self {
        Self { key: "uninstall", title: "Uninstall", primary, enabled: true, intent: Intent::AskRemove }
    }

    const fn reinstall() -> Self {
        Self { key: "reinstall", title: "Reinstall", primary: false, enabled: true, intent: Intent::Install }
    }

    const fn waiting(self) -> Self {
        Self { enabled: false, ..self }
    }

    fn choice(&self) -> Choice<'_> {
        Choice { key: self.key, title: self.title, primary: self.primary, enabled: self.enabled }
    }
}

fn footer_choices(latest: &Latest, installed: Option<Installed>) -> Vec<Offer> {
    match (latest, installed) {
        (Latest::Failed(_), Some(_)) => vec![Offer::uninstall(false), Offer::main(TRY_AGAIN, Intent::Retry)],
        (Latest::Failed(_), None) => vec![Offer::main(TRY_AGAIN, Intent::Retry)],
        (Latest::Checking, Some(_)) => vec![Offer::uninstall(false), Offer::main(UPDATE, Intent::Install).waiting()],
        (Latest::Checking, None) => vec![Offer::main(INSTALL, Intent::Install).waiting()],
        (Latest::Found(version, _), Some(installed)) if installed.current_with(*version) => {
            vec![Offer::reinstall(), Offer::uninstall(true)]
        }
        (Latest::Found(..), Some(_)) => vec![Offer::uninstall(false), Offer::main(UPDATE, Intent::Install)],
        (Latest::Found(..), None) => vec![Offer::main(INSTALL, Intent::Install)],
    }
}

#[cfg(test)]
mod tests {
    use notchbase_release::{Release, Version};

    use super::*;

    fn found(version: &str) -> Latest {
        let release = Release { version: version.into(), url: String::new(), sha256: String::new(), size: None };
        Latest::Found(Version::parse(version).unwrap(), release)
    }

    fn having(version: &str) -> Option<Installed> {
        Some(Installed { version: Version::parse(version) })
    }

    fn titles(offers: &[Offer]) -> Vec<(&str, bool)> {
        offers.iter().map(|offer| (offer.title, offer.primary)).collect()
    }

    #[test]
    fn current_copy_offers_uninstall() {
        let offers = footer_choices(&found("0.3.1"), having("0.3.1"));
        assert_eq!(titles(&offers), [("Reinstall", false), ("Uninstall", true)]);
        assert_eq!(offers[1].intent, Intent::AskRemove);
    }

    #[test]
    fn older_copy_offers_update() {
        let offers = footer_choices(&found("0.3.1"), having("0.3.0"));
        assert_eq!(titles(&offers), [("Uninstall", false), ("Update", true)]);
        let unknown = footer_choices(&found("0.3.1"), Some(Installed { version: None }));
        assert_eq!(titles(&unknown), [("Uninstall", false), ("Update", true)]);
    }

    #[test]
    fn fresh_computer_offers_install() {
        assert_eq!(titles(&footer_choices(&found("0.3.1"), None)), [("Install", true)]);
        assert_eq!(titles(&footer_choices(&Latest::Checking, None)), [("Install", true)]);
    }

    #[test]
    fn main_button_waits_for_lookup() {
        let fresh = footer_choices(&Latest::Checking, None);
        assert!(!fresh[0].enabled);
        let installed = footer_choices(&Latest::Checking, having("0.3.0"));
        assert_eq!(installed.iter().map(|offer| offer.enabled).collect::<Vec<_>>(), [true, false]);
        assert!(footer_choices(&found("0.3.1"), None).iter().all(|offer| offer.enabled));
    }

    #[test]
    fn failed_lookup_offers_retry() {
        let failed = Latest::Failed("offline".into());
        let fresh = footer_choices(&failed, None);
        assert_eq!((fresh[0].title, fresh[0].intent), (TRY_AGAIN, Intent::Retry));
        let installed = footer_choices(&failed, having("0.3.0"));
        assert_eq!(titles(&installed), [("Uninstall", false), (TRY_AGAIN, true)]);
    }

    #[test]
    fn offers_become_keyed_buttons() {
        let offer = Offer::main(INSTALL, Intent::Install).waiting();
        let choice = offer.choice();
        assert_eq!((choice.key, choice.title, choice.primary, choice.enabled), (MAIN, INSTALL, true, false));
        let keys: Vec<&str> = footer_choices(&found("0.3.1"), having("0.3.1")).iter().map(|offer| offer.key).collect();
        assert_eq!(keys, ["reinstall", "uninstall"]);
    }
}
