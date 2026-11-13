//! The local time of day, for log lines and the stack's "Today at 14:02".

/// `secs` since 1970 as local calendar time.
#[cfg(unix)]
pub fn local(secs: i64) -> Option<libc::tm> {
    let secs = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    (!unsafe { libc::localtime_r(&secs, &mut tm) }.is_null()).then_some(tm)
}

/// `secs` since 1970 as local calendar time.
#[cfg(windows)]
pub fn local(secs: i64) -> Option<libc::tm> {
    let secs = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    (unsafe { libc::localtime_s(&mut tm, &secs) } == 0).then_some(tm)
}
