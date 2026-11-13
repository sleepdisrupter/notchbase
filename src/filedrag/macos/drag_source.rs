//! The drag's source object. Other apps get a copy, the notch itself takes nothing, and it notes when the drag ends.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSDragOperation, NSDraggingContext, NSDraggingSession, NSDraggingSource};
use objc2_foundation::NSPoint;

static SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static SOURCE_OF_LATEST_SESSION: RefCell<Option<Retained<DragSource>>> =
        const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NotchbaseFileDragSource"]
    pub(super) struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn allowed_operations(&self, _session: &NSDraggingSession, context: NSDraggingContext) -> NSDragOperation {
            copy_to_other_apps_only(context)
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn session_ended(&self, _session: &NSDraggingSession, _point: NSPoint, _operation: NSDragOperation) {
            SESSION_ACTIVE.store(false, Ordering::Relaxed);
        }
    }
);

impl DragSource {
    pub(super) fn for_new_session(mtm: MainThreadMarker) -> Retained<Self> {
        let unset = Self::alloc(mtm).set_ivars(());
        let source: Retained<Self> = unsafe { msg_send![super(unset), init] };
        SOURCE_OF_LATEST_SESSION.set(Some(source.clone()));
        SESSION_ACTIVE.store(true, Ordering::Relaxed);
        source
    }
}

pub(super) fn session_active() -> bool {
    SESSION_ACTIVE.load(Ordering::Relaxed)
}

fn copy_to_other_apps_only(context: NSDraggingContext) -> NSDragOperation {
    if context == NSDraggingContext::OutsideApplication { NSDragOperation::Copy } else { NSDragOperation::None }
}
