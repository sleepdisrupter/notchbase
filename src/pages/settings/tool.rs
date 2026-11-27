//! A tool's own settings view, built from the settings the tool declares.

use egui::{Rect, vec2};

use super::editing::Editing;
use super::rows::{BACK_H, INSET, ROW_H, Row, SECTION_H, back, card, card_of, paint_lines, section};
use super::setting::{shown, tool_setting};
use crate::bridge::{Action, ToolView};
use crate::pages::PageCtx;
use crate::ui::color::secondary_label;
use crate::ui::metrics::GAP;
use crate::ui::rows::{self, Rows};

const NOTHING: &str = "Nothing to set up";

pub(super) enum Outcome {
    Stay,
    Back,
}

#[derive(Default)]
pub(super) struct Shown {
    tool: Option<String>,
    rows: Rows<String, ()>,
}

impl Shown {
    pub(super) fn height(&self, tool: Option<&ToolView>) -> f32 {
        let laid = self.tool.is_some() && self.tool.as_deref() == tool.map(|tool| tool.id.as_str());
        let rows = match laid {
            true => self.rows.iter().map(|(_, _, shown)| shown).sum::<f32>(),
            false => tool.map_or(0, |tool| shown(tool).count()) as f32,
        };
        BACK_H + GAP + SECTION_H + rows.max(1.0) * ROW_H
    }

    pub(super) fn is_animating(&self) -> bool {
        self.rows.is_animating()
    }

    fn follow(&mut self, tool: &ToolView, animate: bool, dt: f32) -> Vec<(String, f32)> {
        let same = self.tool.as_deref() == Some(tool.id.as_str());
        if !same {
            self.tool = Some(tool.id.clone());
            self.rows = Rows::default();
        }
        self.rows.follow(shown(tool).map(|setting| (setting.spec.key.clone(), ())), animate && same);
        self.rows.step(dt);
        self.rows.iter().map(|(key, _, shown)| (key.clone(), shown)).collect()
    }
}

pub(super) fn ui(
    ctx: &mut PageCtx<'_>,
    tool: Option<&ToolView>,
    from: &str,
    (editing, laid_out): (&mut Editing, &mut Shown),
    actions: &mut Vec<Action>,
) -> Outcome {
    let live = ctx.alpha > super::rows::INTERACTIVE_FROM;
    let mut top = ctx.rect.top();
    let back = back(ctx, top, from, live);
    let Some(tool) = tool else { return Outcome::Back };
    top += BACK_H + GAP;
    section(ctx, top, &tool.name);
    top += SECTION_H;
    let laid = laid_out.follow(tool, live, ctx.dt);
    if laid.is_empty() {
        let rects = card(ctx, top, 1);
        let row = Row { rect: rects[0], title: NOTHING, detail: "" };
        paint_lines(ctx, &row, rects[0].right() - INSET, secondary_label(), false);
    }
    let heights: Vec<f32> = laid.iter().map(|(_, shown)| ROW_H * shown).collect();
    let rects = if laid.is_empty() { Vec::new() } else { card_of(ctx, top, &heights) };
    for ((key, shown), rect) in laid.iter().zip(rects) {
        let Some(setting) = tool.settings.iter().find(|setting| &setting.spec.key == key) else { continue };
        let whole = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H));
        let mark = ctx.canvas.mark();
        let action = tool_setting(ctx, tool, setting, whole, live && *shown > 0.99, editing, actions);
        rows::reveal(ctx.canvas, mark, rect, *shown);
        actions.extend(action);
    }
    if back { Outcome::Back } else { Outcome::Stay }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{SettingSpec, Value};

    use super::*;
    use crate::bridge::{ToolSetting, ToolSlot};

    fn calendar(source: &str) -> ToolView {
        let specs = [
            SettingSpec::choice("source", "Source", [("apple", "Apple"), ("ical", "iCal")], "apple"),
            SettingSpec::text("link", "Link", "").secret().shown_when_is("source", "ical"),
        ];
        let mut settings: Vec<ToolSetting> =
            specs.into_iter().map(|spec| ToolSetting { value: spec.default.clone(), spec }).collect();
        settings[0].value = Value::String(source.into());
        ToolView {
            slot: ToolSlot(1),
            id: "calendar".into(),
            name: "Calendar".into(),
            tile: None,
            page: None,
            live: None,
            settings,
            failure: None,
        }
    }

    #[test]
    fn waiting_setting_grows_and_folds() {
        let mut laid = Shown::default();
        laid.follow(&calendar("apple"), true, 1.0 / 60.0);
        assert_eq!(
            laid.height(Some(&calendar("apple"))),
            BACK_H + GAP + SECTION_H + ROW_H,
            "first rows are simply there"
        );
        laid.follow(&calendar("ical"), true, 1.0 / 60.0);
        let growing = laid.height(Some(&calendar("ical")));
        assert!(
            growing > BACK_H + GAP + SECTION_H + ROW_H && growing < BACK_H + GAP + SECTION_H + 2.0 * ROW_H,
            "{growing}"
        );
        for _ in 0..60 {
            laid.follow(&calendar("ical"), true, 1.0 / 60.0);
        }
        assert_eq!(laid.height(Some(&calendar("ical"))), BACK_H + GAP + SECTION_H + 2.0 * ROW_H);
        laid.follow(&calendar("apple"), true, 1.0 / 60.0);
        assert!(laid.is_animating(), "and folds away again");
    }
}
