//! A tool's own settings as rows: which of them show, and each one's control.

use egui::{Id, Rect, pos2, vec2};
use notchbase_sdk::protocol::{SettingKind, Value};

use super::editing::Editing;
use super::rows::{
    CHOICE, INSET, Row, Toggle, paint_lines, paint_text, pill, round_button, round_button_left_of, stepper, toggled,
};
use super::search::Said;
use crate::bridge::{Action, ToolSetting, ToolView};
use crate::pages::PageCtx;
use crate::ui::color::{secondary_label, tertiary_label};
use crate::ui::field::{self, Edit, Field};
use crate::ui::glyph::Glyph;
use crate::ui::segmented::segmented;

const FIELD: egui::Vec2 = vec2(210.0, 28.0);
const SEGMENT_W: f32 = 58.0;
const CHOOSE: &str = "Choose\u{2026}";
const MAX_SEGMENTS: usize = 4;

/// A tool's setting, as the search reads it.
pub(super) fn said(setting: &ToolSetting) -> Said {
    Said { title: setting.spec.title.clone(), detail: setting.spec.detail.clone(), words: "" }
}

pub(super) fn has_settings(tool: &ToolView) -> bool {
    tool.settings.iter().any(|setting| setting.spec.visible && editable(&setting.spec.kind))
}

pub(super) fn shown(tool: &ToolView) -> impl Iterator<Item = &ToolSetting> {
    tool.settings
        .iter()
        .filter(|setting| setting.spec.visible && editable(&setting.spec.kind) && unlocked(tool, setting))
}

fn unlocked(tool: &ToolView, setting: &ToolSetting) -> bool {
    setting.spec.shows_with(|key| tool.settings.iter().find(|other| other.spec.key == key).map(|other| &other.value))
}

fn editable(kind: &SettingKind) -> bool {
    match kind {
        SettingKind::Toggle | SettingKind::Number { .. } | SettingKind::Folder { .. } | SettingKind::Text { .. } => {
            true
        }
        SettingKind::Choice { options } => !options.is_empty() && options.len() <= MAX_SEGMENTS,
        SettingKind::Data => false,
    }
}

pub(super) fn tool_setting(
    ctx: &mut PageCtx<'_>,
    tool: &ToolView,
    setting: &ToolSetting,
    rect: Rect,
    live: bool,
    editing: &mut Editing,
    actions: &mut Vec<Action>,
) -> Option<Action> {
    let spec = &setting.spec;
    let set = |value: Value| Action::SetToolSetting(tool.slot, spec.key.clone(), value);
    let row = Row { rect, title: &spec.title, detail: &spec.detail };
    let id = Id::new(("notchbase.settings.tool", tool.slot.0, spec.key.as_str()));
    match &spec.kind {
        SettingKind::Toggle => {
            let on = setting.value.as_bool().unwrap_or_default();
            toggled(ctx, &row, &Toggle { id, on, enabled: live }).then(|| set(Value::Bool(!on)))
        }
        SettingKind::Choice { options } => {
            let width = (options.len() as f32 * SEGMENT_W).min(CHOICE.x);
            let at = Rect::from_center_size(
                pos2(rect.right() - INSET - width * 0.5, rect.center().y),
                vec2(width, CHOICE.y),
            );
            paint_text(ctx, &row, at.left());
            let titles: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
            let current = setting.value.as_str().unwrap_or_default();
            let selected = options.iter().position(|option| option.value == current).unwrap_or(0);
            segmented(ctx.ui, ctx.canvas, at, id, &titles, selected, live)
                .map(|index| set(Value::String(options[index].value.clone())))
        }
        SettingKind::Number { min, max, step, unit } => {
            let value = setting.value.as_f64().unwrap_or(*min);
            stepper(ctx, &row, id, [value, *min, *max, *step], unit, live).map(|value| set(number(value)))
        }
        SettingKind::Folder { placeholder } => {
            let current = setting.value.as_str().unwrap_or_default().trim();
            let (choose, picked) = pill(ctx, rect.right() - INSET, rect.center().y, id, CHOOSE, live);
            let mut action = picked.then(|| Action::PickToolFolder(tool.slot, spec.key.clone()));
            let mut left = choose.left();
            if !current.is_empty() {
                let reset = round_button_left_of(choose.left(), rect.center().y);
                if round_button(ctx, reset, id.with("reset"), Glyph::Xmark, live) {
                    action = Some(set(Value::String(String::new())));
                }
                left = reset.left();
            }
            let (detail, tone) = folder_detail(current, placeholder, &spec.detail);
            paint_lines(ctx, &Row { rect, title: &spec.title, detail }, left, tone, true);
            action
        }
        SettingKind::Text { placeholder, secret } => {
            let current = setting.value.as_str().unwrap_or_default();
            let text = TextRow { tool, key: &spec.key, id, current, placeholder, secret: *secret, live };
            text_setting(ctx, &row, &text, editing, actions)
        }
        SettingKind::Data => None,
    }
}

struct TextRow<'a> {
    tool: &'a ToolView,
    key: &'a str,
    id: Id,
    current: &'a str,
    placeholder: &'a str,
    secret: bool,
    live: bool,
}

fn text_setting(
    ctx: &mut PageCtx<'_>,
    row: &Row,
    text: &TextRow,
    editing: &mut Editing,
    actions: &mut Vec<Action>,
) -> Option<Action> {
    let rect = row.rect;
    let at = Rect::from_center_size(pos2(rect.right() - INSET - FIELD.x * 0.5, rect.center().y), FIELD);
    let mut left = at.left();
    let mut action = None;
    let typing = editing.focus.as_ref().is_some_and(|focus| focus.is(text.id));
    if !text.current.is_empty() && !typing {
        let reset = round_button_left_of(at.left(), rect.center().y);
        if round_button(ctx, reset, text.id.with("reset"), Glyph::Xmark, text.live) {
            action = Some(Action::SetToolSetting(text.tool.slot, text.key.to_owned(), Value::String(String::new())));
        }
        left = reset.left();
    }
    paint_text(ctx, row, left);
    let input = Field {
        id: text.id,
        rect: at,
        value: text.current,
        prefix: "",
        placeholder: text.placeholder,
        secret: text.secret,
        editable: text.live,
    };
    let edits = field::field(ctx.ui, ctx.canvas, &input, &mut editing.focus, actions);
    editing.aim(text.id, text.tool.slot, text.key, text.current);
    for edit in edits {
        match edit {
            Edit::Submitted(typed) => action = action.or(editing.submit(&typed)),
            Edit::Cancelled => editing.cancel(),
            Edit::Typed(_) => {}
        }
    }
    action
}

fn folder_detail<'a>(current: &'a str, placeholder: &'a str, detail: &'a str) -> (&'a str, egui::Color32) {
    match (current, placeholder) {
        ("", "") => (detail, secondary_label()),
        ("", placeholder) => (placeholder, tertiary_label()),
        (current, _) => (current, secondary_label()),
    }
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 { Value::from(value as i64) } else { Value::from(value) }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::SettingSpec;

    use super::*;
    use crate::bridge::ToolSlot;

    fn tool(settings: Vec<SettingSpec>) -> ToolView {
        ToolView {
            slot: ToolSlot(1),
            id: "t".into(),
            name: "Tool".into(),
            tile: None,
            page: None,
            live: None,
            settings: settings.into_iter().map(|spec| ToolSetting { value: spec.default.clone(), spec }).collect(),
            failure: None,
        }
    }

    #[test]
    fn only_editable_settings_get_rows() {
        let settings = vec![
            SettingSpec::toggle("a", "A", true),
            SettingSpec::text("b", "B", ""),
            SettingSpec::toggle("c", "C", false).hidden(),
            SettingSpec::number("d", "D", [1.0, 9.0], 1.0, 3.0),
            SettingSpec::data("e", Value::Null),
        ];
        let mixed = tool(settings);
        let keys: Vec<&str> = shown(&mixed).map(|setting| setting.spec.key.as_str()).collect();
        assert_eq!(keys, ["a", "b", "d"]);
        assert!(has_settings(&mixed));
        assert!(!has_settings(&tool(vec![SettingSpec::data("e", Value::Null)])));
    }

    #[test]
    fn setting_can_wait_for_choice() {
        let settings = vec![
            SettingSpec::choice("source", "Source", [("apple", "Apple"), ("ical", "iCal")], "apple"),
            SettingSpec::text("link", "Link", "").secret().shown_when_is("source", "ical"),
        ];
        let mut apple = tool(settings);
        assert_eq!(shown(&apple).count(), 1);
        apple.settings[0].value = Value::String("ical".into());
        let keys: Vec<&str> = shown(&apple).map(|setting| setting.spec.key.as_str()).collect();
        assert_eq!(keys, ["source", "link"]);
    }

    #[test]
    fn whole_numbers_stay_whole() {
        assert_eq!(number(25.0), Value::from(25));
        assert_eq!(number(2.5), Value::from(2.5));
    }
}
