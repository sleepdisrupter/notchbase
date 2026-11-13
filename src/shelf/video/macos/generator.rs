//! An image generator set up for upright, scaled down frames close to the times asked for.

use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::AllocAnyThread as _;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVAsset, AVAssetImageGenerator, AVAssetImageGeneratorResult, NSValueAVFoundationExtensions as _,
};
use objc2_core_foundation::{CGFloat, CGSize};
use objc2_core_graphics::CGImage;
use objc2_core_media::CMTime;
use objc2_foundation::{NSArray, NSError, NSValue};

use super::super::FrameSpec;

const TIMESCALE: i32 = 600;

pub(super) struct Generated {
    pub(super) seconds: f64,
    pub(super) image: Option<Retained<CGImage>>,
    pub(super) error: Option<Retained<NSError>>,
}

pub(super) fn upright_scaled(asset: &AVAsset, spec: FrameSpec) -> Retained<AVAssetImageGenerator> {
    let generator = unsafe { AVAssetImageGenerator::initWithAsset(AVAssetImageGenerator::alloc(), asset) };
    let side = spec.max_side.max(1) as CGFloat;
    let tolerance = at_seconds(0.5 / spec.fps);
    unsafe {
        generator.setAppliesPreferredTrackTransform(true);
        generator.setMaximumSize(CGSize::new(side, side));
        generator.setRequestedTimeToleranceBefore(tolerance);
        generator.setRequestedTimeToleranceAfter(tolerance);
    }
    generator
}

pub(super) fn request(generator: &AVAssetImageGenerator, seconds: &[f64], sender: Sender<Generated>) {
    let times: Vec<Retained<NSValue>> =
        seconds.iter().map(|&at| unsafe { NSValue::valueWithCMTime(at_seconds(at)) }).collect();
    let times = NSArray::from_retained_slice(&times);
    let on_image = RcBlock::new(
        move |requested: CMTime,
              image: *mut CGImage,
              _actual: CMTime,
              result: AVAssetImageGeneratorResult,
              error: *mut NSError| {
            let succeeded = result == AVAssetImageGeneratorResult::Succeeded;
            let generated = Generated {
                seconds: unsafe { requested.seconds() },
                image: if succeeded { unsafe { Retained::retain(image) } } else { None },
                error: unsafe { Retained::retain(error) },
            };
            let _ = sender.send(generated);
        },
    );
    unsafe { generator.generateCGImagesAsynchronouslyForTimes_completionHandler(&times, RcBlock::as_ptr(&on_image)) };
}

pub(super) fn cancel(generator: &AVAssetImageGenerator) {
    unsafe { generator.cancelAllCGImageGeneration() };
}

fn at_seconds(seconds: f64) -> CMTime {
    unsafe { CMTime::with_seconds(seconds, TIMESCALE) }
}
