//! Builds the ScreenCaptureKit stream: a filter on the recorded display that leaves out notchbase's own windows,
//! the capture settings, and a QuickTime movie output with H.264 video.

use std::sync::Arc;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_av_foundation::{AVFileTypeQuickTimeMovie, AVVideoCodecTypeH264};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_media::CMTime;
use objc2_foundation::{NSArray, NSString, NSURL};
use objc2_screen_capture_kit::{
    SCCaptureResolutionType, SCContentFilter, SCDisplay, SCRecordingOutput, SCRecordingOutputConfiguration,
    SCRunningApplication, SCShareableContent, SCStream, SCStreamConfiguration,
};

use super::delegate::{Delegate, describe};
use super::shared::{Capture, Shared};
use crate::record::job::Job;

const FPS: u32 = 60;
const QUEUE_DEPTH: isize = 6;
const SAMPLE_RATE: isize = 48_000;
const CHANNELS: isize = 2;
const DISPLAY_GONE: &str = "That display is no longer connected";

pub(super) fn build(content: &SCShareableContent, job: &Job, shared: &Arc<Shared>) -> Result<Capture, String> {
    let wanted = job.region.display.id;
    let display = unsafe { content.displays() }
        .iter()
        .find(|display| unsafe { display.displayID() } == wanted)
        .ok_or(DISPLAY_GONE)?;
    let me = std::process::id() as i32;
    let ours: Vec<Retained<SCRunningApplication>> =
        unsafe { content.applications() }.iter().filter(|app| unsafe { app.processID() } == me).collect();
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            &display,
            &NSArray::from_retained_slice(&ours),
            &NSArray::new(),
        )
    };
    let delegate = Delegate::new(shared.clone());
    let stream = unsafe {
        SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            &filter,
            &configuration(job, &display),
            Some(ProtocolObject::from_ref(&*delegate)),
        )
    };
    let output = unsafe {
        SCRecordingOutput::initWithConfiguration_delegate(
            SCRecordingOutput::alloc(),
            &output_configuration(job),
            ProtocolObject::from_ref(&*delegate),
        )
    };
    unsafe { stream.addRecordingOutput_error(&output) }.map_err(|error| describe(&error))?;
    Ok(Capture { stream, _output: output, _delegate: delegate })
}

/// Output at the area's size in pixels, cropped with a source rectangle in points for a partial area.
fn configuration(job: &Job, display: &SCDisplay) -> Retained<SCStreamConfiguration> {
    let region = job.region;
    let points = unsafe { display.width() }.max(1) as f64;
    let scale = f64::from(region.display.size[0]) / points;
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { region.display.scale.max(1.0) };
    let [x, y] = region.origin.map(|side| f64::from(side) / scale);
    let [w, h] = region.size.map(|side| f64::from(side) / scale);
    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setWidth(region.size[0] as usize);
        config.setHeight(region.size[1] as usize);
        if !region.is_whole() {
            config.setSourceRect(CGRect::new(CGPoint::new(x, y), CGSize::new(w, h)));
        }
        config.setScalesToFit(false);
        config.setShowsCursor(true);
        config.setMinimumFrameInterval(CMTime::new(1, job.max_fps.unwrap_or(FPS).clamp(1, 120) as i32));
        config.setQueueDepth(QUEUE_DEPTH);
        config.setCaptureResolution(SCCaptureResolutionType::Best);
        config.setCapturesAudio(job.desktop);
        config.setExcludesCurrentProcessAudio(true);
        config.setSampleRate(SAMPLE_RATE);
        config.setChannelCount(CHANNELS);
        config.setCaptureMicrophone(job.microphone);
        if job.microphone
            && let Some(id) = &job.microphone_id
        {
            config.setMicrophoneCaptureDeviceID(Some(&NSString::from_str(id)));
        }
    }
    config
}

fn output_configuration(job: &Job) -> Retained<SCRecordingOutputConfiguration> {
    let config = unsafe { SCRecordingOutputConfiguration::new() };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&job.path.to_string_lossy()));
    unsafe {
        config.setOutputURL(&url);
        if let Some(movie) = AVFileTypeQuickTimeMovie {
            config.setOutputFileType(movie);
        }
        if let Some(h264) = AVVideoCodecTypeH264 {
            config.setVideoCodecType(h264);
        }
    }
    config
}
