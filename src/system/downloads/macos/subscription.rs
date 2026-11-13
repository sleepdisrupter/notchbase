//! Subscribes to the NSProgress objects published for files in the Downloads folder.

use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, PoisonError};

use block2::{DynBlock, RcBlock};
use objc2::ffi::objc_autorelease;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSProgress, NSProgressFileOperationKindDownloading, NSProgressFileOperationKindKey, NSProgressFileURLKey, NSString,
    NSURL,
};

use super::published::{self, Published};
use crate::system::downloads::scan::Listing;

type Shown = Arc<Mutex<Vec<Retained<NSProgress>>>>;

pub(in crate::system::downloads) struct Subscription {
    subscriber: Retained<AnyObject>,
    shown: Shown,
}

impl Subscription {
    pub(in crate::system::downloads) fn start(folder: &Path) -> Self {
        let shown = Shown::default();
        let keeping = shown.clone();
        let on_published = RcBlock::new(move |progress: NonNull<NSProgress>| -> *mut DynBlock<dyn Fn()> {
            let Some(progress) = (unsafe { Retained::retain(progress.as_ptr()) }) else { return std::ptr::null_mut() };
            let keeping = keeping.clone();
            keeping.lock().unwrap_or_else(PoisonError::into_inner).push(progress.clone());
            let on_unpublished = RcBlock::new(move || {
                let mut shown = keeping.lock().unwrap_or_else(PoisonError::into_inner);
                shown.retain(|kept| Retained::as_ptr(kept) != Retained::as_ptr(&progress));
            });
            unsafe { objc_autorelease(RcBlock::into_raw(on_unpublished).cast()) }.cast()
        });
        let folder = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&folder.to_string_lossy()), true);
        let subscriber = unsafe {
            NSProgress::addSubscriberForFileURL_withPublishingHandler(&folder, RcBlock::as_ptr(&on_published))
        };
        Self { subscriber, shown }
    }

    pub(in crate::system::downloads) fn fill(&self, listing: &mut Listing) {
        let shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let published: Vec<Published> =
            autoreleasepool(|_| shown.iter().filter_map(|progress| read(progress)).collect());
        published::fill(listing, &published);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        unsafe { NSProgress::removeSubscriber(&self.subscriber) };
    }
}

fn read(progress: &NSProgress) -> Option<Published> {
    let info = progress.userInfo();
    let kind = info.objectForKey(unsafe { NSProgressFileOperationKindKey })?.downcast::<NSString>().ok()?;
    let file = info.objectForKey(unsafe { NSProgressFileURLKey })?.downcast::<NSURL>().ok()?;
    let name = file.lastPathComponent().filter(|_| &*kind == unsafe { NSProgressFileOperationKindDownloading })?;
    let total = u64::try_from(progress.totalUnitCount()).ok().filter(|total| *total > 0);
    Some(Published { name: name.to_string(), total })
}
