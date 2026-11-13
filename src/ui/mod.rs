//! The pieces the notch and its pages are drawn with. The basics, like the canvas, colors, glass and springs, come
//! from the notchbase_paint crate and are re-exported here.

pub mod agent_mark;
pub mod brand;
pub mod cover;
pub mod element;
pub mod field;
pub mod gl_window;
pub mod handover;
pub mod menu;
pub mod motion;
pub mod picture;
pub mod rows;
pub mod segmented;
pub mod sheen;
#[cfg(test)]
pub mod snapshot;
pub mod swipe;
pub mod trace;

pub use notchbase_paint::{
    buttons, canvas, color, controls, erase, font, glass, glyph, liquid, list, meter, metrics, scroll, shape, spring,
    switch, wave,
};
