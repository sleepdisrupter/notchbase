//! Reads a drag's pasteboard, best first: files, a page, file promises, picture data, then a link.

use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::{ClassType as _, Message as _};
use objc2_app_kit::{
    NSFilePromiseReceiver, NSPasteboard, NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardTypeHTML,
    NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSPasteboardTypeURL,
};
use objc2_foundation::{NSArray, NSString, NSURL};

use super::super::incoming::Dropped;
use super::super::links;

const IMAGE_TYPES: [(&str, Option<&str>); 6] = [
    ("public.png", None),
    ("public.jpeg", None),
    ("com.compuserve.gif", None),
    ("org.webmproject.webp", None),
    // HEIC can't be told from its bytes, so it comes with a name.
    ("public.heic", Some("Image.heic")),
    ("public.tiff", None),
];

/// The only type on a tab dragged out of Firefox or Zen, so the window must accept it to be asked at all.
const GECKO_DRAG: &str = "org.mozilla.MozillaWildcard";

pub(super) struct Found {
    pub(super) dropped: Vec<Dropped>,
    pub(super) promises: Vec<Retained<NSFilePromiseReceiver>>,
}

pub(super) fn accepted_types() -> Retained<NSArray<NSPasteboardType>> {
    let mut types: Vec<Retained<NSPasteboardType>> = unsafe {
        [
            NSPasteboardTypeFileURL,
            NSPasteboardTypeURL,
            NSPasteboardTypeString,
            NSPasteboardTypePNG,
            NSPasteboardTypeTIFF,
        ]
    }
    .into_iter()
    .map(|kind| kind.retain())
    .collect();
    types.extend(IMAGE_TYPES.iter().map(|(kind, _)| NSString::from_str(kind)));
    types.push(NSString::from_str(GECKO_DRAG));
    types.extend(NSFilePromiseReceiver::readableDraggedTypes().iter());
    NSArray::from_retained_slice(&types)
}

/// How many things a drag holds, for the hover. A link or a picture counts as one.
pub(super) fn count(pasteboard: &NSPasteboard) -> usize {
    let files = file_paths(pasteboard).len();
    if files > 0 {
        return files;
    }
    if offered_page(pasteboard).is_some() {
        return 1;
    }
    let promised = promises(pasteboard).len();
    if promised > 0 {
        return promised;
    }
    usize::from(has_image(pasteboard) || link(pasteboard).is_some())
}

/// A page link, when it's all the drag holds or the app prefers it to the picture beside it.
pub(super) fn offered_page(pasteboard: &NSPasteboard) -> Option<String> {
    let page = page_link(pasteboard).filter(|link| !links::is_inline_or_file(link))?;
    let alone = || promises(pasteboard).is_empty() && !has_image(pasteboard) && image_link(pasteboard).is_none();
    (super::super::prefers_page(&page) || alone()).then_some(page)
}

pub(super) fn read(pasteboard: &NSPasteboard) -> Found {
    let files = file_paths(pasteboard);
    if !files.is_empty() {
        return Found { dropped: files.into_iter().map(Dropped::File).collect(), promises: Vec::new() };
    }
    if let Some(page) = offered_page(pasteboard) {
        return Found { dropped: vec![Dropped::Page(page)], promises: Vec::new() };
    }
    let promises = promises(pasteboard);
    if !promises.is_empty() {
        return Found { dropped: Vec::new(), promises };
    }
    let dropped = image(pasteboard).or_else(|| link(pasteboard).map(Dropped::Link));
    Found { dropped: dropped.into_iter().collect(), promises: Vec::new() }
}

fn file_paths(pasteboard: &NSPasteboard) -> Vec<PathBuf> {
    let kind = unsafe { NSPasteboardTypeFileURL };
    let Some(items) = pasteboard.pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.stringForType(kind))
        .filter_map(|url| NSURL::URLWithString(&url)?.filePathURL()?.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

fn promises(pasteboard: &NSPasteboard) -> Vec<Retained<NSFilePromiseReceiver>> {
    let classes = NSArray::from_slice(&[NSFilePromiseReceiver::class()]);
    let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }) else {
        return Vec::new();
    };
    objects.iter().filter_map(|object| object.downcast::<NSFilePromiseReceiver>().ok()).collect()
}

fn has_image(pasteboard: &NSPasteboard) -> bool {
    let types: Vec<Retained<NSString>> = IMAGE_TYPES.iter().map(|(kind, _)| NSString::from_str(kind)).collect();
    pasteboard.availableTypeFromArray(&NSArray::from_retained_slice(&types)).is_some()
}

fn image(pasteboard: &NSPasteboard) -> Option<Dropped> {
    IMAGE_TYPES.iter().find_map(|(kind, name)| {
        let data = pasteboard.dataForType(&NSString::from_str(kind))?;
        Some(Dropped::Data { name: name.map(str::to_owned), bytes: data.to_vec() })
    })
}

fn link(pasteboard: &NSPasteboard) -> Option<String> {
    image_link(pasteboard).or_else(|| page_link(pasteboard))
}

fn image_link(pasteboard: &NSPasteboard) -> Option<String> {
    let html = pasteboard.stringForType(unsafe { NSPasteboardTypeHTML }).map(|html| html.to_string());
    html.and_then(|html| links::image_source(&html))
}

fn page_link(pasteboard: &NSPasteboard) -> Option<String> {
    let text = |kind: &NSPasteboardType| pasteboard.stringForType(kind).map(|text| text.to_string());
    let (url, plain) = unsafe { (NSPasteboardTypeURL, NSPasteboardTypeString) };
    text(url)
        .and_then(|url| links::first_link(&url))
        .or_else(|| text(plain).filter(|plain| links::is_link(plain)).map(|plain| plain.trim().to_owned()))
}
