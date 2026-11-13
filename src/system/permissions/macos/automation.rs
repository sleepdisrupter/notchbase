//! Apple Events access to Music and Spotify, checked without a prompt unless asked.

use std::ffi::c_void;

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use super::super::Standing;

pub const PLAYERS: [&str; 2] = ["com.apple.Music", "com.spotify.client"];
const BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
const ANY: u32 = u32::from_be_bytes(*b"****");
const ALLOWED: i32 = 0;
const NOT_PERMITTED: i32 = -1743;
const WOULD_ASK: i32 = -1744;

#[repr(C)]
struct AeDesc {
    kind: u32,
    data: *mut c_void,
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn AECreateDesc(kind: u32, data: *const c_void, size: isize, result: *mut AeDesc) -> i16;
    fn AEDisposeDesc(desc: *mut AeDesc) -> i16;
    fn AEDeterminePermissionToAutomateTarget(target: *const AeDesc, class: u32, id: u32, ask: u8) -> i32;
}

pub fn running(bundle: &str) -> bool {
    autoreleasepool(|_| {
        !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle)).is_empty()
    })
}

pub fn standing(bundle: &str) -> Option<Standing> {
    determine(bundle, false)
}

pub fn ask(bundle: &str) -> Option<Standing> {
    determine(bundle, true)
}

pub fn combined(standings: impl IntoIterator<Item = Standing>) -> Option<Standing> {
    standings.into_iter().fold(None, |all, one| {
        Some(match (all, one) {
            (Some(Standing::Denied), _) | (_, Standing::Denied) => Standing::Denied,
            (Some(Standing::Undecided), _) | (_, Standing::Undecided) => Standing::Undecided,
            _ => Standing::Granted,
        })
    })
}

fn determine(bundle: &str, ask: bool) -> Option<Standing> {
    let mut target = AeDesc { kind: 0, data: std::ptr::null_mut() };
    let created = unsafe { AECreateDesc(BUNDLE_ID, bundle.as_ptr().cast(), bundle.len() as isize, &mut target) };
    if created != 0 {
        return None;
    }
    let status = unsafe { AEDeterminePermissionToAutomateTarget(&target, ANY, ANY, u8::from(ask)) };
    unsafe { AEDisposeDesc(&mut target) };
    match status {
        ALLOWED => Some(Standing::Granted),
        NOT_PERMITTED => Some(Standing::Denied),
        WOULD_ASK => Some(Standing::Undecided),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_wins_then_undecided() {
        use Standing::{Denied, Granted, Undecided};
        assert_eq!(combined([]), None, "with no player open there is nothing to ask");
        assert_eq!(combined([Granted, Granted]), Some(Granted));
        assert_eq!(combined([Granted, Undecided]), Some(Undecided));
        assert_eq!(combined([Undecided, Denied]), Some(Denied));
        assert_eq!(combined([Denied, Granted]), Some(Denied));
    }

    #[test]
    fn event_codes_are_apples() {
        assert_eq!(BUNDLE_ID, 0x62756e64);
        assert_eq!(ANY, 0x2a2a2a2a);
    }
}
