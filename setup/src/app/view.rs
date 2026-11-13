//! The parts every page shares: the header, the app tile and the page wording.

use std::path::Path;

use egui::{Align2, Color32, Id, Rect, Ui, Vec2, pos2, vec2};
use notchbase_paint::canvas::Canvas;
use notchbase_paint::color::{self, label, secondary_label, tertiary_label};
use notchbase_paint::controls;
use notchbase_paint::font;
use notchbase_paint::glass::{self, Glass};
use notchbase_paint::metrics::CARD_RADIUS;
use notchbase_release::Version;

use super::flow::Page;
use super::{FADE, SETTLED_PAGE, SHOWN_PAGE, Setup};
use crate::install::{self, Installed, Latest, OS};
use crate::pages::{self, Intent, Scene, Task, Welcome, Working};
use crate::{theme, widgets};

const TILE_ICON: f32 = 44.0;
const TILE_TEXT_GAP: f32 = 12.0;
const TILE_LINE: f32 = 15.0;
const CLOSE: f32 = 13.0;
const TITLE_DROP: f32 = 3.0;
const NAME: &str = "notchbase";
const FINDING: &str = "Finding the latest version\u{2026}";
const UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

impl Setup {
    pub(super) fn page_ui(
        &self,
        scene: &Scene<'_>,
        canvas: &mut Canvas,
        page: Page,
        latest: &Latest,
    ) -> Option<Intent> {
        match page {
            Page::Welcome => {
                let welcome = Welcome { options: &self.options, latest, installed: self.installed };
                pages::welcome(scene, canvas, &welcome)
            }
            Page::Confirm => pages::confirm(scene, canvas),
            Page::Working => {
                let working = Working {
                    task: self.task,
                    stage: &self.stage,
                    version: self.version,
                    size: self.size,
                    shown: self.shown,
                };
                pages::working(scene, canvas, &working);
                None
            }
            Page::Done => pages::done(scene, canvas, self.task),
            Page::Failed => pages::failed(scene, canvas, self.task, &self.problem),
        }
    }

    fn title(&self) -> &'static str {
        let latest = match self.lookup.latest() {
            Latest::Found(version, _) => Some(version),
            _ => None,
        };
        title(self.task, self.installed, latest)
    }

    pub(super) fn header(&self, ui: &Ui, rect: Rect, live: bool) -> bool {
        let mut canvas = Canvas::new(ui.painter());
        let y = rect.top() + theme::HEADER_H * 0.5 + TITLE_DROP;
        let left = pos2(rect.left() + theme::PAD + theme::INSET * 0.5, y);
        canvas.text(left, Align2::LEFT_CENTER, self.title(), font::headline(), label());
        let closable = ui.ctx().animate_bool_with_time(Id::new("setup.closable"), self.page != Page::Working, FADE);
        let center = pos2(rect.right() - theme::PAD - CLOSE, y);
        let id = Id::new("setup.close");
        let closed =
            closable > SHOWN_PAGE && widgets::close(ui, &mut canvas, center, id, live && closable > SETTLED_PAGE);
        canvas.paint(ui.painter(), rect.center(), 1.0, Vec2::ZERO, 1.0);
        closed
    }

    pub(super) fn tile(&self, ui: &Ui, rect: Rect, latest: &Latest) {
        let tile = Rect::from_min_size(
            pos2(rect.left() + theme::PAD, rect.top() + theme::HEADER_H),
            vec2(rect.width() - 2.0 * theme::PAD, theme::TILE_H),
        );
        let mut back = Canvas::new(ui.painter());
        glass::paint(&mut back, tile, CARD_RADIUS, Glass::TILE);
        back.paint(ui.painter(), rect.center(), 1.0, Vec2::ZERO, 1.0);
        let icon = Rect::from_center_size(
            pos2(tile.left() + theme::INSET + TILE_ICON * 0.5, tile.center().y),
            Vec2::splat(TILE_ICON),
        );
        ui.painter().image(self.icon.id(), icon, UV, Color32::WHITE);
        let mut text = Canvas::new(ui.painter());
        let left = icon.right() + TILE_TEXT_GAP;
        let room = tile.right() - theme::INSET - left;
        let (status, tone) = match latest {
            Latest::Checking => (FINDING.to_owned(), secondary_label()),
            Latest::Found(version, _) => (status(*version, self.installed), secondary_label()),
            Latest::Failed(problem) => (problem.clone(), color::RED),
        };
        let center = tile.center().y;
        text.text(pos2(left, center - TILE_LINE), Align2::LEFT_CENTER, NAME, font::headline(), label());
        let status = controls::truncate(&text, &status, font::caption(), room);
        text.text(pos2(left, center), Align2::LEFT_CENTER, status, font::caption(), tone);
        let place = controls::truncate_start(&text, &installs_to(&self.place), font::caption(), room);
        text.text(pos2(left, center + TILE_LINE), Align2::LEFT_CENTER, place, font::caption(), tertiary_label());
        text.paint(ui.painter(), rect.center(), 1.0, Vec2::ZERO, 1.0);
    }
}

fn title(task: Task, installed: Option<Installed>, latest: Option<Version>) -> &'static str {
    match (task, installed) {
        (Task::Remove, _) => "Uninstall notchbase",
        (Task::Install, Some(installed)) if latest.is_some_and(|latest| installed.current_with(latest)) => NAME,
        (Task::Install, Some(_)) => "Update notchbase",
        (Task::Install, None) => "Install notchbase",
    }
}

fn status(latest: Version, installed: Option<Installed>) -> String {
    match installed.and_then(|installed| installed.version) {
        Some(version) if version >= latest => format!("Up to date \u{b7} {version}"),
        Some(version) => format!("Version {latest} is out \u{b7} you have {version}"),
        None => format!("Version {latest} for {OS}"),
    }
}

fn installs_to(place: &str) -> String {
    format!("Installs to {place}")
}

pub(super) fn place() -> String {
    install::destination().map(|path| folder_of(&path)).unwrap_or_default()
}

fn folder_of(path: &Path) -> String {
    path.parent().map_or_else(|| path.to_path_buf(), Path::to_path_buf).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn having(text: &str) -> Option<Installed> {
        Some(Installed { version: Version::parse(text) })
    }

    #[test]
    fn title_says_what_will_happen() {
        let latest = Some(version("0.3.1"));
        assert_eq!(title(Task::Remove, having("0.3.1"), latest), "Uninstall notchbase");
        assert_eq!(title(Task::Install, None, latest), "Install notchbase");
        assert_eq!(title(Task::Install, having("0.3.0"), latest), "Update notchbase");
        assert_eq!(title(Task::Install, having("0.3.1"), latest), "notchbase");
        assert_eq!(title(Task::Install, having("0.4.0"), latest), "notchbase");
    }

    #[test]
    fn installed_copy_reads_as_update() {
        assert_eq!(title(Task::Install, having("0.3.1"), None), "Update notchbase");
        assert_eq!(title(Task::Install, Some(Installed { version: None }), Some(version("0.1.0"))), "Update notchbase");
    }

    #[test]
    fn status_compares_versions() {
        let latest = version("0.3.1");
        assert_eq!(status(latest, having("0.3.1")), "Up to date \u{b7} 0.3.1");
        assert_eq!(status(latest, having("0.4.0")), "Up to date \u{b7} 0.4.0");
        assert_eq!(status(latest, having("0.3.0")), "Version 0.3.1 is out \u{b7} you have 0.3.0");
        assert_eq!(status(latest, None), format!("Version 0.3.1 for {OS}"));
        assert_eq!(status(latest, Some(Installed { version: None })), format!("Version 0.3.1 for {OS}"));
    }

    #[test]
    fn tile_names_the_install_folder() {
        assert_eq!(folder_of(Path::new("/Applications/notchbase.app")), "/Applications");
        assert_eq!(folder_of(Path::new("notchbase.exe")), "");
        assert_eq!(folder_of(Path::new("/")), "/");
        assert_eq!(installs_to("/Applications"), "Installs to /Applications");
    }
}
