//! Runs the app for an install step and reports what went wrong.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CHUNK: usize = 4096;
const LAST_WORDS: Duration = Duration::from_millis(300);

/// Waits for the app and returns its last error line if it fails. After installing, the app
/// relaunches itself and that copy inherits stderr, so reading stderr to the end would wait for
/// it. Instead this stops listening shortly after the first process exits.
pub(super) fn finish(mut command: Command) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Couldn't start notchbase: {err}"))?;
    let (said, heard) = mpsc::channel();
    if let Some(mut stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut buffer = [0; CHUNK];
            while let Ok(read) = stderr.read(&mut buffer) {
                if read == 0 || said.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
    }
    let status = child.wait().map_err(|err| format!("Couldn't wait for notchbase: {err}"))?;
    if status.success() {
        return Ok(());
    }
    let until = Instant::now() + LAST_WORDS;
    let mut words = Vec::new();
    while let Ok(chunk) = heard.recv_timeout(until.saturating_duration_since(Instant::now())) {
        words.extend(chunk);
    }
    Err(super::problem(&String::from_utf8_lossy(&words), &status.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLE: &str = "NOTCHBASE_SETUP_TEST_ROLE";
    const LINGER: Duration = Duration::from_secs(15);

    fn playing(role: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "install::child::tests::plays_a_role", "--nocapture"]).env(ROLE, role);
        command
    }

    #[test]
    fn plays_a_role() {
        match std::env::var(ROLE).as_deref() {
            Ok("relaunching") => {
                let mut app = playing("relaunched");
                app.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
                eprintln!("notchbase: No room left");
                std::process::exit(if std::env::var("FAIL").is_ok() { 1 } else { 0 });
            }
            Ok("relaunched") => std::thread::sleep(LINGER),
            _ => {}
        }
    }

    #[test]
    fn relaunching_install_still_finishes() {
        let started = Instant::now();
        assert_eq!(finish(playing("relaunching")), Ok(()));
        let mut failing = playing("relaunching");
        failing.env("FAIL", "1");
        assert_eq!(finish(failing), Err("No room left".to_owned()));
        assert!(started.elapsed() < Duration::from_secs(5), "waited for the relaunched app");
    }

    #[test]
    #[ignore = "shows the old hang: reading the output to its end waits for the relaunched app"]
    fn reading_to_the_end_hangs() {
        let started = Instant::now();
        let output = playing("relaunching").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output();
        assert!(output.is_ok());
        assert!(started.elapsed() >= LINGER - Duration::from_secs(1), "{:?}", started.elapsed());
    }
}
