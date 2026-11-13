//! Saves a PDF compactly: unreachable objects dropped, plain streams deflated, and objects packed into streams.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, SaveOptions};

use super::graph::each_ref;

const TRAILER_KEYS_STILL_VALID: [&[u8]; 3] = [b"Root", b"Info", b"ID"];
const BEST_DEFLATE_LEVEL: u32 = 9;

pub(super) fn save(doc: &mut Document) -> lopdf::Result<Vec<u8>> {
    keep_only_valid_trailer_keys(doc);
    prune_unreachable(doc);
    deflate_unfiltered_streams(doc);
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .compression_level(BEST_DEFLATE_LEVEL)
        .build();
    let mut out = Vec::new();
    doc.save_with_options(&mut out, options)?;
    Ok(out)
}

fn keep_only_valid_trailer_keys(doc: &mut Document) {
    let mut trailer = Dictionary::new();
    for key in TRAILER_KEYS_STILL_VALID {
        if let Ok(value) = doc.trailer.get(key) {
            trailer.set(key, value.clone());
        }
    }
    doc.trailer = trailer;
}

fn prune_unreachable(doc: &mut Document) {
    let mut pending = Vec::new();
    for (_, value) in doc.trailer.iter() {
        each_ref(value, &mut |id| pending.push(id));
    }
    let mut reachable = HashSet::new();
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if let Some(object) = doc.objects.get(&id) {
            each_ref(object, &mut |reference| {
                if !reachable.contains(&reference) {
                    pending.push(reference);
                }
            });
        }
    }
    doc.objects.retain(|id, _| reachable.contains(id));
}

fn deflate_unfiltered_streams(doc: &mut Document) {
    for object in doc.objects.values_mut() {
        if let Object::Stream(stream) = object
            && stream.allows_compression
            && !stream.dict.has_type(b"Metadata")
        {
            let _kept_as_is_when_not_smaller = stream.compress();
        }
    }
}
