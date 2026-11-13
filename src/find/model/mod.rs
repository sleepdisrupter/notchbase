//! The index in memory. Settled rows sit in a base sorted so a folder's children are together, rows added since
//! sit in an overlay, and a rebuild merges the two. Names are stored folded for matching.

pub(in crate::find) mod base;
pub(in crate::find) mod columns;
mod evict;
pub(in crate::find) mod exotic;
mod fold;
mod live;
mod merge;
mod overlay;
mod paths;
#[cfg(test)]
mod tests;

pub use fold::*;
pub use live::*;

pub type EntryId = u32;

pub mod flag {
    pub const DIR: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const TIER_MASK: u8 = 0x0C;
    pub const TIER_SHIFT: u8 = 2;
    pub const INSIDE_PACKAGE: u8 = 0x10;
    pub const CLOUD: u8 = 0x20;
    pub const LINK: u8 = 0x40;
    pub const EXOTIC: u8 = 0x80;
    pub const IDENTITY: u8 = DIR | LINK;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub modified: u32,
    pub created: u32,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Arena<'a> {
    pub bytes: &'a [u8],
    pub first: EntryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub cap: usize,
    pub level: u8,
    pub boot: Option<EntryId>,
}

pub struct Rebuilt {
    base: base::Base,
}

pub const MERGE_ROWS: usize = 65_536;
pub const OVERLAY_MAX: usize = 1_048_576;

pub fn overlay_limit(base_rows: usize) -> usize {
    base_rows.clamp(MERGE_ROWS, OVERLAY_MAX)
}

pub fn tier(flags: u8) -> u8 {
    (flags & flag::TIER_MASK) >> flag::TIER_SHIFT
}

pub fn with_tier(flags: u8, tier: u8) -> u8 {
    (flags & !flag::TIER_MASK) | ((tier << flag::TIER_SHIFT) & flag::TIER_MASK)
}
