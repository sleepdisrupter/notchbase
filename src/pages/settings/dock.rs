//! The Dock view's rows: notchbase dock, the separate Dock replacement, switched on or off, and its settings in groups.
//! Every change goes to the dock's own `dock.json`, which the running dock takes within half a second.

use egui::{Id, Rect, pos2};

use super::rows::{CHOICE, INSET, Row, Toggle, paint_text, stepper, toggled};
use super::search::Said;
#[cfg(not(windows))]
use crate::bridge::DockMissionControl;
use crate::bridge::{Action, DockChange, DockSide, DockStackSort, DockStackView, DockView};
use crate::pages::PageCtx;
use crate::ui::segmented::segmented;

/// The glass's three looks, lightest first.
const GLASS: [f32; 3] = [0.3, 0.5, 0.72];
#[cfg(not(windows))]
mod words {
    pub(super) const PITCH: &str = "Liquid animations, live previews and its own Cmd+Tab";
    pub(super) const ON: &str = "Replaces the macOS Dock, and puts it back when off";
    pub(super) const SECOND_ROW: &str = "Permissions";
    pub(super) const ASKS: &str = "macOS asks for Accessibility and Screen Recording next";
    pub(super) const USES: &str = "Accessibility moves windows, Screen Recording draws previews";
    pub(super) const SUMMARY_NEW: &str = "Replace the macOS Dock";
    pub(super) const AVOID: &str = "Filled and zoomed windows end above the dock";
    pub(super) const SYSTEM_MINIMIZE: &str = "macOS";
    pub(super) const PULSE: &str = "Spotify or Music swells gently to the beat while it plays";
}
#[cfg(windows)]
mod words {
    pub(super) const PITCH: &str = "Liquid animations, live previews and a Downloads stack";
    pub(super) const ON: &str = "Replaces the taskbar, and puts it back when off";
    pub(super) const SECOND_ROW: &str = "Taskbar";
    pub(super) const ASKS: &str = "Hidden while the dock runs, back as it was when it's off";
    pub(super) const USES: &str = ASKS;
    pub(super) const SUMMARY_NEW: &str = "Replace the taskbar";
    pub(super) const AVOID: &str = "Maximized windows end above the dock";
    pub(super) const SYSTEM_MINIMIZE: &str = "Windows";
    pub(super) const PULSE: &str = "Spotify or another player swells gently to the beat while it plays";
}
use words::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DockRow {
    Running,
    Asks,
    Side,
    Size,
    Magnify,
    Glass,
    Names,
    Displays,
    Fullscreen,
    Avoid,
    Previews,
    #[cfg(not(windows))]
    Mission,
    Downloads,
    StackView,
    StackSort,
    OpenEffect,
    MinimizeEffect,
    MinimizeTime,
    Pulse,
    #[cfg(not(windows))]
    Switcher,
}

use DockRow::*;

/// The switch and the row that says what the system asks the dock for, then each group under its heading.
pub(super) const TOP: &[DockRow] = &[Running, Asks];
#[cfg(not(windows))]
pub(super) const GROUPS: &[(&str, &[DockRow])] = &[
    ("Appearance", &[Side, Size, Magnify, Glass, Names]),
    ("Behaviour", &[Displays, Fullscreen, Avoid, Previews, Mission]),
    ("Downloads", &[Downloads, StackView, StackSort]),
    ("Animations", &[OpenEffect, MinimizeEffect, MinimizeTime, Pulse]),
    ("Cmd+Tab", &[Switcher]),
];
/// Windows has no Mission Control or Cmd+Tab rows.
#[cfg(windows)]
pub(super) const GROUPS: &[(&str, &[DockRow])] = &[
    ("Appearance", &[Side, Size, Magnify, Glass, Names]),
    ("Behaviour", &[Displays, Fullscreen, Avoid, Previews]),
    ("Downloads", &[Downloads, StackView, StackSort]),
    ("Animations", &[OpenEffect, MinimizeEffect, MinimizeTime, Pulse]),
];

/// The tile's line.
pub(super) fn summary(dock: &DockView) -> String {
    match (dock.found, dock.running) {
        (_, true) => format!("On \u{b7} {} \u{b7} {} pt", side_name(dock.side).to_lowercase(), dock.tile_size.round()),
        (true, false) => "Off".to_owned(),
        (false, false) => SUMMARY_NEW.to_owned(),
    }
}

fn side_name(side: DockSide) -> &'static str {
    SIDES[DockSide::ALL.iter().position(|s| *s == side).unwrap_or(1)]
}

const SIDES: [&str; 3] = ["Left", "Bottom", "Right"];

impl DockRow {
    fn title(self) -> &'static str {
        match self {
            Running => "notchbase dock",
            Asks => SECOND_ROW,
            Side => "Position",
            Size => "Icon size",
            Magnify => "Magnification",
            Glass => "Glass",
            Names => "App names",
            Displays => "Displays",
            Fullscreen => "Hide in full screen",
            Avoid => "Keep windows clear",
            Previews => "Window previews",
            #[cfg(not(windows))]
            Mission => "Mission Control",
            Downloads => "Show Downloads",
            StackView => "Opens as",
            StackSort => "Sort by",
            OpenEffect => "Open effect",
            MinimizeEffect => "Minimize effect",
            MinimizeTime => "Minimize time",
            Pulse => "Music pulse",
            #[cfg(not(windows))]
            Switcher => "Dock's own switcher",
        }
    }

    fn detail(self, dock: &DockView) -> &'static str {
        match self {
            Running if dock.running => ON,
            Running => PITCH,
            Asks if dock.running => USES,
            Asks => ASKS,
            Side => "The edge of the screen it stands on",
            Size => "How big the icons are at rest",
            Magnify => "Icons swell gently under the pointer",
            Glass => "How dark it is",
            Names => "A name over the icon under the pointer",
            Displays => "The main screen only, or a dock on each",
            Fullscreen => "Push the pointer on its edge to bring it up",
            Avoid => AVOID,
            Previews => "Rest on a running app to see its windows",
            #[cfg(not(windows))]
            Mission => "Stay over it, or fade away",
            Downloads => "A stack of your newest files, beside the Trash",
            StackView => "A fan for a few files, a grid for many",
            StackSort => "Which file sits on top of the pile",
            OpenEffect => "The dock swells as an app opens",
            MinimizeEffect => "How windows go down into the dock",
            MinimizeTime => "Shorter is snappier",
            Pulse => PULSE,
            #[cfg(not(windows))]
            Switcher => "Shows on the pointer's screen, only its windows",
        }
    }

    fn words(self) -> &'static str {
        match self {
            Running => "dock taskbar replace on off",
            Asks => "accessibility screen recording taskbar permissions",
            Side => "left right bottom vertical side edge",
            Size => "icons small big tile",
            Magnify => "zoom magnify",
            Glass => "dark light smoked transparency blur",
            Names => "labels titles tooltip",
            Displays => "screens monitors external all",
            Fullscreen => "hide auto",
            Avoid => "maximize fill zoom windows",
            Previews => "thumbnails windows hover",
            #[cfg(not(windows))]
            Mission => "expose spaces",
            Downloads => "stack files trash",
            StackView => "stack fan grid",
            StackSort => "stack order newest name",
            OpenEffect => "launch bounce animation",
            MinimizeEffect => "genie liquid animation",
            MinimizeTime => "speed animation duration",
            Pulse => "music beat spotify",
            #[cfg(not(windows))]
            Switcher => "cmd tab app switcher alt",
        }
    }

    pub(super) fn said(self, dock: &DockView) -> Said {
        Said { title: self.title().to_owned(), detail: self.detail(dock).to_owned(), words: self.words() }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: Rect, dock: &DockView, live: bool) -> Option<Action> {
        // Nothing to change until the dock has made its file, with the apps it took from the system's dock.
        let can = live && dock.found;
        let (title, detail) = (self.title(), self.detail(dock));
        let row = (rect, title, detail);
        let change = match self {
            Running => {
                let toggle = Toggle { id: id("running"), on: dock.running, enabled: live };
                toggled(ctx, &Row { rect, title, detail }, &toggle).then_some(DockChange::Running(!dock.running))
            }
            Asks => {
                paint_text(ctx, &Row { rect, title, detail }, rect.right());
                None
            }
            Side => {
                let current = DockSide::ALL.iter().position(|s| *s == dock.side).unwrap_or(1);
                choice(ctx, row, &SIDES, current, can).map(|i| DockChange::Side(DockSide::ALL[i]))
            }
            Size => {
                let value = [f64::from(dock.tile_size), 16.0, 96.0, 4.0];
                let row = Row { rect, title, detail };
                stepper(ctx, &row, id("size"), value, "pt", can).map(|next| DockChange::TileSize(next as f32))
            }
            Magnify => {
                flag(ctx, row, dock.magnification, can).then_some(DockChange::Magnification(!dock.magnification))
            }
            Glass => {
                let near = |a: &(usize, &f32), b: &(usize, &f32)| {
                    (a.1 - dock.glass).abs().total_cmp(&(b.1 - dock.glass).abs())
                };
                let shade = GLASS.iter().enumerate().min_by(near).map_or(1, |(i, _)| i);
                choice(ctx, row, &["Light", "Smoked", "Dark"], shade, can).map(|i| DockChange::Glass(GLASS[i]))
            }
            Names => flag(ctx, row, dock.show_names, can).then_some(DockChange::ShowNames(!dock.show_names)),
            Displays => choice(ctx, row, &["Main", "All"], usize::from(dock.all_displays), can)
                .map(|i| DockChange::AllDisplays(i == 1)),
            Fullscreen => flag(ctx, row, dock.hide_in_fullscreen, can)
                .then_some(DockChange::HideInFullscreen(!dock.hide_in_fullscreen)),
            Avoid => flag(ctx, row, dock.avoid_windows, can).then_some(DockChange::AvoidWindows(!dock.avoid_windows)),
            Previews => {
                flag(ctx, row, dock.window_previews, can).then_some(DockChange::WindowPreviews(!dock.window_previews))
            }
            #[cfg(not(windows))]
            Mission => {
                let current = usize::from(dock.mission_control == DockMissionControl::Fade);
                choice(ctx, row, &["Stay", "Fade"], current, can).map(|i| {
                    DockChange::MissionControl(if i == 1 {
                        DockMissionControl::Fade
                    } else {
                        DockMissionControl::Cover
                    })
                })
            }
            Downloads => {
                flag(ctx, row, dock.show_downloads, can).then_some(DockChange::ShowDownloads(!dock.show_downloads))
            }
            StackView => {
                let current = DockStackView::ALL.iter().position(|v| *v == dock.stack_view).unwrap_or(0);
                choice(ctx, row, &["Auto", "Fan", "Grid"], current, can)
                    .map(|i| DockChange::StackView(DockStackView::ALL[i]))
            }
            StackSort => {
                let current = DockStackSort::ALL.iter().position(|s| *s == dock.stack_sort).unwrap_or(0);
                choice(ctx, row, &["Added", "Changed", "Name"], current, can)
                    .map(|i| DockChange::StackSort(DockStackSort::ALL[i]))
            }
            OpenEffect => flag(ctx, row, dock.launch_pop, can).then_some(DockChange::LaunchPop(!dock.launch_pop)),
            MinimizeEffect => {
                let current = usize::from(!dock.minimize_liquid);
                choice(ctx, row, &["Liquid", SYSTEM_MINIMIZE], current, can).map(|i| DockChange::MinimizeLiquid(i == 0))
            }
            MinimizeTime => {
                let seconds = (f64::from(dock.minimize_duration) * 100.0).round() / 100.0;
                let row = Row { rect, title, detail };
                stepper(ctx, &row, id("speed"), [seconds, 0.25, 0.8, 0.05], "s", can && dock.minimize_liquid)
                    .map(|next| DockChange::MinimizeDuration(next as f32))
            }
            Pulse => flag(ctx, row, dock.music_pulse, can).then_some(DockChange::MusicPulse(!dock.music_pulse)),
            #[cfg(not(windows))]
            Switcher => flag(ctx, row, dock.replace_switcher, can)
                .then_some(DockChange::ReplaceSwitcher(!dock.replace_switcher)),
        };
        change.map(Action::Dock)
    }
}

fn id(name: &str) -> Id {
    Id::new("notchbase.settings.dock").with(name)
}

fn flag(ctx: &mut PageCtx<'_>, (rect, title, detail): (Rect, &str, &str), on: bool, enabled: bool) -> bool {
    toggled(ctx, &Row { rect, title, detail }, &Toggle { id: id(title), on, enabled })
}

fn choice(
    ctx: &mut PageCtx<'_>,
    (rect, title, detail): (Rect, &str, &str),
    titles: &[&str],
    selected: usize,
    live: bool,
) -> Option<usize> {
    let width = CHOICE.x.min(titles.len() as f32 * 64.0);
    let control =
        Rect::from_center_size(pos2(rect.right() - INSET - width * 0.5, rect.center().y), egui::vec2(width, CHOICE.y));
    paint_text(ctx, &Row { rect, title, detail }, control.left());
    segmented(ctx.ui, ctx.canvas, control, id(title), titles, selected, live).filter(|i| *i != selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_says_whether_it_is_on() {
        let on = DockView { found: true, running: true, side: DockSide::Left, tile_size: 48.0, ..DockView::DEFAULT };
        assert_eq!(summary(&on), "On \u{b7} left \u{b7} 48 pt");
        assert_eq!(summary(&DockView { running: false, ..on.clone() }), "Off");
        assert_eq!(summary(&DockView::DEFAULT), SUMMARY_NEW);
    }

    #[test]
    fn every_row_has_words() {
        let rows = TOP.iter().chain(GROUPS.iter().flat_map(|(_, rows)| rows.iter()));
        for row in rows {
            let said = row.said(&DockView::DEFAULT);
            assert!(!said.title.is_empty() && !said.detail.is_empty() && !said.words.is_empty(), "{row:?}");
        }
    }
}
