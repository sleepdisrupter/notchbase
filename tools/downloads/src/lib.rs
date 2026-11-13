//! Downloads: browser downloads in the notch. Finished ones land on the Shelf, and a zip offers to
//! unzip itself.

mod offer;
mod size;
mod unzip;

use std::path::PathBuf;
use std::time::Duration;

use notchbase_sdk::prelude::*;

use offer::Offer;
use size::bytes;

pub const ID: &str = "downloads";
const SHELVE: &str = "shelve";
const ANNOUNCE: &str = "announce";
const OFFER_UNZIP: &str = "offer_unzip";
const RING: f32 = 13.0;
const GLYPH: f32 = 14.0;
const BADGE: f32 = 26.0;
const GAP: f32 = 6.0;
const LONGEST: usize = 20;
const LANDED_FOR: Duration = Duration::from_millis(3200);
const DOWNLOADED: &str = "Downloaded";

pub struct Downloads {
    active: Vec<Download>,
    hidden: Vec<String>,
    quiet: bool,
    offer: Option<Offer>,
    prefs: Prefs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefs {
    shelve: bool,
    announce: bool,
    offer_unzip: bool,
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self {
            shelve: settings.bool(SHELVE),
            announce: settings.bool(ANNOUNCE),
            offer_unzip: settings.bool(OFFER_UNZIP),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Changed(DownloadChange),
    Hide(String),
    Focus(Focus),
    Configured(Settings),
    Unzip,
    Unzipped(Result<PathBuf, (PathBuf, String)>),
    Decline,
    Expire(PathBuf),
}

impl Tool for Downloads {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Downloads")
            .background()
            .description("Browser downloads in the notch")
            .version(env!("CARGO_PKG_VERSION"))
            .setting(
                SettingSpec::toggle(SHELVE, "Add to the Shelf", true)
                    .detail("Put a finished download on the Shelf, ready to drag"),
            )
            .setting(SettingSpec::toggle(ANNOUNCE, "Show when finished", true).detail("Show the file when it lands"))
            .setting(
                SettingSpec::toggle(OFFER_UNZIP, "Offer to unzip", true)
                    .detail("Ask to unzip a .zip when it lands, right next to it"),
            )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = Prefs::from(context.settings());
        (Self { active: Vec::new(), hidden: Vec::new(), quiet: false, offer: None, prefs }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Changed(DownloadChange::Progress { download }) => {
                match self.active.iter_mut().find(|known| known.id == download.id) {
                    Some(known) => *known = download,
                    None => self.active.push(download),
                }
                Command::none()
            }
            Msg::Changed(DownloadChange::Cancelled { download }) => {
                self.forget(&download.id);
                Command::none()
            }
            Msg::Changed(DownloadChange::Finished { download, path }) => {
                self.forget(&download.id);
                let shelved = if self.prefs.shelve { Command::shelve([&path]) } else { Command::none() };
                let announce = self.prefs.announce && !self.quiet;
                let announced = if announce { Command::notice(landed(&download)) } else { Command::none() };
                shelved.and(announced).and(self.offer_to_unzip(path))
            }
            Msg::Focus(focus) => {
                self.quiet = focus.active;
                Command::none()
            }
            Msg::Hide(id) => {
                self.hidden.push(id);
                Command::none()
            }
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                Command::none()
            }
            Msg::Unzip => self.unzip(),
            Msg::Unzipped(result) => self.unzipped(result),
            Msg::Decline => {
                self.offer = None;
                Command::none()
            }
            Msg::Expire(path) => {
                self.offer.take_if(|offer| offer.expires_with(&path));
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::downloads(Msg::Changed),
            Subscription::focus(Msg::Focus),
            Subscription::settings(Msg::Configured),
        ])
    }

    fn live(&self) -> Option<Live<Msg>> {
        let mut shown = self.active.iter().filter(|download| !self.hidden.contains(&download.id));
        let Some(download) = shown.next() else { return self.offer.as_ref().map(Offer::live) };
        let more = shown.count();
        let leading: Element<Msg> = match download.share() {
            Some(share) => ring(share).size(RING).tone(Tone::Blue).into(),
            None => icon(Icon::Download).size(GLYPH).tone(Tone::Blue).into(),
        };
        let amount = || text(amount(download, more)).value().secondary();
        let island = row![text(shortened(&download.name)).callout(), amount()].spacing(GAP);
        let hide = button("").icon(Icon::Xmark).on_press(Msg::Hide(download.id.clone()));
        Some(Live::new(leading, adaptive(amount(), island)).controls(hide))
    }
}

impl Downloads {
    fn forget(&mut self, id: &str) {
        self.active.retain(|download| download.id != id);
        self.hidden.retain(|hidden| hidden != id);
    }
}

fn amount(download: &Download, more: usize) -> String {
    let amount = bytes(download.received);
    if more == 0 { amount } else { format!("{amount} +{more}") }
}

fn landed(download: &Download) -> Notice {
    let detail = format!("{DOWNLOADED} \u{b7} {}", bytes(download.received));
    Notice::new(badge(Icon::Download, Tone::Blue, BADGE), text(shortened(&download.name)).callout())
        .below(text(detail).caption().secondary())
        .banner()
        .pop()
        .duration(LANDED_FOR)
}

fn shortened(name: &str) -> String {
    match name.char_indices().nth(LONGEST) {
        Some((at, _)) => format!("{}\u{2026}", name[..at].trim_end()),
        None => name.to_owned(),
    }
}

notchbase_sdk::export!(Downloads);

#[cfg(test)]
mod tests;
