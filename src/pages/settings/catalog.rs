//! The Tools and Indicators views: the tools folder, config.json and the installed tools.

use egui::{Align2, Id, Rect, pos2, vec2};

use super::general::has_settings;
use super::rows::{
    BACK_H, INSET, ROW_H, Row, SECTION_H, back, card, paint_lines, pill, pill_width, round_button,
    round_button_left_of, section,
};
use crate::bridge::{Action, PanelData, ToolInfo};
use crate::pages::PageCtx;
use crate::ui::color::{self, secondary_label, tertiary_label};
use crate::ui::glyph::Glyph;
use crate::ui::metrics::GAP;
use crate::ui::switch::switch;
use crate::ui::{controls, font};

const GEAR_SECS: f32 = 0.2;
const SWITCH: egui::Vec2 = vec2(40.0, 24.0);
const HINT_H: f32 = 30.0;
const COG_GAP: f32 = 4.0;
const OPEN: &str = "Open";
const RELOAD: &str = "Reload";
const RELOAD_DETAIL: &str = "Pick up tools you've added or rebuilt";
const BUILT_IN: &str = "Built in";
const UNKNOWN: &str = "Unknown location";
#[cfg(target_os = "windows")]
const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .dll inside";
#[cfg(target_os = "macos")]
const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .dylib inside";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const HINT: &str = "One folder per tool in the tools folder, with its .wasm or .so inside";

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

    fn listed(self, catalog: &[ToolInfo]) -> impl Iterator<Item = &ToolInfo> {
        catalog.iter().filter(move |info| Kind::of(info) == self)
    }
}

pub(super) fn height(catalog: &[ToolInfo], kind: Kind) -> f32 {
    let listed = kind.count(catalog).max(1) as f32 * ROW_H;
    match kind {
        Kind::Tools => BACK_H + GAP + 3.0 * ROW_H + HINT_H + SECTION_H + listed,
        Kind::Indicators => BACK_H + GAP + listed,
    }
}

pub(super) enum Outcome {
    Stay,
    Back,
    Settings(String),
}

pub(super) fn ui(ctx: &mut PageCtx<'_>, data: &PanelData, kind: Kind, actions: &mut Vec<Action>) -> Outcome {
    let live = ctx.alpha > super::rows::INTERACTIVE_FROM;
    let mut top = ctx.rect.top();
    let back = back(ctx, top, "Settings", live);
    top += BACK_H + GAP;

    let count = kind.count(data.catalog);
    if kind == Kind::Tools {
        let rows = card(ctx, top, 3);
        locate(ctx, &rows[0], "Tools folder", data.locations.tools.as_deref(), live, actions);
        locate(ctx, &rows[1], "config.json", data.locations.config.as_deref(), live, actions);
        reload(ctx, &rows[2], live, actions);
        top += 3.0 * ROW_H;
        let hint =
            Rect::from_min_size(pos2(ctx.rect.left() + INSET, top), vec2(ctx.rect.width() - 2.0 * INSET, HINT_H));
        let shown = controls::truncate(ctx.canvas, HINT, font::caption(), hint.width());
        ctx.canvas.text(hint.left_center(), Align2::LEFT_CENTER, shown, font::caption(), tertiary_label());
        top += HINT_H;
        section(ctx, top, &format!("Installed ({count})"));
        top += SECTION_H;
    }
    let rows = card(ctx, top, count.max(1));
    if count == 0 {
        let title = if kind == Kind::Tools { NO_TOOLS } else { NO_INDICATORS };
        let row = Row { rect: rows[0], title, detail: "" };
        paint_lines(ctx, &row, rows[0].right() - INSET, secondary_label(), false);
    }
    let mut opened = None;
    for (info, rect) in kind.listed(data.catalog).zip(rows) {
        let settings = data.tools.iter().any(|tool| tool.id == info.id && has_settings(tool));
        if installed(ctx, info, rect, settings, actions) {
            opened = Some(info.id.clone());
        }
    }
    match (back, opened) {
        (true, _) => Outcome::Back,
        (false, Some(id)) => Outcome::Settings(id),
        (false, None) => Outcome::Stay,
    }
}

fn reload(ctx: &mut PageCtx<'_>, rect: &Rect, live: bool, actions: &mut Vec<Action>) {
    let control_left = rect.right() - INSET - pill_width(ctx.canvas, RELOAD);
    let row = Row { rect: *rect, title: "Reload tools", detail: RELOAD_DETAIL };
    paint_lines(ctx, &row, control_left, secondary_label(), false);
    let id = Id::new("notchbase.tools.reload");
    if pill(ctx, rect.right() - INSET, rect.center().y, id, RELOAD, live).1 {
        actions.push(Action::ReloadTools);
    }
}

fn locate(ctx: &mut PageCtx<'_>, rect: &Rect, title: &str, path: Option<&str>, live: bool, actions: &mut Vec<Action>) {
    let id = Id::new(("notchbase.tools.open", title));
    let control_left = rect.right() - INSET - pill_width(ctx.canvas, OPEN);
    let row = Row { rect: *rect, title, detail: path.unwrap_or(UNKNOWN) };
    paint_lines(ctx, &row, control_left, secondary_label(), true);
    let Some(path) = path else { return };
    let (_, open) = pill(ctx, rect.right() - INSET, rect.center().y, id, OPEN, live);
    if open {
        actions.push(Action::OpenPath(path.to_owned()));
    }
}

fn installed(ctx: &mut PageCtx<'_>, info: &ToolInfo, rect: Rect, cog: bool, actions: &mut Vec<Action>) -> bool {
    let live = ctx.alpha > super::rows::INTERACTIVE_FROM;
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
    fn tools_view_fits_folder_rows() {
        let folders = BACK_H + GAP + 3.0 * ROW_H + HINT_H + SECTION_H + ROW_H;
        assert_eq!(height(&[], Kind::Tools), folders);
    }
}
