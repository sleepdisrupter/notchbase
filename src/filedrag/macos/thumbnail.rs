//! Thumbnails from Quick Look, waiting at most 2 seconds, with the Finder icon when there's none.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AllocAnyThread as _;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_foundation::{CGFloat, CGSize};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSError, NSString};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes, QLThumbnailGenerator,
    QLThumbnailRepresentation,
};

use super::icon::finder_icon;
use super::paths::{file_url, ns_string_path};
use super::raster::rasterize_to_fit;

const QUICK_LOOK_TIMEOUT: Duration = Duration::from_secs(2);

pub(in crate::filedrag) fn thumbnail(path: &Path, max_px: u32) -> Option<egui::ColorImage> {
    autoreleasepool(|_| {
        let path = ns_string_path(path)?;
        let image = quick_look_image(&path, max_px).or_else(|| finder_icon(&path, max_px))?;
        rasterize_to_fit(&image, max_px)
    })
}

fn quick_look_image(path: &NSString, max_px: u32) -> Option<Retained<CGImage>> {
    let side = max_px as CGFloat;
    let request = unsafe {
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            QLThumbnailGenerationRequest::alloc(),
            &file_url(path),
            CGSize::new(side, side),
            1.0,
            QLThumbnailGenerationRequestRepresentationTypes::All,
        )
    };
    let (sender, receiver) = mpsc::channel();
    let on_done = RcBlock::new(move |representation: *mut QLThumbnailRepresentation, _error: *mut NSError| {
        let image =
            unsafe { Retained::retain(representation) }.map(|representation| unsafe { representation.CGImage() });
        let _ = sender.send(image);
    });
    let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };
    unsafe { generator.generateBestRepresentationForRequest_completionHandler(&request, &on_done) };
    match receiver.recv_timeout(QUICK_LOOK_TIMEOUT) {
        Ok(image) => image,
        Err(_) => {
            unsafe { generator.cancelRequest(&request) };
            None
        }
    }
}
