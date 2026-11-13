//! A mark on the picture: a line or a block of words.

use egui::Rect;

use super::lettering::Lettering;
use super::stroke::Stroke;

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Line(Stroke),
    Text(Lettering),
}

impl Mark {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Line(stroke) => stroke.bounds(),
            Self::Text(lettering) => lettering.bounds(),
        }
    }

    #[cfg(test)]
    pub fn line(&self) -> Option<&Stroke> {
        match self {
            Self::Line(stroke) => Some(stroke),
            Self::Text(_) => None,
        }
    }

    pub fn text(&self) -> Option<&Lettering> {
        match self {
            Self::Line(_) => None,
            Self::Text(lettering) => Some(lettering),
        }
    }
}

impl From<Stroke> for Mark {
    fn from(stroke: Stroke) -> Self {
        Self::Line(stroke)
    }
}

impl From<Lettering> for Mark {
    fn from(lettering: Lettering) -> Self {
        Self::Text(lettering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::PixelRect;
    use crate::annotate::testing::{block, line};
    use crate::ui::color;

    const AREA: PixelRect = PixelRect { x: 8, y: 6, width: 20, height: 10 };

    fn crosses_threads<Sent: Send>(_: &Sent) -> bool {
        true
    }

    #[test]
    fn mark_is_a_line_or_words() {
        let (stroke, lettering) = (line(color::RED, 4.0, [2.0, 2.0], [30.0, 2.0]), block(color::GREEN, AREA));
        let (drawn, written) = (Mark::from(stroke.clone()), Mark::from(lettering.clone()));
        assert_eq!((drawn.line(), drawn.text()), (Some(&stroke), None));
        assert_eq!((written.line(), written.text()), (None, Some(&lettering)));
        assert!(drawn != written && drawn == drawn.clone() && written == written.clone());
        assert!(crosses_threads(&drawn) && crosses_threads(&written));
    }

    #[test]
    fn mark_bounded_by_its_ink() {
        let stroke = line(color::RED, 4.0, [2.0, 2.0], [30.0, 2.0]);
        assert_eq!(Mark::from(stroke.clone()).bounds(), stroke.bounds());
        assert_eq!(Mark::from(block(color::GREEN, AREA)).bounds(), AREA.to_rect());
        assert_eq!(Mark::from(Lettering::empty()).bounds(), Rect::NOTHING);
    }
}
