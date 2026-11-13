//! A row of color swatches to choose from.

use super::{Element, Kind, Map};
use crate::protocol::Color;

pub struct Palette<Msg> {
    colors: Vec<Color>,
    selected: Option<usize>,
    on_select: Option<Map<usize, Msg>>,
}

pub fn palette<Msg, C: Into<Color>>(colors: impl IntoIterator<Item = C>) -> Palette<Msg> {
    Palette { colors: colors.into_iter().map(Into::into).collect(), selected: None, on_select: None }
}

impl<Msg> Palette<Msg> {
    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn on_select(mut self, map: impl Fn(usize) -> Msg + 'static) -> Self {
        self.on_select = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Palette<Msg>> for Element<Msg> {
    fn from(palette: Palette<Msg>) -> Self {
        Element::new(Kind::Palette { colors: palette.colors, selected: palette.selected, on_select: palette.on_select })
    }
}
