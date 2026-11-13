//! Makes a PDF page from an image. A JPEG goes in as it is, anything else is decoded first.

use std::fs;
use std::path::Path;

use image::GenericImageView;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use super::jpeg::embed_jpeg;
use crate::shelf::icc;
use crate::shelf::images::{self, Decoded};
use crate::shelf::names::couldnt_read;

const POINTS_PER_PIXEL: f32 = 0.75;
const MAX_PAGE_SIDE_POINTS: f32 = 14_400.0;

pub(super) struct EmbeddedImage {
    pub(super) id: ObjectId,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) page_rotation: i64,
}

pub(super) fn image_page(doc: &mut Document, path: &Path, parent: ObjectId) -> Result<ObjectId, String> {
    let bytes = fs::read(path).map_err(|_| couldnt_read(path))?;
    let image = match embed_jpeg(doc, &bytes) {
        Some(jpeg) => jpeg,
        None => embed_decoded(doc, path)?,
    };
    let scale = POINTS_PER_PIXEL.min(MAX_PAGE_SIDE_POINTS / image.width.max(image.height) as f32);
    let width = hundredths(image.width as f32 * scale);
    let height = hundredths(image.height as f32 * scale);
    let draw_image = format!("q {width:.2} 0 0 {height:.2} 0 0 cm /Im0 Do Q").into_bytes();
    let contents = doc.add_object(Stream::new(Dictionary::new(), draw_image));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => parent,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image.id } },
        "Contents" => contents,
    };
    if image.page_rotation != 0 {
        page.set("Rotate", image.page_rotation);
    }
    Ok(doc.add_object(page))
}

fn hundredths(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn embed_decoded(doc: &mut Document, path: &Path) -> Result<EmbeddedImage, String> {
    let Decoded { image, icc } = images::decode(path)?;
    let (width, height) = image.dimensions();
    let gray = !image.color().has_color();
    let alpha = image
        .color()
        .has_alpha()
        .then(|| image.to_rgba8().pixels().map(|pixel| pixel[3]).collect::<Vec<u8>>())
        .filter(|alpha| alpha.iter().any(|&a| a != 255));
    let samples = if gray { image.to_luma8().into_raw() } else { image.to_rgb8().into_raw() };
    drop(image);

    let color_space = color_space(doc, icc, gray);
    let mut dict = image_dictionary(width, height, color_space);
    if let Some(alpha) = alpha {
        let soft_mask = deflated(Stream::new(image_dictionary(width, height, "DeviceGray".into()), alpha));
        dict.set("SMask", doc.add_object(soft_mask));
    }
    let id = doc.add_object(deflated(Stream::new(dict, samples)));
    Ok(EmbeddedImage { id, width, height, page_rotation: 0 })
}

pub(super) fn image_dictionary(width: u32, height: u32, color_space: Object) -> Dictionary {
    dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width,
        "Height" => height,
        "ColorSpace" => color_space,
        "BitsPerComponent" => 8,
    }
}

fn deflated(mut stream: Stream) -> Stream {
    let _kept_as_is_when_not_smaller = stream.compress();
    stream
}

pub(super) fn color_space(doc: &mut Document, icc: Option<Vec<u8>>, gray: bool) -> Object {
    let (device, components) = if gray { ("DeviceGray", 1) } else { ("DeviceRGB", 3) };
    match icc {
        Some(profile) if icc::matches(&profile, !gray) => {
            let stream = deflated(Stream::new(dictionary! { "N" => components, "Alternate" => device }, profile));
            Object::Array(vec!["ICCBased".into(), doc.add_object(stream).into()])
        }
        _ => device.into(),
    }
}
