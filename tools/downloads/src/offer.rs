//! The offer to unzip a finished zip. It goes away by itself after 30 seconds.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notchbase_sdk::prelude::*;

use super::unzip::{is_zip, unzip};
use super::{BADGE, Downloads, GAP, GLYPH, Msg, shortened};

const ASKS_FOR: Duration = Duration::from_secs(30);
const UNZIPPED_FOR: Duration = Duration::from_millis(3200);
const QUESTION: &str = "Unzip?";
const UNZIPPING: &str = "Unzipping\u{2026}";
const UNZIP: &str = "Unzip";
const UNZIPPED: &str = "Unzipped";

pub struct Offer {
    path: PathBuf,
    name: String,
    unzipping: bool,
}

impl Offer {
    pub fn expires_with(&self, path: &Path) -> bool {
        !self.unzipping && self.path == path
    }

    pub fn live(&self) -> Live<Msg> {
        let leading = icon(Icon::Folder).size(GLYPH).tone(Tone::Blue);
        let word = || text(if self.unzipping { UNZIPPING } else { QUESTION }).secondary();
        let island = row![text(shortened(&self.name)).callout(), word()].spacing(GAP);
        let live = Live::new(leading, adaptive(word(), island));
        if self.unzipping {
            return live;
        }
        let yes = button(UNZIP).on_press(Msg::Unzip);
        let no = button("").icon(Icon::Xmark).on_press(Msg::Decline);
        live.controls(row![yes, no].spacing(GAP))
    }
}

impl Downloads {
    pub(super) fn offer_to_unzip(&mut self, path: String) -> Command<Msg> {
        let path = PathBuf::from(path);
        if !self.prefs.offer_unzip || !is_zip(&path) || self.offer.as_ref().is_some_and(|offer| offer.unzipping) {
            return Command::none();
        }
        let name = path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        self.offer = Some(Offer { path: path.clone(), name, unzipping: false });
        Command::spawn(move |emitter| {
            std::thread::sleep(ASKS_FOR);
            emitter.send(Msg::Expire(path));
        })
    }

    pub(super) fn unzip(&mut self) -> Command<Msg> {
        let Some(offer) = self.offer.as_mut().filter(|offer| !offer.unzipping) else { return Command::none() };
        offer.unzipping = true;
        let archive = offer.path.clone();
        Command::perform(move || unzip(&archive).map_err(|err| (archive, err)), Msg::Unzipped)
    }

    pub(super) fn unzipped(&mut self, result: Result<PathBuf, (PathBuf, String)>) -> Command<Msg> {
        let name = self.offer.take().map(|offer| offer.name).unwrap_or_default();
        match result {
            Ok(folder) => {
                let shelved = if self.prefs.shelve { Command::shelve([&folder]) } else { Command::none() };
                shelved.and(Command::notice(unzipped(&folder)))
            }
            Err((archive, err)) => Command::log(format!("unzipping {} failed: {err}", archive.display()))
                .and(Command::notify(format!("Couldn't unzip {}", shortened(&name)))),
        }
    }
}

fn unzipped(folder: &Path) -> Notice {
    let name = folder.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Notice::new(badge(Icon::Folder, Tone::Blue, BADGE), text(shortened(&name)).callout())
        .below(text(UNZIPPED).caption().secondary())
        .banner()
        .pop()
        .duration(UNZIPPED_FOR)
}
