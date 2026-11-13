//! Whether a process is still running. One we aren't allowed to signal is still alive.

pub fn process_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
    if pid <= 0 {
        return false;
    }
    let signalled = unsafe { libc::kill(pid, 0) == 0 };
    signalled || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_processes_are_alive() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(u32::MAX));
        assert!(!process_alive(0));
    }
}
