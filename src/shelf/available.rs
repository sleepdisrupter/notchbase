//! Which operations to offer for a selection. All video, all audio, or a mix of images and PDFs each get their own set.

use super::images::is_compressible_ext;
use super::{Clip, Format, Item, Kind, Op};

pub fn available_ops(items: &[&Item]) -> Vec<Op> {
    if !items.is_empty() && items.iter().all(|i| i.kind == Kind::Video) {
        return vec![Op::Encode(Clip::Mp4), Op::Compress];
    }
    if !items.is_empty() && items.iter().all(|i| i.kind == Kind::Audio) {
        return vec![Op::Encode(Clip::M4a)];
    }
    let all_images_or_pdfs = items.iter().all(|i| matches!(i.kind, Kind::Image | Kind::Pdf));
    if items.is_empty() || !all_images_or_pdfs {
        return Vec::new();
    }
    let images = items.iter().filter(|i| i.kind == Kind::Image).count();
    let only_images = images == items.len();
    let all_compressible = items.iter().all(|i| i.kind == Kind::Pdf || is_compressible_ext(&i.ext));
    let combinable = items.len() >= 2;
    let splittable = matches!(items, [pdf] if pdf.kind == Kind::Pdf && pdf.pages != Some(1));

    let mut ops = Vec::new();
    if only_images {
        ops.push(Op::Convert(Format::Png));
    }
    if all_compressible {
        ops.push(Op::Compress);
    }
    if combinable {
        ops.push(Op::Combine);
    }
    if splittable {
        ops.push(Op::Split);
    }
    ops
}
