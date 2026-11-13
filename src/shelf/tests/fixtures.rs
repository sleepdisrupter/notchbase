use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::{DynamicImage, ImageBuffer, ImageDecoder, ImageEncoder, ImageReader, Rgb, RgbImage, Rgba, RgbaImage};
use lopdf::xref::XrefType;
use lopdf::{Document, Object, ObjectId, Stream, dictionary};

use crate::poll::Wake;
use crate::shelf::ops::{self, Outcome, Outputs, Source};
use crate::shelf::{Kind, Op, Shelf, Task};

pub(in crate::shelf) struct TempDir(pub(in crate::shelf) PathBuf);

impl TempDir {
    pub(in crate::shelf) fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("notchbase-shelf-test-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(in crate::shelf) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn half_transparent() -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(64, 48, |x, y| {
        Rgba([(x * 4) as u8, (y * 5) as u8, 200, if x < 32 { 0 } else { 255 }])
    }))
}

pub(super) fn noise(width: u32, height: u32) -> RgbImage {
    let mut state = 0x2545_F491_u32;
    RgbImage::from_fn(width, height, |_, _| {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        };
        Rgb([next(), next(), next()])
    })
}

pub(super) fn save_image(image: &DynamicImage, path: PathBuf) -> PathBuf {
    image.save(&path).unwrap();
    path
}

pub(super) fn jpeg_bytes(image: &RgbImage, quality: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(image.clone())
        .write_with_encoder(JpegEncoder::new_with_quality(&mut bytes, quality))
        .unwrap();
    bytes
}

pub(super) fn save_jpeg(image: &RgbImage, quality: u8, path: PathBuf) -> PathBuf {
    fs::write(&path, jpeg_bytes(image, quality)).unwrap();
    path
}

pub(in crate::shelf) fn icc_profile(color_space: &[u8; 4]) -> Vec<u8> {
    let mut profile = vec![0; 132];
    profile[16..20].copy_from_slice(color_space);
    profile
}

pub(super) fn save_png_with_icc(image: &DynamicImage, profile: Vec<u8>, path: PathBuf) -> PathBuf {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder.set_icc_profile(profile).unwrap();
    image.write_with_encoder(encoder).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

pub(super) fn embedded_icc(path: &Path) -> Option<Vec<u8>> {
    let reader = ImageReader::open(path).unwrap().with_guessed_format().unwrap();
    reader.into_decoder().unwrap().icc_profile().unwrap()
}

pub(super) fn uncompressed_16_bit_gray_png(path: PathBuf) -> PathBuf {
    let image = DynamicImage::ImageRgba16(ImageBuffer::from_fn(160, 120, |x, y| {
        let level = (((x / 8) * 13 + (y / 8) * 7) % 256) as u16 * 257;
        Rgba([level, level, level, u16::MAX])
    }));
    let mut bytes = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut bytes, CompressionType::Uncompressed, PngFilter::NoFilter);
    image.write_with_encoder(encoder).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}

pub(super) struct PdfFixture {
    pub(super) pages: usize,
    pub(super) padding_bytes: usize,
    pub(super) inherited_rotation: Option<i64>,
    pub(super) first_page_listed_twice: bool,
    pub(super) dangling_thumbnails: bool,
}

impl PdfFixture {
    pub(super) fn pages(pages: usize) -> Self {
        Self {
            pages,
            padding_bytes: 0,
            inherited_rotation: None,
            first_page_listed_twice: false,
            dangling_thumbnails: false,
        }
    }

    pub(super) fn padded(mut self, bytes: usize) -> Self {
        self.padding_bytes = bytes;
        self
    }

    pub(super) fn rotated(mut self, degrees: i64) -> Self {
        self.inherited_rotation = Some(degrees);
        self
    }

    pub(super) fn listing_the_first_page_twice(mut self) -> Self {
        self.first_page_listed_twice = true;
        self
    }

    pub(super) fn with_dangling_thumbnails(mut self) -> Self {
        self.dangling_thumbnails = true;
        self
    }

    pub(super) fn save(&self, path: PathBuf) -> PathBuf {
        let mut doc = Document::with_version("1.5");
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let tree = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let pages: Vec<ObjectId> = (1..=self.pages)
            .map(|number| {
                let contents = doc.add_object(Stream::new(dictionary! {}, self.page_text(number)));
                doc.add_object(dictionary! { "Type" => "Page", "Parent" => tree, "Contents" => contents })
            })
            .collect();
        let mut kids: Vec<Object> = pages.iter().map(|&page| page.into()).collect();
        if self.first_page_listed_twice {
            kids.insert(1, pages[0].into());
        }
        let mut root = dictionary! {
            "Type" => "Pages",
            "Count" => kids.len() as i64,
            "Kids" => kids,
            "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        if let Some(degrees) = self.inherited_rotation {
            root.set("Rotate", degrees);
        }
        doc.objects.insert(tree, root.into());
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
        doc.trailer.set("Root", catalog);
        if self.dangling_thumbnails {
            let missing = (doc.max_id + 1, 0);
            for &page in &pages {
                doc.get_dictionary_mut(page).unwrap().set("Thumb", missing);
            }
        }
        doc.save(&path).unwrap();
        path
    }

    fn page_text(&self, number: usize) -> Vec<u8> {
        let mut content = format!("BT /F1 24 Tf 72 700 Td (Page {number}) Tj ET\n").into_bytes();
        while content.len() < self.padding_bytes {
            content.extend_from_slice(b"% the quick brown fox jumps over the lazy dog\n");
        }
        content
    }
}

pub(super) fn with_exif_orientation_6(jpeg: &[u8]) -> Vec<u8> {
    const ORIENTATION_TAG: u16 = 0x0112;
    const SHORT: u16 = 3;
    const ROTATE_90_CLOCKWISE: u32 = 6;
    let mut tiff = b"II*\0\x08\0\0\0".to_vec();
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&ORIENTATION_TAG.to_le_bytes());
    tiff.extend_from_slice(&SHORT.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&ROTATE_90_CLOCKWISE.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

pub(super) fn image_source(path: &Path) -> Source {
    Source { path: path.to_path_buf(), kind: Kind::Image }
}

pub(super) fn audio_source(path: &Path) -> Source {
    Source { path: path.to_path_buf(), kind: Kind::Audio }
}

pub(super) fn pdf_source(path: &Path) -> Source {
    Source { path: path.to_path_buf(), kind: Kind::Pdf }
}

pub(super) fn run(op: Op, sources: &[Source]) -> Outcome {
    let mut progress = |fraction: f32| assert!((0.0..=1.0).contains(&fraction));
    let cancel = AtomicBool::new(false);
    ops::run(op, sources, &Outputs::new(None), &mut Task::new(&mut progress, &cancel))
}

pub(super) fn single_output(outcome: &Outcome) -> &Path {
    assert!(outcome.notice.ok, "{:?}", outcome.notice);
    assert_eq!(outcome.outputs.len(), 1, "{:?}", outcome.outputs);
    &outcome.outputs[0]
}

pub(super) fn output_names(outcome: &Outcome) -> Vec<String> {
    outcome.outputs.iter().map(|path| crate::shelf::names::file_name(path)).collect()
}

pub(super) fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.page_iter().collect()
}

pub(super) fn page_image(doc: &Document, page: ObjectId) -> &Stream {
    let resolve = |object| doc.dereference(object).unwrap().1;
    let page = doc.get_dictionary(page).unwrap();
    let resources = resolve(page.get(b"Resources").unwrap()).as_dict().unwrap();
    let xobjects = resolve(resources.get(b"XObject").unwrap()).as_dict().unwrap();
    resolve(xobjects.get(b"Im0").unwrap()).as_stream().unwrap()
}

pub(super) fn media_box(doc: &Document, page: ObjectId) -> Vec<f32> {
    let page = doc.get_dictionary(page).unwrap();
    let sides = page.get(b"MediaBox").unwrap().as_array().unwrap();
    sides.iter().map(|side| side.as_float().unwrap()).collect()
}

pub(super) fn page_rotation(doc: &Document, page: ObjectId) -> Option<i64> {
    doc.get_dictionary(page).unwrap().get(b"Rotate").and_then(Object::as_i64).ok()
}

pub(super) fn page_text(doc: &Document, page: ObjectId) -> String {
    String::from_utf8_lossy(&doc.get_page_content(page)).into_owned()
}

pub(super) fn count_typed(doc: &Document, type_name: &[u8]) -> usize {
    doc.objects.values().filter(|object| object.as_dict().is_ok_and(|dict| dict.has_type(type_name))).count()
}

pub(super) fn no_wake() -> Wake {
    Arc::new(|| {})
}

pub(super) fn wait_until(shelf: &mut Shelf, mut done: impl FnMut(&mut Shelf) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        shelf.sync();
        if done(shelf) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for the shelf worker");
        std::thread::sleep(Duration::from_millis(5));
    }
}
