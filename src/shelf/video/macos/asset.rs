//! Opening a file as an AVURLAsset and loading its tracks and duration.

use std::path::Path;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_av_foundation::{AVAsset, AVAssetTrack, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo, AVURLAsset};
use objc2_core_media::{CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError, NSURL};

use super::geometry::displayed_dims;
use super::loading::{LoadError, load};
use super::wait;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Media {
    Video,
    Audio,
}

pub(super) fn open(path: &Path) -> Option<Retained<AVURLAsset>> {
    let url = NSURL::from_file_path(path)?;
    Some(unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) })
}

pub(super) fn readable(asset: &AVAsset) -> Result<(), LoadError> {
    load(ProtocolObject::from_ref(asset), &["tracks"])
}

pub(super) fn duration_seconds(asset: &AVAsset) -> Option<f64> {
    load(ProtocolObject::from_ref(asset), &["duration"]).ok()?;
    finite_seconds(unsafe { asset.duration() })
}

pub(super) fn tracks(asset: &AVAsset, media: Media) -> Result<Retained<NSArray<AVAssetTrack>>, LoadError> {
    let media_type = media_type(media).ok_or(None)?;
    wait::reply(|sender| {
        let on_loaded = RcBlock::new(move |tracks: *mut NSArray<AVAssetTrack>, error: *mut NSError| {
            let loaded = unsafe { Retained::retain(tracks) }.ok_or_else(|| unsafe { Retained::retain(error) });
            let _ = sender.send(loaded);
        });
        unsafe { asset.loadTracksWithMediaType_completionHandler(media_type, &on_loaded) };
    })
    .unwrap_or(Err(None))
}

pub(super) fn first_video_dims(asset: &AVAsset) -> Option<[u32; 2]> {
    let track = tracks(asset, Media::Video).ok()?.firstObject()?;
    load(ProtocolObject::from_ref(&*track), &["naturalSize", "preferredTransform"]).ok()?;
    let (size, transform) = unsafe { (track.naturalSize(), track.preferredTransform()) };
    displayed_dims(size.width, size.height, [transform.a, transform.b, transform.c, transform.d])
}

fn media_type(media: Media) -> Option<&'static AVMediaType> {
    match media {
        Media::Video => unsafe { AVMediaTypeVideo },
        Media::Audio => unsafe { AVMediaTypeAudio },
    }
}

fn finite_seconds(time: CMTime) -> Option<f64> {
    let flags = time.flags;
    let numeric = flags.contains(CMTimeFlags::Valid) && !flags.intersects(CMTimeFlags::ImpliedValueFlagsMask);
    let seconds = unsafe { time.seconds() };
    (numeric && seconds.is_finite() && seconds > 0.0).then_some(seconds)
}
