//! The groups settings are sorted into: one tile each on the home view, and a view of its own rows behind it.

use egui::{Align2, pos2};

use super::app;
use super::assistant;
use super::catalog::{self, Kind};
use super::dock;
use super::editing::Editing;
use super::find;
use super::item::{Item, Went, notifications_summary};
use super::notch::{self, NotchRow};
use super::rows::{BACK_H, INSET, INTERACTIVE_FROM, ROW_H, SECTION_H, back, card, section};
use super::setting::shown;
use crate::bridge::{Action, PanelData};
use crate::pages::PageCtx;
use crate::ui::color::tertiary_label;
use crate::ui::glyph::Glyph;
use crate::ui::metrics::GAP;
use crate::ui::{controls, font};

const HINT_H: f32 = 22.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Group {
    Popups,
    Notch,
    Windows,
    Find,
    Assistant,
    Dock,
    Tools,
    Indicators,
    Notifications,
    Permissions,
    App,
}

const ALL: [Group; 11] = [
    Group::Popups,
    Group::Notch,
    Group::Windows,
    Group::Find,
    Group::Assistant,
    Group::Dock,
    Group::Tools,
    Group::Indicators,
    Group::Notifications,
    Group::Permissions,
    Group::App,
];

/// What a group's view is made of, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Block {
    Section(String),
    Card(Vec<Item>),
    Hint(&'static str),
    /// A card with one row saying the list is empty.
    Nothing(Kind),
}

pub(super) enum Outcome {
    Stay,
    Back,
    Went(Went),
}

impl Group {
    /// The tiles this platform shows, in order.
    pub(super) fn tiles() -> Vec<Group> {
        ALL.into_iter().filter(|group| group.supported()).collect()
    }

    fn supported(self) -> bool {
        match self {
            Group::Dock => crate::system::dock::SUPPORTED,
            Group::Permissions => crate::system::permissions::SUPPORTED,
            _ => true,
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Group::Popups => "Popups",
            Group::Notch => "Notch",
            Group::Windows => "Windows",
            Group::Find => "Find",
            Group::Assistant => "Assistant",
            Group::Dock => "Dock",
            Group::Tools => Kind::Tools.title(),
            Group::Indicators => Kind::Indicators.title(),
            Group::Notifications => "Notifications",
            Group::Permissions => "Permissions",
            Group::App => "notchbase",
        }
    }

    pub(super) fn glyph(self) -> Glyph {
        match self {
            Group::Popups => Glyph::Speaker,
            Group::Notch => Glyph::Display,
            Group::Windows => Glyph::Frames,
            Group::Find => Glyph::Search,
            Group::Assistant => Glyph::Spark,
            Group::Dock => Glyph::AppIcon,
            Group::Tools => Glyph::Grid,
            Group::Indicators => Glyph::Battery,
            Group::Notifications => Glyph::Bell,
            Group::Permissions => Glyph::Shield,
            Group::App => Glyph::Gear,
        }
    }

    /// The tile's second line: how things stand in it now.
    pub(super) fn summary(self, data: &PanelData) -> String {
        match self {
            Group::Popups => popups_summary(data),
            Group::Notch => switched_on(notch::NOTCH, data),
            Group::Windows => on_off(data.snap_windows, "Layouts on"),
            Group::Find => find::summary(data.find),
            Group::Assistant => assistant::summary(data.assistant, data.questions_in_notch, data.progress_in_notch),
            Group::Dock => dock::summary(data.dock),
            Group::Tools => format!("{} installed", Kind::Tools.count(data.catalog)),
            Group::Indicators => format!("{} installed", Kind::Indicators.count(data.catalog)),
            Group::Notifications => notifications_summary(&data.app),
            Group::Permissions => "What the notch can use".to_owned(),
            Group::App => app::summary(&data.app),
        }
    }

    pub(super) fn blocks(self, data: &PanelData) -> Vec<Block> {
        let items = |rows: &[NotchRow]| Block::Card(rows.iter().copied().map(Item::Notch).collect());
        let mut blocks = vec![Block::Section(self.title().to_owned())];
        match self {
            Group::Popups => blocks.push(items(notch::POPUPS)),
            Group::Notch => blocks.push(items(notch::NOTCH)),
            Group::Windows => blocks.push(items(notch::WINDOWS)),
            Group::Find => blocks.push(Block::Card(find::ROWS.iter().copied().map(Item::Find).collect())),
            Group::Assistant => {
                blocks.push(Block::Card(assistant::ROWS.iter().copied().map(Item::Assistant).collect()));
            }
            Group::Dock => {
                blocks.push(Block::Card(dock::TOP.iter().copied().map(Item::Dock).collect()));
                for (title, rows) in dock::GROUPS {
                    blocks.push(Block::Section((*title).to_owned()));
                    blocks.push(Block::Card(rows.iter().copied().map(Item::Dock).collect()));
                }
            }
            Group::Tools => {
                blocks.push(Block::Card(catalog::ROWS.iter().copied().map(Item::Tools).collect()));
                blocks.push(Block::Hint(catalog::HINT));
                blocks.push(Block::Section(format!("Installed ({})", Kind::Tools.count(data.catalog))));
                blocks.push(installed(Kind::Tools, data));
            }
            Group::Indicators => blocks.push(installed(Kind::Indicators, data)),
            Group::Notifications => blocks.push(Block::Card(vec![Item::Notifications])),
            Group::Permissions => blocks.push(Block::Card(vec![Item::Permissions])),
            Group::App => blocks.push(Block::Card(app::rows(&data.app).into_iter().map(Item::App).collect())),
        }
        blocks
    }

    /// Every row the search looks through in this group, each with where it lives. A tool's own settings live under
    /// the tool's name.
    pub(super) fn searched(self, data: &PanelData) -> Vec<(Item, String)> {
        let mut found: Vec<(Item, String)> = self
            .blocks(data)
            .into_iter()
            .flat_map(|block| match block {
                Block::Card(items) => items,
                _ => Vec::new(),
            })
            .map(|item| (item, self.title().to_owned()))
            .collect();
        let kind = match self {
            Group::Tools => Kind::Tools,
            Group::Indicators => Kind::Indicators,
            _ => return found,
        };
        for info in kind.listed(data.catalog) {
            let Some(tool) = data.tools.iter().find(|tool| tool.id == info.id) else { continue };
            for setting in shown(tool) {
                found.push((Item::Setting(tool.id.clone(), setting.spec.key.clone()), tool.name.clone()));
            }
        }
        found
    }
}

fn installed(kind: Kind, data: &PanelData) -> Block {
    let items: Vec<Item> = kind.listed(data.catalog).map(|info| Item::Installed(info.id.clone())).collect();
    if items.is_empty() { Block::Nothing(kind) } else { Block::Card(items) }
}

fn popups_summary(data: &PanelData) -> String {
    let names: Vec<&str> = notch::POPUPS
        .iter()
        .filter(|row| row.on(data))
        .map(|row| match row {
            NotchRow::Brightness => "brightness",
            _ => "volume",
        })
        .collect();
    match names.as_slice() {
        [] => "Off".to_owned(),
        [only] if notch::POPUPS.len() > 1 => format!("{} only", capitalized(only)),
        [only] => format!("{} on", capitalized(only)),
        all => capitalized(&all.join(" and ")),
    }
}

fn switched_on(rows: &[NotchRow], data: &PanelData) -> String {
    let on = rows.iter().filter(|row| row.on(data)).count();
    format!("{on} of {} on", rows.len())
}

fn on_off(on: bool, said: &str) -> String {
    if on { said.to_owned() } else { "Off".to_owned() }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

pub(super) fn height(group: Group, data: &PanelData) -> f32 {
    let blocks: f32 = group.blocks(data).iter().map(block_height).sum();
    BACK_H + GAP + blocks
}

fn block_height(block: &Block) -> f32 {
    match block {
        Block::Section(_) => SECTION_H,
        Block::Card(items) => items.len() as f32 * ROW_H + GAP,
        Block::Hint(_) => HINT_H,
        Block::Nothing(_) => ROW_H + GAP,
    }
}

pub(super) fn ui(
    ctx: &mut PageCtx<'_>,
    group: Group,
    data: &PanelData,
    editing: &mut Editing,
    actions: &mut Vec<Action>,
) -> Outcome {
    let live = ctx.alpha > INTERACTIVE_FROM;
    let mut top = ctx.rect.top();
    let back = back(ctx, top, "Settings", live);
    top += BACK_H + GAP;
    let mut went = None;
    for block in group.blocks(data) {
        match &block {
            Block::Section(title) => {
                section(ctx, top, title);
            }
            Block::Card(items) => {
                for (item, rect) in items.iter().zip(card(ctx, top, items.len())) {
                    went = item.paint(ctx, rect, data, (live, editing), actions).or(went);
                }
            }
            Block::Hint(hint) => {
                let room = ctx.rect.width() - 2.0 * INSET;
                let shown = controls::truncate(ctx.canvas, hint, font::caption(), room);
                let at = pos2(ctx.rect.left() + INSET, top + HINT_H * 0.5 - GAP * 0.5);
                ctx.canvas.text(at, Align2::LEFT_CENTER, shown, font::caption(), tertiary_label());
            }
            Block::Nothing(kind) => {
                let rects = card(ctx, top, 1);
                catalog::nothing(ctx, rects[0], *kind);
            }
        }
        top += block_height(&block);
    }
    match (back, went) {
        (true, _) => Outcome::Back,
        (false, Some(went)) => Outcome::Went(went),
        (false, None) => Outcome::Stay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_shows_its_own_tiles() {
        let tiles = Group::tiles();
        assert_eq!(tiles.contains(&Group::Dock), crate::system::dock::SUPPORTED);
        assert_eq!(tiles.contains(&Group::Permissions), crate::system::permissions::SUPPORTED);
        assert_eq!(tiles.first(), Some(&Group::Popups));
        assert_eq!(tiles.last(), Some(&Group::App));
    }

    #[test]
    fn words_start_capitalized() {
        assert_eq!(capitalized("volume and brightness"), "Volume and brightness");
        assert_eq!(capitalized(""), "");
        assert_eq!(on_off(false, "Layouts on"), "Off");
    }

    #[test]
    fn every_block_has_height() {
        let rows = vec![Item::Notifications, Item::Permissions];
        assert_eq!(block_height(&Block::Card(rows)), 2.0 * ROW_H + GAP);
        assert_eq!(block_height(&Block::Section("Dock".into())), SECTION_H);
        assert_eq!(block_height(&Block::Nothing(Kind::Tools)), ROW_H + GAP);
    }
}
