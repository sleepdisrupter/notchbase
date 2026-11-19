//! What's on a virtual display, captured with ScreenCaptureKit (straight to 4:2:0, pointer
//! included) and encoded in hardware with VideoToolbox, tuned for a live picture: real time, no
//! reordered pictures, and a key picture when one is asked for.

use std::ffi::{c_int, c_void};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    CMSampleBuffer, CMTime, CMVideoFormatDescriptionGetH264ParameterSetAtIndex, kCMTimeInvalid, kCMVideoCodecType_H264,
};
use objc2_core_video::CVImageBuffer;
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput, SCStreamOutputType,
};
use objc2_video_toolbox::{
    VTCompressionSession, VTEncodeInfoFlags, VTSessionSetProperty, kVTCompressionPropertyKey_AllowFrameReordering,
    kVTCompressionPropertyKey_AverageBitRate, kVTCompressionPropertyKey_ExpectedFrameRate,
    kVTCompressionPropertyKey_MaxKeyFrameInterval, kVTCompressionPropertyKey_ProfileLevel,
    kVTCompressionPropertyKey_RealTime, kVTEncodeFrameOptionKey_ForceKeyFrame, kVTProfileLevel_H264_Main_AutoLevel,
};

use crate::display::{Feed, Picture, h264};

/// 4:2:0 with the luma and chroma planes apart, video range: what the encoder takes as is.
const NV12: u32 = u32::from_be_bytes(*b"420v");
/// How long macOS may take to list a display just made, and to start capturing it.
const START_WAIT: Duration = Duration::from_secs(4);
/// A still screen still sends a picture this often, so the other side knows it's alive.
const REFRESH_EVERY: Duration = Duration::from_secs(1);
const PERMISSION: &str =
    "Allow notchbase to record the screen in System Settings, Privacy & Security, Screen & System Audio Recording";

/// Something from Apple's frameworks that's only touched under a lock or handed over whole.
struct Sendable<T>(T);
// SAFETY: as above.
unsafe impl<T> Send for Sendable<T> {}

/// Captures display `id` at `size` pixels into `feed` until dropped.
pub struct Capture {
    stream: Retained<SCStream>,
    _output: Retained<Output>,
    _queue: DispatchRetained<DispatchQueue>,
    stop: Arc<AtomicBool>,
    refresh: Option<JoinHandle<()>>,
}

// SAFETY: the stream is only told to stop, once, from whichever thread drops this.
unsafe impl Send for Capture {}

impl Capture {
    pub fn start(id: u32, size: [u32; 2], fps: u32, feed: Feed) -> Result<Self, String> {
        let display = find(id)?;
        let encoder = Encoder::new(size, fps, feed)?;
        // SAFETY: ScreenCaptureKit set up with its own calls; the output and its queue live as
        // long as the stream.
        unsafe {
            let config = SCStreamConfiguration::new();
            config.setWidth(size[0] as usize);
            config.setHeight(size[1] as usize);
            config.setMinimumFrameInterval(CMTime::new(1, fps as i32));
            config.setPixelFormat(NV12);
            config.setShowsCursor(true);
            config.setQueueDepth(5);
            config.setScalesToFit(true);
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display.0,
                &NSArray::new(),
            );
            let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None);
            let output = Output::new(encoder.clone());
            let queue = DispatchQueue::new("roam.display.capture", None);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|err| describe(&err))?;
            let (started, wait) = mpsc::channel();
            let block = RcBlock::new(move |error: *mut NSError| {
                let _ = started.send(error.as_ref().map(describe));
            });
            stream.startCaptureWithCompletionHandler(Some(&block));
            match wait.recv_timeout(START_WAIT) {
                Ok(None) => {}
                Ok(Some(why)) => return Err(why),
                Err(_) => return Err("The screen capture didn't start".into()),
            }
            let stop = Arc::new(AtomicBool::new(false));
            let refresh = {
                let (stop, encoder) = (stop.clone(), Sendable(encoder));
                std::thread::Builder::new()
                    .name("roam-display-refresh".into())
                    .spawn(move || {
                        let encoder = encoder;
                        while !stop.load(Ordering::Acquire) {
                            std::thread::sleep(Duration::from_millis(100));
                            encoder.0.refresh();
                        }
                    })
                    .ok()
            };
            Ok(Self { stream, _output: output, _queue: queue, stop, refresh })
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(refresh) = self.refresh.take() {
            let _ = refresh.join();
        }
        // SAFETY: stops the stream started above; nothing waits on it.
        unsafe { self.stream.stopCaptureWithCompletionHandler(None) };
    }
}

/// The display ScreenCaptureKit knows by `id`, once it lists it.
fn find(id: u32) -> Result<Sendable<Retained<SCDisplay>>, String> {
    let started = Instant::now();
    loop {
        let (found, wait) = mpsc::channel();
        let block = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: both come from ScreenCaptureKit, either may be null.
            let answer = unsafe {
                match content.as_ref() {
                    Some(content) => Ok(content.displays().iter().find(|d| d.displayID() == id).map(Sendable)),
                    None => Err(error.as_ref().map_or_else(|| PERMISSION.to_owned(), describe)),
                }
            };
            let _ = found.send(answer);
        });
        // SAFETY: the block answers once, on a queue of ScreenCaptureKit's.
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
        match wait.recv_timeout(START_WAIT) {
            Ok(Ok(Some(display))) => return Ok(display),
            Ok(Ok(None)) if started.elapsed() < START_WAIT => std::thread::sleep(Duration::from_millis(200)),
            Ok(Ok(None)) => return Err("The screen capture can't see the virtual display".into()),
            Ok(Err(why)) => return Err(why),
            Err(_) => return Err(PERMISSION.into()),
        }
    }
}

fn describe(error: &NSError) -> String {
    let words = error.localizedDescription().to_string();
    // The user declined, or hasn't been asked.
    if error.code() == -3801 { PERMISSION.to_owned() } else { words }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "RoamDisplayCaptureOutput"]
    #[ivars = Arc<Encoder>]
    struct Output;

    unsafe impl NSObjectProtocol for Output {}

    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            // Idle pictures (nothing changed) come without an image.
            // SAFETY: a sample ScreenCaptureKit hands over for the call.
            if let Some(image) = unsafe { sample.image_buffer() } {
                self.ivars().frame(Some(image));
            }
        }
    }
);

impl Output {
    fn new(encoder: Arc<Encoder>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(encoder);
        // SAFETY: NSObject's own initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// A VideoToolbox H.264 encoder that sends what it makes to a feed.
struct Encoder {
    session: CFRetained<VTCompressionSession>,
    /// The feed the output callback pushes to: a box freed once the session is gone.
    feed: *mut Feed,
    state: Mutex<State>,
}

struct State {
    /// The newest picture, kept to send again while the screen is still.
    last: Option<Sendable<CFRetained<CVImageBuffer>>>,
    sent: Instant,
    started: Instant,
}

// SAFETY: the session takes pictures under `state`'s lock, and the feed is thread-safe.
unsafe impl Send for Encoder {}
// SAFETY: as above.
unsafe impl Sync for Encoder {}

impl Encoder {
    fn new([width, height]: [u32; 2], fps: u32, feed: Feed) -> Result<Arc<Self>, String> {
        let feed = Box::into_raw(Box::new(feed));
        let mut session: *mut VTCompressionSession = std::ptr::null_mut();
        // SAFETY: the callback gets the feed box, which outlives the session (see Drop).
        let status = unsafe {
            VTCompressionSession::create(
                None,
                width as i32,
                height as i32,
                kCMVideoCodecType_H264,
                None,
                None,
                None,
                Some(encoded),
                feed.cast(),
                NonNull::from(&mut session),
            )
        };
        let Some(session) = NonNull::new(session).filter(|_| status == 0) else {
            // SAFETY: nothing else has the box.
            drop(unsafe { Box::from_raw(feed) });
            return Err(format!("The Mac's video encoder didn't start (error {status})"));
        };
        // SAFETY: a session just created, owned from here on.
        let session = unsafe { CFRetained::from_raw(session) };
        let bitrate = (f64::from(width) * f64::from(height) * f64::from(fps) * 0.12).clamp(8e6, 40e6) as i32;
        // SAFETY: documented keys with values of the types they take.
        unsafe {
            set(&session, kVTCompressionPropertyKey_RealTime, CFBoolean::new(true));
            set(&session, kVTCompressionPropertyKey_AllowFrameReordering, CFBoolean::new(false));
            set(&session, kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Main_AutoLevel);
            set(&session, kVTCompressionPropertyKey_AverageBitRate, &CFNumber::new_i32(bitrate));
            set(&session, kVTCompressionPropertyKey_ExpectedFrameRate, &CFNumber::new_i32(fps as i32));
            // A key picture now and then, so a lost one is never lost for long.
            set(&session, kVTCompressionPropertyKey_MaxKeyFrameInterval, &CFNumber::new_i32(fps as i32 * 4));
        }
        let now = Instant::now();
        Ok(Arc::new(Self {
            session,
            feed,
            state: Mutex::new(State { last: None, sent: now - REFRESH_EVERY, started: now }),
        }))
    }

    fn feed(&self) -> &Feed {
        // SAFETY: the box lives until this is dropped.
        unsafe { &*self.feed }
    }

    /// Encodes a new picture, or the last one again when `image` is `None`.
    fn frame(&self, image: Option<CFRetained<CVImageBuffer>>) {
        let Ok(mut state) = self.state.lock() else { return };
        if let Some(image) = image {
            state.last = Some(Sendable(image));
        }
        let feed = self.feed();
        if feed.stopped() {
            return;
        }
        let key = feed.key_wanted();
        if feed.busy() {
            if key {
                feed.ask_key();
            }
            return;
        }
        let Some(last) = &state.last else {
            if key {
                feed.ask_key();
            }
            return;
        };
        let at = state.started.elapsed().as_micros() as i64;
        let force: Option<CFRetained<CFDictionary<CFString, CFBoolean>>> = key.then(|| {
            // SAFETY: a documented key.
            CFDictionary::from_slices(&[unsafe { kVTEncodeFrameOptionKey_ForceKeyFrame }], &[CFBoolean::new(true)])
        });
        // SAFETY: a picture the session may keep until it's encoded; it calls back with the result.
        unsafe {
            let _ = self.session.encode_frame(
                &last.0,
                CMTime::new(at, 1_000_000),
                kCMTimeInvalid,
                force.as_deref().map(AsRef::as_ref),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        state.sent = Instant::now();
    }

    /// Sends the last picture again if it's been a while, or a key picture was asked for.
    fn refresh(&self) {
        let due = self.state.lock().is_ok_and(|state| {
            let quiet = state.sent.elapsed();
            quiet >= REFRESH_EVERY || (self.feed().key_pending() && quiet >= Duration::from_millis(100))
        });
        if due {
            self.frame(None);
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: finishes what's under way, then the session goes before the feed it calls into.
        unsafe {
            let _ = self.session.complete_frames(kCMTimeInvalid);
            self.session.invalidate();
            drop(Box::from_raw(self.feed));
        }
    }
}

unsafe fn set(session: &VTCompressionSession, key: &CFString, value: &CFType) {
    // SAFETY: the caller passes a key the session knows, with a value of its type.
    let _ = unsafe { VTSessionSetProperty(session, key, Some(value)) };
}

/// The encoder's callback: one encoded picture, length-prefixed, turned into Annex B.
unsafe extern "C-unwind" fn encoded(
    feed: *mut c_void,
    _frame: *mut c_void,
    status: i32,
    _flags: VTEncodeInfoFlags,
    sample: *mut CMSampleBuffer,
) {
    // SAFETY: `feed` is the box handed to the session; the sample lives for the call.
    let (feed, sample) = unsafe { (&*feed.cast::<Feed>(), sample.as_ref()) };
    let Some(sample) = sample.filter(|_| status == 0) else { return };
    // SAFETY: as above.
    if let Some(picture) = unsafe { annex_b(sample) } {
        feed.push(picture);
    }
}

unsafe fn annex_b(sample: &CMSampleBuffer) -> Option<Picture> {
    // SAFETY: plain CoreMedia reads of a sample the encoder made.
    unsafe {
        let block = sample.data_buffer()?;
        let length = block.data_length();
        let mut avcc = vec![0u8; length];
        if block.copy_data_bytes(0, length, NonNull::new(avcc.as_mut_ptr())?.cast()) != 0 {
            return None;
        }
        let mut header = 4usize;
        let mut sets: Vec<Vec<u8>> = Vec::new();
        if let Some(format) = sample.format_description() {
            let (mut count, mut size): (usize, c_int) = (0, 0);
            let (no_set, no_size) = (std::ptr::null_mut(), std::ptr::null_mut());
            if CMVideoFormatDescriptionGetH264ParameterSetAtIndex(&format, 0, no_set, no_size, &mut count, &mut size)
                == 0
            {
                if (1..=4).contains(&size) {
                    header = size as usize;
                }
                for index in 0..count {
                    let (mut set, mut bytes) = (std::ptr::null(), 0usize);
                    let read = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                        &format,
                        index,
                        &mut set,
                        &mut bytes,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    );
                    if read == 0 && !set.is_null() {
                        sets.push(std::slice::from_raw_parts(set, bytes).to_vec());
                    }
                }
            }
        }
        let plain = h264::from_avcc(&avcc, header, &[]);
        if !h264::is_key(&plain) {
            return Some(Picture { key: false, data: plain });
        }
        let before: Vec<&[u8]> = sets.iter().map(Vec::as_slice).collect();
        Some(Picture { key: true, data: h264::from_avcc(&avcc, header, &before) })
    }
}
