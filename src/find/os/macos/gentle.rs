//! Moves a scan thread to utility priority for the CPU and the disk, and stops it from downloading cloud files.

use super::ffi::{
    IOPOL_MATERIALIZE_DATALESS_FILES_OFF, IOPOL_SCOPE_THREAD, IOPOL_TYPE_DISK,
    IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_UTILITY, setiopolicy_np,
};

pub fn be_gentle() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
        setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD, IOPOL_UTILITY);
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_THREAD,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_int;

    use super::*;

    unsafe extern "C" {
        fn getiopolicy_np(kind: c_int, scope: c_int) -> c_int;
    }

    fn policies() -> (i32, i32) {
        unsafe {
            (
                getiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD),
                getiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_THREAD),
            )
        }
    }

    #[test]
    fn gentle_thread_runs_at_utility() {
        let gentle = std::thread::spawn(|| {
            be_gentle();
            policies()
        });
        assert_eq!(gentle.join().ok(), Some((IOPOL_UTILITY, IOPOL_MATERIALIZE_DATALESS_FILES_OFF)));
    }

    #[test]
    fn gentle_changes_only_caller() {
        let before = policies();
        let _ = std::thread::spawn(be_gentle).join();
        assert_eq!(policies(), before);
        assert_ne!(before.0, IOPOL_UTILITY);
    }
}
