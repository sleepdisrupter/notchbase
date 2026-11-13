//! AirDrop through the system's sharing service. It starts on the main thread and reports back through a delegate.

use std::cell::RefCell;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSSharingService, NSSharingServiceDelegate, NSSharingServiceNameSendViaAirDrop};
use objc2_foundation::{NSArray, NSCocoaErrorDomain, NSError, NSString, NSURL, NSUserCancelledError};

use super::{Ended, Finished};
use crate::poll::Wake;

const NOT_ON_MAIN: &str = "AirDrop has to start on the main thread";
const UNAVAILABLE: &str = "AirDrop isn't available on this Mac";
const REFUSED: &str = "AirDrop can't send these files";

struct Watch {
    id: u64,
    wake: Wake,
}

/// The sharing service holds its delegate weakly, so both stay here until the outcome is taken.
struct Flying {
    id: u64,
    _service: Retained<NSSharingService>,
    _delegate: Retained<Delegate>,
}

thread_local! {
    static FLYING: RefCell<Vec<Flying>> = const { RefCell::new(Vec::new()) };
    static FINISHED: RefCell<Vec<Finished>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NotchbaseAirDropDelegate"]
    #[ivars = Watch]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSSharingServiceDelegate for Delegate {
        #[unsafe(method(sharingService:didShareItems:))]
        fn did_share(&self, _service: &NSSharingService, _items: &NSArray) {
            self.finish(Ended::Sent);
        }

        #[unsafe(method(sharingService:didFailToShareItems:error:))]
        fn did_fail(&self, _service: &NSSharingService, _items: &NSArray, error: &NSError) {
            self.finish(ended_by(error));
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, watch: Watch) -> Retained<Self> {
        let unset = Self::alloc(mtm).set_ivars(watch);
        unsafe { msg_send![super(unset), init] }
    }

    fn finish(&self, ended: Ended) {
        let Watch { id, wake } = self.ivars();
        let flying = FLYING.with_borrow(|flying| flying.iter().any(|sending| sending.id == *id));
        let told = FINISHED.with_borrow(|finished| finished.iter().any(|done| done.id == *id));
        if flying && !told {
            FINISHED.with_borrow_mut(|finished| finished.push(Finished { id: *id, ended }));
            wake();
        }
    }
}

pub(super) fn send(id: u64, paths: &[PathBuf], wake: Wake) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or(NOT_ON_MAIN)?;
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .filter_map(|path| path.to_str())
        .map(|path| NSURL::fileURLWithPath(&NSString::from_str(path)))
        .collect();
    if urls.len() != paths.len() {
        return Err(REFUSED.into());
    }
    let name = unsafe { NSSharingServiceNameSendViaAirDrop };
    let service = NSSharingService::sharingServiceNamed(name).ok_or(UNAVAILABLE)?;
    let urls = NSArray::from_retained_slice(&urls);
    let items: &NSArray = unsafe { urls.cast_unchecked() };
    if !unsafe { service.canPerformWithItems(Some(items)) } {
        return Err(REFUSED.into());
    }
    let delegate = Delegate::new(mtm, Watch { id, wake });
    service.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    FLYING.with_borrow_mut(|flying| flying.push(Flying { id, _service: service.clone(), _delegate: delegate }));
    NSApplication::sharedApplication(mtm).activate();
    unsafe { service.performWithItems(items) };
    Ok(())
}

pub(super) fn take_finished() -> Vec<Finished> {
    let finished = FINISHED.with_borrow_mut(std::mem::take);
    if !finished.is_empty() {
        FLYING.with_borrow_mut(|flying| flying.retain(|sending| finished.iter().all(|done| done.id != sending.id)));
    }
    finished
}

fn ended_by(error: &NSError) -> Ended {
    let cocoa = unsafe { NSCocoaErrorDomain };
    if error.code() == NSUserCancelledError && *error.domain() == *cocoa {
        return Ended::Cancelled;
    }
    let reason = error.localizedDescription().to_string();
    eprintln!("notchbase: AirDrop didn't send: {reason}");
    Ended::Failed(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(domain: &NSString, code: isize) -> Retained<NSError> {
        unsafe { NSError::errorWithDomain_code_userInfo(domain, code, None) }
    }

    #[test]
    fn closing_the_window_cancels() {
        let cocoa = unsafe { NSCocoaErrorDomain };
        assert_eq!(ended_by(&error(cocoa, NSUserCancelledError)), Ended::Cancelled);
        assert!(matches!(ended_by(&error(cocoa, 4)), Ended::Failed(_)));
        let elsewhere = NSString::from_str("SFOperationErrorDomain");
        assert!(matches!(ended_by(&error(&elsewhere, NSUserCancelledError)), Ended::Failed(_)));
    }
}
