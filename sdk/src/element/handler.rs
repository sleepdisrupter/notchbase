//! The table that maps handler ids in a lowered view back to messages.

use crate::protocol::{HandlerId, Pointer, UiValue};

pub(crate) enum Handler<Msg> {
    Press(Msg),
    Bool(Box<dyn Fn(bool) -> Msg>),
    Index(Box<dyn Fn(usize) -> Msg>),
    Number(Box<dyn Fn(f32) -> Msg>),
    Text(Box<dyn Fn(String) -> Msg>),
    Picked(Box<dyn Fn(usize, usize) -> Msg>),
    Pointer(Box<dyn Fn(Pointer) -> Msg>),
}

impl<Msg: Clone> Handler<Msg> {
    fn call(&self, value: UiValue) -> Option<Msg> {
        match (self, value) {
            (Handler::Press(msg), _) => Some(msg.clone()),
            (Handler::Bool(map), UiValue::Bool(on)) => Some(map(on)),
            (Handler::Index(map), UiValue::Index(index)) => Some(map(index)),
            (Handler::Number(map), UiValue::Number(number)) => Some(map(number)),
            (Handler::Text(map), UiValue::Text(text)) => Some(map(text)),
            (Handler::Picked(map), UiValue::Picked { column, index }) => Some(map(column, index)),
            (Handler::Pointer(map), UiValue::Pointer(pointer)) => Some(map(pointer)),
            _ => None,
        }
    }
}

pub(crate) struct Handlers<Msg> {
    table: Vec<Handler<Msg>>,
}

impl<Msg> Default for Handlers<Msg> {
    fn default() -> Self {
        Self { table: Vec::new() }
    }
}

impl<Msg> Handlers<Msg> {
    pub(crate) fn add(&mut self, handler: Handler<Msg>) -> HandlerId {
        self.table.push(handler);
        HandlerId(self.table.len() as u32 - 1)
    }
}

impl<Msg: Clone> Handlers<Msg> {
    pub(crate) fn dispatch(&self, id: HandlerId, value: UiValue) -> Option<Msg> {
        self.table.get(id.0 as usize)?.call(value)
    }
}
