//! Whether stdin is a pipe or a socket. A bare launch fed through a pipe is a hook that lost its arguments.

#[cfg(unix)]
pub fn input_is_piped() -> bool {
    use std::os::fd::AsFd;

    std::io::stdin().as_fd().try_clone_to_owned().is_ok_and(|input| is_pipe(&std::fs::File::from(input)))
}

#[cfg(not(unix))]
pub fn input_is_piped() -> bool {
    false
}

#[cfg(unix)]
fn is_pipe(file: &std::fs::File) -> bool {
    use std::os::unix::fs::FileTypeExt;

    file.metadata().is_ok_and(|metadata| metadata.file_type().is_fifo() || metadata.file_type().is_socket())
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs::File;
    use std::os::fd::OwnedFd;

    use super::*;

    #[test]
    fn only_pipes_count() {
        let (reader, _writer) = std::io::pipe().unwrap();
        assert!(is_pipe(&File::from(OwnedFd::from(reader))));
        assert!(!is_pipe(&File::open("/dev/null").unwrap()));
        assert!(!is_pipe(&File::open(std::env::current_exe().unwrap()).unwrap()));
    }
}
