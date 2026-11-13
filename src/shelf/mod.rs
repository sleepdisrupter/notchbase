//! The shelf, where files dropped on the notch wait to be dragged out, shared or worked on. It converts and compresses
//! images, video and audio, combines images and PDFs and splits a PDF into pages, saving each result beside its source.

pub(crate) mod airdrop;
mod audio;
mod available;
mod format;
mod icc;
mod images;
mod item;
mod kind;
mod model;
mod names;
mod notice;
mod ops;
mod path_key;
mod pdf;
mod png_opt;
mod store;
mod task;
#[cfg(test)]
mod tests;
mod video;
mod worker;

pub use available::available_ops;
pub use format::{Clip, Format, Op};
pub use item::{Item, ItemId, Origin};
pub use kind::Kind;
pub use model::Shelf;
pub use notice::Notice;
use task::Task;
pub use worker::Busy;

pub const THUMB_PX: u32 = 128;

// No decode may take more than this, so a huge or hostile file is refused instead of eating the memory.
const MAX_DECODED_BYTES: u64 = 512 << 20;
