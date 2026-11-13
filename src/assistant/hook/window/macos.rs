//! Parents from proc_pidinfo, stopping at launchd. A window owner is an app with the regular activation policy.

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};

const LAUNCHD: u32 = 1;

pub(super) fn owner(me: u32) -> Option<u32> {
    super::nearest(me, parent, regular)
}

fn parent(pid: u32) -> Option<u32> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = libc::c_int::try_from(size_of::<libc::proc_bsdshortinfo>()).ok()?;
    let written = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDT_SHORTBSDINFO, 0, (&raw mut info).cast(), size) };
    (written == size).then_some(info.pbsi_ppid).filter(|parent| *parent > LAUNCHD)
}

fn regular(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
}
