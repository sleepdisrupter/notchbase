//! The Objective-C delegate that hears when the recording starts, when it finishes, and of any failure.

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{SCRecordingOutput, SCRecordingOutputDelegate, SCStream, SCStreamDelegate};

use super::shared::Shared;
use crate::record::job::Take;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "NotchbaseRecordingDelegate"]
    #[ivars = Arc<Shared>]
    pub(super) struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl SCRecordingOutputDelegate for Delegate {
        #[unsafe(method(recordingOutputDidStartRecording:))]
        fn did_start(&self, _output: &SCRecordingOutput) {
            self.ivars().started();
        }

        #[unsafe(method(recordingOutput:didFailWithError:))]
        fn did_fail(&self, _output: &SCRecordingOutput, error: &NSError) {
            self.ivars().finish(Err(describe(error)));
        }

        #[unsafe(method(recordingOutputDidFinishRecording:))]
        fn did_finish(&self, output: &SCRecordingOutput) {
            let seconds = unsafe { output.recordedDuration().seconds() };
            let seconds = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
            self.ivars().finish(Ok(Take { seconds, note: None }));
        }
    }

    unsafe impl SCStreamDelegate for Delegate {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            self.ivars().finish(Err(describe(error)));
        }
    }
);

impl Delegate {
    pub(super) fn new(shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) fn describe(error: &NSError) -> String {
    error.localizedDescription().to_string()
}
