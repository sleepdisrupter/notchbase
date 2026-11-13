//! Rows, columns, cards, lists and sizing.

use super::{Element, Kind, Map};
use crate::protocol::Align;

pub struct Row<Msg> {
    children: Vec<Element<Msg>>,
    spacing: Option<f32>,
    align: Align,
}

pub fn row<Msg>(children: impl IntoIterator<Item = Element<Msg>>) -> Row<Msg> {
    Row { children: children.into_iter().collect(), spacing: None, align: Align::Center }
}

impl<Msg> Row<Msg> {
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn push(mut self, child: impl Into<Element<Msg>>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl<Msg> From<Row<Msg>> for Element<Msg> {
    fn from(row: Row<Msg>) -> Self {
        Element::new(Kind::Row { children: row.children, spacing: row.spacing, align: row.align })
    }
}

pub struct Column<Msg> {
    children: Vec<Element<Msg>>,
    spacing: Option<f32>,
    align: Align,
}

pub fn column<Msg>(children: impl IntoIterator<Item = Element<Msg>>) -> Column<Msg> {
    Column { children: children.into_iter().collect(), spacing: None, align: Align::Start }
}

impl<Msg> Column<Msg> {
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn push(mut self, child: impl Into<Element<Msg>>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl<Msg> From<Column<Msg>> for Element<Msg> {
    fn from(column: Column<Msg>) -> Self {
        Element::new(Kind::Column { children: column.children, spacing: column.spacing, align: column.align })
    }
}

pub fn card<Msg>(child: impl Into<Element<Msg>>) -> Element<Msg> {
    Element::new(Kind::Card { child: Box::new(child.into()), padding: None })
}

pub fn list<Msg>(children: impl IntoIterator<Item = Element<Msg>>) -> Element<Msg> {
    Element::new(Kind::List { children: children.into_iter().collect() })
}

/// Shows `notch` around a camera notch and `island` on the floating island.
pub fn adaptive<Msg>(notch: impl Into<Element<Msg>>, island: impl Into<Element<Msg>>) -> Element<Msg> {
    Element::new(Kind::Adaptive { notch: Box::new(notch.into()), island: Box::new(island.into()) })
}

/// Gives `child` an identity of its own, so its animations follow it when a list reorders.
pub fn keyed<Msg>(key: impl Into<String>, child: impl Into<Element<Msg>>) -> Element<Msg> {
    Element::new(Kind::Keyed { key: key.into(), child: Box::new(child.into()) })
}

pub struct Pan<Msg> {
    child: Element<Msg>,
    offset: f32,
    on_pan: Option<Map<f32, Msg>>,
    on_tap: Option<Map<f32, Msg>>,
}

/// A strip wider than its room that the user drags sideways. The tool keeps the offset.
pub fn pan<Msg>(child: impl Into<Element<Msg>>) -> Pan<Msg> {
    Pan { child: child.into(), offset: 0.0, on_pan: None, on_tap: None }
}

impl<Msg> Pan<Msg> {
    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub fn on_pan(mut self, map: impl Fn(f32) -> Msg + 'static) -> Self {
        self.on_pan = Some(Box::new(map));
        self
    }

    pub fn on_tap(mut self, map: impl Fn(f32) -> Msg + 'static) -> Self {
        self.on_tap = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Pan<Msg>> for Element<Msg> {
    fn from(pan: Pan<Msg>) -> Self {
        let Pan { child, offset, on_pan, on_tap } = pan;
        Element::new(Kind::Pan { child: Box::new(child), offset, on_pan, on_tap })
    }
}

pub trait Sizing<Msg>: Into<Element<Msg>> + Sized {
    fn width(self, width: f32) -> Element<Msg> {
        frame(self.into(), Some(width), None)
    }

    fn height(self, height: f32) -> Element<Msg> {
        frame(self.into(), None, Some(height))
    }

    fn frame(self, width: f32, height: f32) -> Element<Msg> {
        frame(self.into(), Some(width), Some(height))
    }

    /// Only cards take padding. Anything else is left as it is.
    fn padding(self, padding: f32) -> Element<Msg> {
        match self.into() {
            Element { kind: Kind::Card { child, .. } } => Element::new(Kind::Card { child, padding: Some(padding) }),
            other => other,
        }
    }
}

impl<Msg, T: Into<Element<Msg>>> Sizing<Msg> for T {}

fn frame<Msg>(child: Element<Msg>, width: Option<f32>, height: Option<f32>) -> Element<Msg> {
    Element::new(Kind::Frame { child: Box::new(child), width, height, align: Align::Center })
}

#[macro_export]
macro_rules! row {
    () => {
        $crate::element::row(::std::vec::Vec::new())
    };
    ($($child:expr),+ $(,)?) => {
        $crate::element::row(::std::vec![$($crate::element::Element::from($child)),+])
    };
}

#[macro_export]
macro_rules! column {
    () => {
        $crate::element::column(::std::vec::Vec::new())
    };
    ($($child:expr),+ $(,)?) => {
        $crate::element::column(::std::vec![$($crate::element::Element::from($child)),+])
    };
}
