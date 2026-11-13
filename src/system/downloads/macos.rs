//! On macOS, the total Safari writes into a .download bundle, and the progress other apps publish for the folder.

use std::path::Path;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::NSString;

mod published;
mod subscription;

pub(super) use subscription::Subscription;

const MANIFEST: &str = "Info.plist";
const TOTAL: &str = "DownloadEntryProgressTotalToLoad";

pub(super) fn bundle_total(bundle: &Path) -> Option<u64> {
    autoreleasepool(|_| {
        let file = NSString::from_str(&bundle.join(MANIFEST).to_string_lossy());
        let manifest: Option<Retained<AnyObject>> =
            unsafe { msg_send![class!(NSDictionary), dictionaryWithContentsOfFile: &*file] };
        let total: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*manifest?, objectForKey: &*NSString::from_str(TOTAL)] };
        let total = total?;
        let numeric: bool = unsafe { msg_send![&*total, respondsToSelector: sel!(longLongValue)] };
        if !numeric {
            return None;
        }
        let bytes: i64 = unsafe { msg_send![&*total, longLongValue] };
        u64::try_from(bytes).ok().filter(|bytes| *bytes > 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safari_bundle_gives_the_total() {
        let bundle = std::env::temp_dir().join(format!("notchbase-bundle-{}.download", std::process::id()));
        std::fs::create_dir_all(&bundle).unwrap();
        assert_eq!(bundle_total(&bundle), None, "a bundle without a manifest has no total");
        let manifest = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>{TOTAL}</key><integer>4096</integer></dict></plist>"
        );
        std::fs::write(bundle.join(MANIFEST), manifest).unwrap();
        assert_eq!(bundle_total(&bundle), Some(4096));
        let _ = std::fs::remove_dir_all(&bundle);
    }
}
