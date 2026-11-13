//! One file on the shelf and what is known about it so far: its kind, size, pages, length, dimensions and thumbnail.

use std::fs::Metadata;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::ColorImage;

use super::Kind;
use super::names::{file_name, lowercase_extension};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Dropped,
    Restored,
    Produced,
}

#[derive(Clone)]
pub struct Item {
    pub id: ItemId,
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub ext: String,
    pub bytes: Option<u64>,
    pub pages: Option<u32>,
    pub seconds: Option<f64>,
    pub dims: Option<[u32; 2]>,
    pub thumb: Option<Arc<ColorImage>>,
    pub thumb_rev: u64,
    pub added: Instant,
    pub origin: Origin,
}

impl Item {
    pub(super) fn new(id: ItemId, path: PathBuf, metadata: &Metadata, added: Instant, origin: Origin) -> Self {
        let is_dir = metadata.is_dir();
        Self {
            id,
            name: file_name(&path),
            kind: Kind::of_path(&path, is_dir),
            ext: if is_dir { String::new() } else { lowercase_extension(&path).to_uppercase() },
            bytes: (!is_dir).then_some(metadata.len()),
            pages: None,
            seconds: None,
            dims: None,
            thumb: None,
            thumb_rev: 0,
            added,
            origin,
            path,
        }
    }
}
