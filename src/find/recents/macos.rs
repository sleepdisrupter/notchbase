//! Asks Spotlight for the files used lately under home.

use std::collections::HashSet;
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null;

use objc2::Message;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDate, NSString};

use crate::find::context::Stamp;
use crate::find::usage::Used;

type Ref = *const c_void;

const BLOCK_UNTIL_GATHERED: usize = 1;
const NO_SCOPE_OPTIONS: u32 = 0;
const NEWER_THAN: &str = "kMDItemLastUsedDate >= $time.iso(";
const END_OF_DATE: &str = ")";

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    static kMDItemLastUsedDate: Ref;
    static kMDItemPath: Ref;
    static kMDQueryScopeHome: Ref;
    fn MDQueryCreate(allocator: Ref, asking: Ref, values: Ref, sorting: Ref) -> *mut c_void;
    fn MDQuerySetSearchScope(query: Ref, scopes: Ref, options: u32);
    fn MDQuerySetMaxCount(query: Ref, most: isize);
    fn MDQueryExecute(query: Ref, options: usize) -> u8;
    fn MDQueryGetResultCount(query: Ref) -> isize;
    fn MDQueryGetAttributeValueOfResultAtIndex(query: Ref, name: Ref, index: isize) -> Ref;
    fn MDQueryGetResultAtIndex(query: Ref, index: isize) -> Ref;
    fn MDItemCopyAttribute(item: Ref, name: Ref) -> *mut c_void;
}

pub fn recently_used(since: Stamp, most: usize) -> Vec<Used> {
    if most == 0 {
        return Vec::new();
    }
    let used = autoreleasepool(|_| gathered(since, most)).unwrap_or_default();
    newest_first(used, since, most)
}

fn gathered(since: Stamp, most: usize) -> Option<Vec<Used>> {
    let (path, last_used, home) =
        unsafe { (named(kMDItemPath)?, named(kMDItemLastUsedDate)?, named(kMDQueryScopeHome)?) };
    let asking = NSString::from_str(&asking(since)?);
    let values = NSArray::from_retained_slice(std::slice::from_ref(&last_used));
    let scopes = NSArray::from_retained_slice(&[home]);
    let query = unsafe { MDQueryCreate(null(), raw(&asking), raw(&values), null()) };
    let query = unsafe { Retained::from_raw(query.cast::<AnyObject>()) }?;
    let ran = unsafe {
        MDQuerySetSearchScope(raw(&query), raw(&scopes), NO_SCOPE_OPTIONS);
        MDQuerySetMaxCount(raw(&query), isize::try_from(most).unwrap_or(isize::MAX));
        MDQueryExecute(raw(&query), BLOCK_UNTIL_GATHERED) != 0
    };
    let results = if ran { unsafe { MDQueryGetResultCount(raw(&query)) }.max(0) } else { 0 };
    let value = |name: &Retained<NSString>, index: isize| unsafe {
        MDQueryGetAttributeValueOfResultAtIndex(raw(&query), raw(name), index).cast::<AnyObject>().as_ref()
    };
    let place = |index: isize| unsafe {
        let item = MDQueryGetResultAtIndex(raw(&query), index);
        Retained::from_raw(MDItemCopyAttribute(item, raw(&path)).cast::<AnyObject>()).filter(|_| !item.is_null())
    };
    let used = |index: isize| place(index).and_then(|path| result(Some(&path), value(&last_used, index)));
    Some((0..results).filter_map(used).collect())
}

fn result(path: Option<&AnyObject>, last_used: Option<&AnyObject>) -> Option<Used> {
    let path = path?.downcast_ref::<NSString>()?.to_string();
    let last = last_used?.downcast_ref::<NSDate>()?.timeIntervalSince1970();
    Some(Used { path: PathBuf::from(path), last: stamp(last)? })
}

unsafe fn named(constant: Ref) -> Option<Retained<NSString>> {
    unsafe { Retained::retain(constant.cast::<NSString>().cast_mut()) }
}

fn raw<T: Message>(object: &Retained<T>) -> Ref {
    Retained::as_ptr(object).cast()
}

fn asking(since: Stamp) -> Option<String> {
    let since = jiff::Timestamp::from_second(i64::from(since)).ok()?;
    Some(format!("{NEWER_THAN}{since}{END_OF_DATE}"))
}

fn newest_first(mut used: Vec<Used>, since: Stamp, most: usize) -> Vec<Used> {
    used.retain(|used| used.last >= since && used.path.is_absolute());
    used.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.path.cmp(&b.path)));
    let mut listed = HashSet::new();
    used.retain(|used| listed.insert(used.path.clone()));
    used.truncate(most);
    used
}

fn stamp(seconds: f64) -> Option<Stamp> {
    (seconds.is_finite() && seconds >= 1.0 && seconds <= f64::from(Stamp::MAX)).then_some(seconds as Stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::{DAY, now};
    use crate::find::weights::{SEED_MOST, SEED_WITHIN};

    const MONDAY_NOON: Stamp = 1_704_110_400;

    fn used(path: &str, last: Stamp) -> Used {
        Used { path: PathBuf::from(path), last }
    }

    #[test]
    fn query_names_last_used_date() {
        assert_eq!(asking(MONDAY_NOON).as_deref(), Some("kMDItemLastUsedDate >= $time.iso(2024-01-01T12:00:00Z)"));
        assert_eq!(asking(0).as_deref(), Some("kMDItemLastUsedDate >= $time.iso(1970-01-01T00:00:00Z)"));
    }

    #[test]
    fn used_newest_first_once() {
        let gathered = vec![
            used("/Users/john/old.txt", MONDAY_NOON - 50),
            used("/Users/john/b.txt", MONDAY_NOON + 10),
            used("relative.txt", MONDAY_NOON + 99),
            used("/Users/john/a.txt", MONDAY_NOON + 10),
            used("/Users/john/c.txt", MONDAY_NOON + 5),
            used("/Users/john/c.txt", MONDAY_NOON + 30),
        ];
        let kept = newest_first(gathered.clone(), MONDAY_NOON, 10);
        let names: Vec<&str> = kept.iter().filter_map(|used| used.path.to_str()).collect();
        assert_eq!(names, ["/Users/john/c.txt", "/Users/john/a.txt", "/Users/john/b.txt"]);
        assert_eq!(newest_first(gathered, MONDAY_NOON, 1), [used("/Users/john/c.txt", MONDAY_NOON + 30)]);
    }

    #[test]
    fn spotlight_date_to_stamp() {
        assert_eq!(stamp(1_704_110_400.9), Some(1_704_110_400));
        assert_eq!(
            (stamp(-5.0), stamp(0.0), stamp(f64::NAN), stamp(f64::INFINITY), stamp(1e12)),
            (None, None, None, None, None)
        );
    }

    #[test]
    fn metadata_names_link() {
        let name = |constant: Ref| unsafe { named(constant) }.map(|name| name.to_string());
        let (path, last_used, home) = unsafe { (kMDItemPath, kMDItemLastUsedDate, kMDQueryScopeHome) };
        assert_eq!(name(path).as_deref(), Some("kMDItemPath"));
        assert_eq!(name(last_used).as_deref(), Some("kMDItemLastUsedDate"));
        assert_eq!(name(home).as_deref(), Some("kMDQueryScopeHome"));
        assert_eq!(raw(&NSString::from_str("a")), Retained::as_ptr(&NSString::from_str("a")).cast::<c_void>());
    }

    #[test]
    fn result_is_path_and_date() {
        let path = NSString::from_str("/Users/john/notes/plan.md");
        let date = NSDate::dateWithTimeIntervalSince1970(f64::from(MONDAY_NOON) + 0.4);
        let (path, date): (&AnyObject, &AnyObject) = (&path, &date);
        assert_eq!(
            result(Some(path), Some(date)),
            Some(Used { path: "/Users/john/notes/plan.md".into(), last: MONDAY_NOON })
        );
        assert_eq!((result(Some(date), Some(date)), result(Some(path), Some(path))), (None, None));
        assert_eq!((result(None, Some(date)), result(Some(path), None)), (None, None));
    }

    #[test]
    #[ignore = "asks the real Spotlight index of this machine for recently used files"]
    fn spotlight_answers_under_home() {
        let since = now() - SEED_WITHIN;
        let used = recently_used(since, SEED_MOST);
        println!("{} recently used files, newest {:?}", used.len(), used.first());
        assert!(used.len() <= SEED_MOST);
        assert!(used.iter().all(|used| used.path.is_absolute() && used.last >= since && used.last <= now() + DAY));
        assert!(used.windows(2).all(|pair| pair[0].last >= pair[1].last));
    }
}
