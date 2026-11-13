//! Loads an asset's values asynchronously and waits for them.

use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_av_foundation::{AVAsynchronousKeyValueLoading, AVKeyValueStatus};
use objc2_foundation::{NSArray, NSError, NSString};

use super::wait;

pub(super) type Loadable = ProtocolObject<dyn AVAsynchronousKeyValueLoading>;
pub(super) type LoadError = Option<Retained<NSError>>;

pub(super) fn load(object: &Loadable, keys: &[&str]) -> Result<(), LoadError> {
    let keys: Vec<Retained<NSString>> = keys.iter().map(|key| NSString::from_str(key)).collect();
    let requested = NSArray::from_retained_slice(&keys);
    wait::reply(|sender| {
        let on_loaded = RcBlock::new(move || {
            let _ = sender.send(());
        });
        unsafe { object.loadValuesAsynchronouslyForKeys_completionHandler(&requested, Some(&on_loaded)) };
    })
    .ok_or(None)?;
    keys.iter().try_for_each(|key| loaded(object, key))
}

fn loaded(object: &Loadable, key: &NSString) -> Result<(), LoadError> {
    let mut error: LoadError = None;
    let status: AVKeyValueStatus = unsafe { msg_send![object, statusOfValueForKey: key, error: &mut error] };
    if status == AVKeyValueStatus::Loaded { Ok(()) } else { Err(error) }
}
