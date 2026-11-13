//! The Dock view: notchbase dock, the separate Dock replacement, switched on or off, and its settings in five groups.
//! Every change goes to the dock's own `dock.json`, which the running dock takes within half a second.

use egui::{Id, Rect, pos2};

use super::rows::{
    BACK_H, CHOICE, INSET, ROW_H, Row, SECTION_H, Toggle, back, card, paint_text, section, stepper, toggled,
};
#[cfg(not(windows))]
use crate::bridge::DockMissionControl;
use crate::bridge::{Action, DockChange, DockStackSort, DockStackView, DockView};
use crate::pages::PageCtx;
use crate::ui::metrics::GAP;
use crate::ui::segmented::segmented;

/// Rows in each group under the switch: Appearance, Behaviour, Downloads, Animations, Cmd+Tab.
#[cfg(not(windows))]
const GROUPS: &[usize] = &[4, 5, 3, 4, 1];
/// Windows has no Mission Control or Cmd+Tab rows.
#[cfg(windows)]
const GROUPS: &[usize] = &[4, 4, 3, 4];
/// The glass's three looks, lightest first.
const GLASS: [f32; 3] = [0.3, 0.5, 0.72];
/// The switch and the row that says what macOS asks the dock for.
const TOP_ROWS: usize = 2;
#[cfg(not(windows))]
mod words {
    pub(super) const PITCH: &str = "Liquid animations, live previews and its own Cmd+Tab";
    pub(super) const ON: &str = "Replaces the macOS Dock, and puts it back when off";
    pub(super) const SECOND_ROW: &str = "Permissions";
    pub(super) const ASKS: &str = "macOS asks for Accessibility and Screen Recording next";
    pub(super) const USES: &str = "Accessibility moves windows, Screen Recording draws previews";
    pub(super) const SUMMARY_ON: &str = "On \u{b7} replaces the macOS Dock";
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
    pub(super) const SUMMARY_ON: &str = "On \u{b7} replaces the taskbar";
    pub(super) const SUMMARY_NEW: &str = "Replace the taskbar";
    pub(super) const AVOID: &str = "Maximized windows end above the dock";
    pub(super) const SYSTEM_MINIMIZE: &str = "Windows";
    pub(super) const PULSE: &str = "Spotify or another player swells gently to the beat while it plays";
}
use words::*;

pub(super) enum Outcome {
    Stay,
    Back,
}

pub(super) fn height() -> f32 {
    let groups: f32 = GROUPS.iter().map(|rows| SECTION_H + *rows as f32 * ROW_H + GAP).sum();
    BACK_H + GAP + TOP_ROWS as f32 * ROW_H + GAP + groups
}

/// The General view's row about it.
pub(super) fn summary(dock: &DockView) -> &'static str {
    match (dock.found, dock.running) {
        (_, true) => SUMMARY_ON,
        (true, false) => "Off",
        (false, false) => SUMMARY_NEW,
    }
}

pub(super) fn ui(ctx: &mut PageCtx<'_>, dock: &DockView, actions: &mut Vec<Action>) -> Outcome {
    let live = ctx.alpha > super::rows::INTERACTIVE_FROM;
    let mut top = ctx.rect.top();
    let back = back(ctx, top, "Settings", live);
    top += BACK_H + GAP;
    let mut change = |c: DockChange| actions.push(Action::Dock(c));

    // Off, it says what it is and what macOS will ask for before the switch starts anything.
    let rows = card(ctx, top, TOP_ROWS);
    let row = Row { rect: rows[0], title: "notchbase dock", detail: if dock.running { ON } else { PITCH } };
    if toggled(ctx, &row, &Toggle { id: id("running"), on: dock.running, enabled: live }) {
        change(DockChange::Running(!dock.running));
    }
    paint_text(
        ctx,
        &Row { rect: rows[1], title: SECOND_ROW, detail: if dock.running { USES } else { ASKS } },
        rows[1].right(),
    );
    top += TOP_ROWS as f32 * ROW_H + GAP;
    // Nothing to change until the dock has made its file, with the apps it took from the macOS Dock.
    let can = live && dock.found;

    let rows = group(ctx, &mut top, "Appearance", GROUPS[0]);
    let [size, magnify, glass, names] = rows[..] else { return outcome(back) };
    let row = Row { rect: size, title: "Icon size", detail: "How big the icons are at rest" };
    if let Some(next) = stepper(ctx, &row, id("size"), [f64::from(dock.tile_size), 32.0, 96.0, 4.0], "pt", can) {
        change(DockChange::TileSize(next as f32));
    }
    flag(ctx, (magnify, "Magnification", "Icons swell gently under the pointer"), dock.magnification, can)
        .then(|| change(DockChange::Magnification(!dock.magnification)));
    let shade = GLASS.iter().enumerate().min_by(|a, b| (a.1 - dock.glass).abs().total_cmp(&(b.1 - dock.glass).abs()));
    let shade = shade.map_or(1, |(i, _)| i);
    if let Some(i) = choice(ctx, (glass, "Glass", "How dark it is"), &["Light", "Smoked", "Dark"], shade, can) {
        change(DockChange::Glass(GLASS[i]));
    }
    flag(ctx, (names, "App names", "A name over the icon under the pointer"), dock.show_names, can)
        .then(|| change(DockChange::ShowNames(!dock.show_names)));

    let rows = group(ctx, &mut top, "Behaviour", GROUPS[1]);
    #[cfg(not(windows))]
    let [displays, fullscreen, avoid, previews, mission] = rows[..] else { return outcome(back) };
    #[cfg(windows)]
    let [displays, fullscreen, avoid, previews] = rows[..] else { return outcome(back) };
    let row = (displays, "Displays", "The main screen only, or a dock on each");
    if let Some(i) = choice(ctx, row, &["Main", "All"], usize::from(dock.all_displays), can) {
        change(DockChange::AllDisplays(i == 1));
    }
    let detail = "Push the pointer on the bottom edge to bring it up";
    flag(ctx, (fullscreen, "Hide in full screen", detail), dock.hide_in_fullscreen, can)
        .then(|| change(DockChange::HideInFullscreen(!dock.hide_in_fullscreen)));
    flag(ctx, (avoid, "Keep windows clear", AVOID), dock.avoid_windows, can)
        .then(|| change(DockChange::AvoidWindows(!dock.avoid_windows)));
    let detail = "Rest on a running app to see its windows";
    flag(ctx, (previews, "Window previews", detail), dock.window_previews, can)
        .then(|| change(DockChange::WindowPreviews(!dock.window_previews)));
    #[cfg(not(windows))]
    {
        let current = usize::from(dock.mission_control == DockMissionControl::Fade);
        let row = (mission, "Mission Control", "Stay over it, or fade away");
        if let Some(i) = choice(ctx, row, &["Stay", "Fade"], current, can) {
            let how = if i == 1 { DockMissionControl::Fade } else { DockMissionControl::Cover };
            change(DockChange::MissionControl(how));
        }
    }

    let rows = group(ctx, &mut top, "Downloads", GROUPS[2]);
    let [shown, view, sort] = rows[..] else { return outcome(back) };
    let detail = "A stack of your newest files, beside the Trash";
    flag(ctx, (shown, "Show Downloads", detail), dock.show_downloads, can)
        .then(|| change(DockChange::ShowDownloads(!dock.show_downloads)));
    let current = DockStackView::ALL.iter().position(|v| *v == dock.stack_view).unwrap_or(0);
    let row = (view, "Opens as", "A fan for a few files, a grid for many");
    if let Some(i) = choice(ctx, row, &["Auto", "Fan", "Grid"], current, can) {
        change(DockChange::StackView(DockStackView::ALL[i]));
    }
    let current = DockStackSort::ALL.iter().position(|s| *s == dock.stack_sort).unwrap_or(0);
    let row = (sort, "Sort by", "Which file sits on top of the pile");
    if let Some(i) = choice(ctx, row, &["Added", "Changed", "Name"], current, can) {
        change(DockChange::StackSort(DockStackSort::ALL[i]));
    }

    let rows = group(ctx, &mut top, "Animations", GROUPS[3]);
    let [open, minimize, speed, pulse] = rows[..] else { return outcome(back) };
    flag(ctx, (open, "Open effect", "The dock swells as an app opens"), dock.launch_pop, can)
        .then(|| change(DockChange::LaunchPop(!dock.launch_pop)));
    let current = usize::from(!dock.minimize_liquid);
    let row = (minimize, "Minimize effect", "How windows go down into the dock");
    if let Some(i) = choice(ctx, row, &["Liquid", SYSTEM_MINIMIZE], current, can) {
        change(DockChange::MinimizeLiquid(i == 0));
    }
    let row = Row { rect: speed, title: "Minimize time", detail: "Shorter is snappier" };
    let seconds = (f64::from(dock.minimize_duration) * 100.0).round() / 100.0;
    if let Some(next) = stepper(ctx, &row, id("speed"), [seconds, 0.25, 0.8, 0.05], "s", can && dock.minimize_liquid) {
        change(DockChange::MinimizeDuration(next as f32));
    }
    flag(ctx, (pulse, "Music pulse", PULSE), dock.music_pulse, can)
        .then(|| change(DockChange::MusicPulse(!dock.music_pulse)));

    #[cfg(not(windows))]
    {
        let rows = group(ctx, &mut top, "Cmd+Tab", GROUPS[4]);
        let detail = "Only apps with a window on screen, with live pictures";
        flag(ctx, (rows[0], "Dock's own switcher", detail), dock.replace_switcher, can)
            .then(|| change(DockChange::ReplaceSwitcher(!dock.replace_switcher)));
    }

    outcome(back)
}

fn outcome(back: bool) -> Outcome {
    if back { Outcome::Back } else { Outcome::Stay }
}

fn id(name: &str) -> Id {
    Id::new("notchbase.settings.dock").with(name)
}

fn group(ctx: &mut PageCtx<'_>, top: &mut f32, title: &str, rows: usize) -> Vec<Rect> {
    section(ctx, *top, title);
    *top += SECTION_H;
    let rects = card(ctx, *top, rows);
    *top += rows as f32 * ROW_H + GAP;
    rects
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
    fn view_is_back_a_switch_and_its_groups() {
        let rows: usize = GROUPS.iter().sum();
        let groups = GROUPS.len() as f32;
        let expected = BACK_H + GAP + 2.0 * ROW_H + GAP + groups * (SECTION_H + GAP) + rows as f32 * ROW_H;
        assert!((height() - expected).abs() < 0.01);
    }

    #[test]
    fn row_in_general_says_whether_it_is_on() {
        let on = DockView { found: true, running: true, ..DockView::DEFAULT };
        assert!(summary(&on).starts_with("On"));
        assert_eq!(summary(&DockView { running: false, ..on.clone() }), "Off");
        assert_eq!(summary(&DockView::DEFAULT), SUMMARY_NEW);
    }
}
