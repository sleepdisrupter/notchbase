//! The Tools and Indicators views: the tools folder, config.json and the installed tools.

use egui::{Id, Rect, pos2, vec2};

use super::rows::{INSET, Row, paint_lines, pill, pill_width, round_button, round_button_left_of};
use super::search::Said;
use crate::bridge::{Action, Locations, ToolInfo};
use crate::pages::PageCtx;
use crate::ui::color::{self, secondary_label};
use crate::ui::glyph::Glyph;
use crate::ui::switch::switch;

const GEAR_SECS: f32 = 0.2;
const SWITCH: egui::Vec2 = vec2(40.0, 24.0);
const COG_GAP: f32 = 4.0;
const OPEN: &str = "Open";
const RELOAD: &str = "Reload";
const FOLDER: &str = "Tools folder";
const CONFIG: &str = "config.json";
const RELOAD_DETAIL: &str = "Pick up tools you've added or rebuilt";
const BUILT_IN: &str = "Built in";
const UNKNOWN: &str = "Unknown location";
#[cfg(target_os = "windows")]
pub(super) const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .dll inside";
#[cfg(target_os = "macos")]
pub(super) const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .dylib inside";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(super) const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .so inside";

const NO_TOOLS: &str = "No tools found";
const NO_INDICATORS: &str = "Nothing here yet";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Tools,
    Indicators,
}

impl Kind {
    pub(super) fn of(info: &ToolInfo) -> Self {
        if info.background { Kind::Indicators } else { Kind::Tools }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Kind::Tools => "Tools",
            Kind::Indicators => "Indicators",
        }
    }

    pub(super) fn count(self, catalog: &[ToolInfo]) -> usize {
        self.listed(catalog).count()
    }

    pub(super) fn listed(self, catalog: &[ToolInfo]) -> impl Iterator<Item = &ToolInfo> {
        catalog.iter().filter(move |info| Kind::of(info) == self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolsRow {
    Folder,
    Config,
    Reload,
}

pub(super) const ROWS: &[ToolsRow] = &[ToolsRow::Folder, ToolsRow::Config, ToolsRow::Reload];

impl ToolsRow {
    pub(super) fn said(self, locations: &Locations) -> Said {
        let (title, detail, words) = match self {
            ToolsRow::Folder => (FOLDER, locations.tools.as_deref().unwrap_or(UNKNOWN), "plugins open finder explorer"),
            ToolsRow::Config => (CONFIG, locations.config.as_deref().unwrap_or(UNKNOWN), "settings file json edit"),
            ToolsRow::Reload => ("Reload tools", RELOAD_DETAIL, "plugins refresh rebuild restart"),
        };
        Said { title: title.to_owned(), detail: detail.to_owned(), words }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: Rect, locations: &Locations, live: bool) -> Option<Action> {
        match self {
            ToolsRow::Folder => locate(ctx, &rect, FOLDER, locations.tools.as_deref(), live),
            ToolsRow::Config => locate(ctx, &rect, CONFIG, locations.config.as_deref(), live),
            ToolsRow::Reload => reload(ctx, &rect, live),
        }
    }
}

/// An installed tool's row, as the search reads it.
pub(super) fn said(info: &ToolInfo) -> Said {
    let detail = info.problem.as_deref().or(info.location.as_deref()).unwrap_or(BUILT_IN);
    Said { title: info.name.clone(), detail: detail.to_owned(), words: "tool plugin indicator enable disable" }
}

fn reload(ctx: &mut PageCtx<'_>, rect: &Rect, live: bool) -> Option<Action> {
    let control_left = rect.right() - INSET - pill_width(ctx.canvas, RELOAD);
    let row = Row { rect: *rect, title: "Reload tools", detail: RELOAD_DETAIL };
    paint_lines(ctx, &row, control_left, secondary_label(), false);
    let id = Id::new("notchbase.tools.reload");
    pill(ctx, rect.right() - INSET, rect.center().y, id, RELOAD, live).1.then_some(Action::ReloadTools)
}

fn locate(ctx: &mut PageCtx<'_>, rect: &Rect, title: &str, path: Option<&str>, live: bool) -> Option<Action> {
    let id = Id::new(("notchbase.tools.open", title));
    let control_left = rect.right() - INSET - pill_width(ctx.canvas, OPEN);
    let row = Row { rect: *rect, title, detail: path.unwrap_or(UNKNOWN) };
    paint_lines(ctx, &row, control_left, secondary_label(), true);
    let path = path?;
    let (_, open) = pill(ctx, rect.right() - INSET, rect.center().y, id, OPEN, live);
    open.then(|| Action::OpenPath(path.to_owned()))
}

/// The one row a card shows when it lists nothing.
pub(super) fn nothing(ctx: &mut PageCtx<'_>, rect: Rect, kind: Kind) {
    let title = if kind == Kind::Tools { NO_TOOLS } else { NO_INDICATORS };
    let row = Row { rect, title, detail: "" };
    paint_lines(ctx, &row, rect.right() - INSET, secondary_label(), false);
}

/// An installed tool: its switch, and a gear to its own settings when it has any. True when the gear is pressed.
pub(super) fn installed(
    ctx: &mut PageCtx<'_>,
    info: &ToolInfo,
    rect: Rect,
    (cog, live): (bool, bool),
    actions: &mut Vec<Action>,
) -> bool {
    let title = if info.version.is_empty() { info.name.clone() } else { format!("{}  {}", info.name, info.version) };
    let unloadable = info.problem.is_some() && !info.enabled;
    let at = Rect::from_center_size(pos2(rect.right() - INSET - SWITCH.x * 0.5, rect.center().y), SWITCH);
    let mut control_left = if unloadable { rect.right() - INSET } else { at.left() };
    let mut opened = false;
    let id = Id::new(("notchbase.tools.settings", info.id.as_str()));
    let geared = ctx.ui.ctx().animate_bool_with_time(id.with("geared"), cog && info.enabled, GEAR_SECS);
    if geared > 0.001 {
        let gear = round_button_left_of(control_left - COG_GAP, rect.center().y);
        let mark = ctx.canvas.mark();
        opened = round_button(ctx, gear, id, Glyph::Gear, live && cog && info.enabled);
        for shape in ctx.canvas.since(mark) {
            crate::ui::canvas::fade(shape, geared);
        }
        control_left = crate::ui::spring::lerp(control_left, gear.left(), geared);
    }
    let (detail, tone, path) = match &info.problem {
        Some(problem) => (problem.as_str(), color::ORANGE, false),
        None => (info.location.as_deref().unwrap_or(BUILT_IN), secondary_label(), info.location.is_some()),
    };
    paint_lines(ctx, &Row { rect, title: &title, detail }, control_left, tone, path);
    let id = Id::new(("notchbase.tools.enabled", info.id.as_str()));
    if !unloadable && switch(ctx.ui, ctx.canvas, at, id, info.enabled, live) {
        actions.push(Action::SetToolEnabled(info.id.clone(), !info.enabled));
    }
    opened
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_tool_says_where_it_is() {
        let mut info = ToolInfo {
            id: "timer".into(),
            name: "Timer".into(),
            version: "1.0".into(),
            location: None,
            enabled: true,
            problem: None,
            background: false,
        };
        assert_eq!(said(&info).detail, BUILT_IN);
        info.location = Some("/tools/timer".into());
        assert_eq!(said(&info).detail, "/tools/timer");
        info.problem = Some("Couldn't load".into());
        assert_eq!((said(&info).title, said(&info).detail), ("Timer".to_owned(), "Couldn't load".to_owned()));
        assert_eq!(Kind::of(&info), Kind::Tools);
    }
}
