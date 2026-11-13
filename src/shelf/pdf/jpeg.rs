//! Embeds a JPEG straight into a PDF when its frame is one PDF readers can show.

use std::io::Cursor;

use image::ImageDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::metadata::Orientation;
use lopdf::{Document, Stream};

use super::image_page::{EmbeddedImage, color_space, image_dictionary};

const START_OF_IMAGE: [u8; 2] = [0xFF, 0xD8];
const MARKER_PREFIX: u8 = 0xFF;
const PDF_READABLE_FRAMES: std::ops::RangeInclusive<u8> = 0xC0..=0xC2;

struct FrameHeader {
    marker: u8,
    precision: u8,
    width: u16,
    height: u16,
    components: u8,
}

pub(super) fn embed_jpeg(doc: &mut Document, bytes: &[u8]) -> Option<EmbeddedImage> {
    let frame = frame_header(bytes)?;
    let embeddable = PDF_READABLE_FRAMES.contains(&frame.marker)
        && frame.precision == 8
        && matches!(frame.components, 1 | 3)
        && frame.width > 0
        && frame.height > 0;
    if !embeddable {
        return None;
    }
    let mut decoder = JpegDecoder::new(Cursor::new(bytes)).ok()?;
    let page_rotation = page_rotation(decoder.orientation().ok()?)?;
    let color_space = color_space(doc, decoder.icc_profile().ok().flatten(), frame.components == 1);
    let (width, height) = (u32::from(frame.width), u32::from(frame.height));
    let mut dict = image_dictionary(width, height, color_space);
    dict.set("Filter", "DCTDecode");
    let id = doc.add_object(Stream::new(dict, bytes.to_vec()).with_compression(false));
    Some(EmbeddedImage { id, width, height, page_rotation })
}

fn page_rotation(orientation: Orientation) -> Option<i64> {
    match orientation {
        Orientation::NoTransforms => Some(0),
        Orientation::Rotate90 => Some(90),
        Orientation::Rotate180 => Some(180),
        Orientation::Rotate270 => Some(270),
        Orientation::FlipHorizontal
        | Orientation::FlipVertical
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => None,
    }
}

fn frame_header(bytes: &[u8]) -> Option<FrameHeader> {
    if !bytes.starts_with(&START_OF_IMAGE) {
        return None;
    }
    let mut at = START_OF_IMAGE.len();
    while at + 4 <= bytes.len() {
        if bytes[at] != MARKER_PREFIX {
            return None;
        }
        let marker = bytes[at + 1];
        match marker {
            MARKER_PREFIX => at += 1,
            0x01 | 0xD0..=0xD8 => at += 2,
            0xD9 | 0xDA => return None,
            _ if is_start_of_frame(marker) => return parse_frame(marker, bytes.get(at + 4..at + 10)?),
            _ => {
                let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
                if length < 2 {
                    return None;
                }
                at += 2 + length;
            }
        }
    }
    None
}

fn is_start_of_frame(marker: u8) -> bool {
    const HUFFMAN_TABLES: u8 = 0xC4;
    const RESERVED_EXTENSION: u8 = 0xC8;
    const ARITHMETIC_CONDITIONING: u8 = 0xCC;
    (0xC0..=0xCF).contains(&marker) && !matches!(marker, HUFFMAN_TABLES | RESERVED_EXTENSION | ARITHMETIC_CONDITIONING)
}

fn parse_frame(marker: u8, frame: &[u8]) -> Option<FrameHeader> {
    let &[precision, height_high, height_low, width_high, width_low, components] = frame else { return None };
    Some(FrameHeader {
        marker,
        precision,
        width: u16::from_be_bytes([width_high, width_low]),
        height: u16::from_be_bytes([height_high, height_low]),
        components,
    })
}
