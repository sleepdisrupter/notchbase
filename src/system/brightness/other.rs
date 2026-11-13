//! Other systems have no brightness control here.

pub struct Display;

impl Display {
    pub fn open() -> Self {
        Self
    }

    pub fn is_adjustable(&mut self) -> bool {
        false
    }

    pub fn read(&mut self) -> Option<f32> {
        None
    }

    pub fn set(&mut self, _level: f32) {}
}
