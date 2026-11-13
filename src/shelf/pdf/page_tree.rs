//! Reading the page tree: each page once, with what it inherits from above filled in.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

const INHERITABLE_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const MAX_TREE_DEPTH: usize = 64;
const US_LETTER_MEDIA_BOX: [i64; 4] = [0, 0, 612, 792];

pub(super) fn distinct_pages(doc: &Document) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    doc.page_iter().filter(|page| seen.insert(*page)).collect()
}

pub(super) fn flattened_page(doc: &Document, page: ObjectId) -> Option<Dictionary> {
    let mut dict = doc.get_dictionary(page).ok()?.clone();
    for key in INHERITABLE_PAGE_KEYS {
        if !dict.has(key)
            && let Some(value) = inherited(doc, page, key)
        {
            dict.set(key, value.clone());
        }
    }
    if !dict.has(b"MediaBox") {
        let letter: Vec<Object> = US_LETTER_MEDIA_BOX.iter().map(|&side| Object::Integer(side)).collect();
        dict.set("MediaBox", letter);
    }
    Some(dict)
}

pub(super) fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node = doc.get_dictionary(page).ok()?;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}
