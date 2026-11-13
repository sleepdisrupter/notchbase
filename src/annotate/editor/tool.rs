//! The two tools, the brush and text.

use crate::ui::glyph::Glyph;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Brush,
    Text,
}

impl Tool {
    pub(super) const ALL: [Self; 2] = [Self::Brush, Self::Text];

    pub(super) fn index(self) -> usize {
        match self {
            Self::Brush => 0,
            Self::Text => 1,
        }
    }

    pub(super) fn glyph(self) -> Glyph {
        match self {
            Self::Brush => Glyph::Brush,
            Self::Text => Glyph::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_brush_tools_in_order() {
        assert_eq!(Tool::default(), Tool::Brush);
        assert_eq!(Tool::ALL.map(Tool::index), [0, 1]);
        assert_eq!(Tool::ALL.map(Tool::glyph), [Glyph::Brush, Glyph::Text]);
    }
}
