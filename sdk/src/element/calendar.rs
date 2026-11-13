//! Month and week calendar views.

use super::{Element, Kind, Map};
use crate::protocol::{Color, DayMark, MonthNode, WeekDay, WeekNode};

pub struct Month<Msg> {
    node: MonthNode,
    on_select: Option<Map<usize, Msg>>,
    on_scroll: Option<Map<f32, Msg>>,
}

pub fn month<Msg>(year: i32, month: u8) -> Month<Msg> {
    Month {
        node: MonthNode { year, month: month.clamp(1, 12), ..MonthNode::default() },
        on_select: None,
        on_scroll: None,
    }
}

impl<Msg> Month<Msg> {
    pub fn today(mut self, day: u8) -> Self {
        self.node.today = Some(day);
        self
    }

    pub fn selected(mut self, day: u8) -> Self {
        self.node.selected = Some(day);
        self
    }

    pub fn mark(mut self, day: u8, color: impl Into<Color>) -> Self {
        let color = color.into();
        match self.node.marks.iter_mut().find(|mark| mark.day == day) {
            Some(mark) => mark.colors.push(color),
            None => self.node.marks.push(DayMark { day, colors: vec![color] }),
        }
        self
    }

    pub fn monday_first(mut self, monday_first: bool) -> Self {
        self.node.monday_first = monday_first;
        self
    }

    pub fn on_select(mut self, map: impl Fn(i32, u8, u8) -> Msg + 'static) -> Self {
        self.on_select = Some(Box::new(move |packed: usize| {
            let (year, month, day) = MonthNode::unpack(packed);
            map(year, month, day)
        }));
        self
    }

    pub fn on_scroll(mut self, map: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_scroll = Some(stepped(map));
        self
    }
}

impl<Msg> From<Month<Msg>> for Element<Msg> {
    fn from(month: Month<Msg>) -> Self {
        Element::new(Kind::Month { node: month.node, on_select: month.on_select, on_scroll: month.on_scroll })
    }
}

fn stepped<Msg>(map: impl Fn(i32) -> Msg + 'static) -> Map<f32, Msg> {
    Box::new(move |steps: f32| map(steps.round() as i32))
}

pub struct Week<Msg> {
    days: Vec<WeekDay>,
    on_day: Option<Map<usize, Msg>>,
    on_event: Option<Map<String, Msg>>,
    on_scroll: Option<Map<f32, Msg>>,
}

pub fn week<Msg>(days: impl IntoIterator<Item = WeekDay>) -> Week<Msg> {
    Week { days: days.into_iter().collect(), on_day: None, on_event: None, on_scroll: None }
}

impl<Msg> Week<Msg> {
    pub fn on_day(mut self, map: impl Fn(i32, u8, u8) -> Msg + 'static) -> Self {
        self.on_day = Some(Box::new(move |packed: usize| {
            let (year, month, day) = MonthNode::unpack(packed);
            map(year, month, day)
        }));
        self
    }

    pub fn on_event(mut self, map: impl Fn(String) -> Msg + 'static) -> Self {
        self.on_event = Some(Box::new(map));
        self
    }

    pub fn on_scroll(mut self, map: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_scroll = Some(stepped(map));
        self
    }
}

impl<Msg> From<Week<Msg>> for Element<Msg> {
    fn from(week: Week<Msg>) -> Self {
        Element::new(Kind::Week {
            node: WeekNode { days: week.days, ..WeekNode::default() },
            on_day: week.on_day,
            on_event: week.on_event,
            on_scroll: week.on_scroll,
        })
    }
}
