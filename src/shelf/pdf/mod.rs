//! PDFs with lopdf: counting pages, combining, compressing and splitting.

mod combine;
mod compress;
mod graph;
mod image_page;
mod jpeg;
mod page_tree;
mod save;
mod split;

use std::path::Path;

use lopdf::{Document, LoadOptions};

use super::names::{couldnt_read, file_name};

pub(super) use combine::combine;
pub(super) use compress::compress;
pub(super) use split::Pages;

// Caps on unpacking, so a small PDF can't expand into gigabytes.
const MAX_STREAM_BYTES: usize = 512 << 20;
const MAX_STRUCTURE_STREAM_BYTES: usize = 64 << 20;

pub(super) fn page_count(path: &Path) -> Option<u32> {
    let doc = load(path).ok()?;
    u32::try_from(page_tree::distinct_pages(&doc).len()).ok()
}

fn load(path: &Path) -> Result<Document, String> {
    let options = LoadOptions { max_decompressed_size: Some(MAX_STRUCTURE_STREAM_BYTES), ..LoadOptions::default() };
    let mut doc = Document::load_with_options(path, options).map_err(|_| couldnt_read(path))?;
    let still_locked = doc.trailer.has(b"Encrypt");
    if still_locked {
        return Err(format!("{} is password-protected", file_name(path)));
    }
    graph::null_dangling_refs(&mut doc);
    if doc.page_iter().next().is_none() {
        return Err(couldnt_read(path));
    }
    Ok(doc)
}
