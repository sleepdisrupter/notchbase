//! Compresses a PDF losslessly by deflating its plain streams again at the best level.

use std::path::Path;

use lopdf::{Document, Object, Stream};

use super::save::save;
use super::{MAX_STREAM_BYTES, load};
use crate::shelf::names::file_name;

pub(in crate::shelf) fn compress(path: &Path) -> Result<Vec<u8>, String> {
    let mut doc = load(path)?;
    redeflate_at_best_level(&mut doc);
    save(&mut doc).map_err(|_| format!("Couldn't compress {}", file_name(path)))
}

fn redeflate_at_best_level(doc: &mut Document) {
    for object in doc.objects.values_mut() {
        if let Object::Stream(stream) = object
            && is_plain_flate(stream)
            && let Some(smaller) = redeflated(stream)
        {
            *stream = smaller;
        }
    }
}

fn is_plain_flate(stream: &Stream) -> bool {
    let only_flate = matches!(stream.filters().as_deref(), Ok([filter]) if *filter == b"FlateDecode");
    stream.allows_compression && only_flate && !stream.dict.has(b"DecodeParms")
}

fn redeflated(stream: &Stream) -> Option<Stream> {
    let content = stream.decompressed_content_with_limit(MAX_STREAM_BYTES).ok()?;
    let mut again = stream.clone();
    again.set_plain_content(content);
    again.compress().ok()?;
    let smaller = again.dict.has(b"Filter") && again.content.len() < stream.content.len();
    smaller.then_some(again)
}
