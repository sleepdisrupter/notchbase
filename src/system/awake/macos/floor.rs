//! Turns sleep off or on with `pmset disablesleep`. The first time asks for an admin password and adds a sudoers rule
//! that allows only that command, so later switches need no password.

use std::ffi::{CStr, CString, c_char, c_void};
use std::process::{Command, Stdio};
use std::ptr;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn AuthorizationCreate(
        rights: *const c_void,
        environment: *const c_void,
        flags: u32,
        auth: *mut *mut c_void,
    ) -> i32;
    fn AuthorizationFree(auth: *mut c_void, flags: u32) -> i32;
    fn AuthorizationExecuteWithPrivileges(
        auth: *mut c_void,
        tool: *const c_char,
        flags: u32,
        argv: *const *const c_char,
        pipe: *mut *mut libc::FILE,
    ) -> i32;
}

const FLAG_DEFAULTS: u32 = 0;
const FLAG_DESTROY_RIGHTS: u32 = 1 << 3;

const SUDO: &str = "/usr/bin/sudo";
const PMSET: &str = "/usr/bin/pmset";
const DONE: &str = "notchbase-granted";

const GRANT: &CStr = c"set -e
dir=/etc/sudoers.d
file=$dir/notchbase
mkdir -p -m 755 \"$dir\"
tmp=$(mktemp \"$dir/.notchbase.XXXXXX\")
trap 'rm -f \"$tmp\"' EXIT
printf '%s ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 0, /usr/bin/pmset -a disablesleep 1\\n' \"$1\" > \"$tmp\"
chmod 440 \"$tmp\"
chown root:wheel \"$tmp\"
/usr/sbin/visudo -cf \"$tmp\" > /dev/null
mv \"$tmp\" \"$file\"
/usr/bin/pmset -a disablesleep \"$2\"
echo notchbase-granted
";

pub(super) fn request(disabled: bool, report: impl FnOnce(bool) + Send + 'static) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || report(run(disabled)));
}

fn run(disabled: bool) -> bool {
    let value = if disabled { "1" } else { "0" };
    quietly(value) || grant(value) && quietly(value)
}

fn quietly(value: &str) -> bool {
    Command::new(SUDO)
        .args(["-n", PMSET, "-a", "disablesleep", value])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn grant(value: &str) -> bool {
    let (Some(user), Ok(value)) = (account(), CString::new(value)) else {
        return false;
    };
    let user = CString::new(user).expect("a checked account name has no NUL");
    let argv: [*const c_char; 6] =
        [c"-c".as_ptr(), GRANT.as_ptr(), c"notchbase".as_ptr(), user.as_ptr(), value.as_ptr(), ptr::null()];
    unsafe {
        let mut auth: *mut c_void = ptr::null_mut();
        if AuthorizationCreate(ptr::null(), ptr::null(), FLAG_DEFAULTS, &mut auth) != 0 {
            return false;
        }
        let mut pipe: *mut libc::FILE = ptr::null_mut();
        let launched =
            AuthorizationExecuteWithPrivileges(auth, c"/bin/sh".as_ptr(), FLAG_DEFAULTS, argv.as_ptr(), &mut pipe);
        AuthorizationFree(auth, FLAG_DESTROY_RIGHTS);
        launched == 0 && !pipe.is_null() && finished(pipe)
    }
}

unsafe fn finished(pipe: *mut libc::FILE) -> bool {
    let mut output = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let read = unsafe { libc::fread(chunk.as_mut_ptr().cast(), 1, chunk.len(), pipe) };
        if read == 0 {
            break;
        }
        output.extend_from_slice(&chunk[..read]);
    }
    unsafe { libc::fclose(pipe) };
    String::from_utf8_lossy(&output).lines().any(|line| line.trim() == DONE)
}

fn account() -> Option<String> {
    let name = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() {
            return None;
        }
        CStr::from_ptr((*entry).pw_name).to_str().ok()?.to_owned()
    };
    plain(&name).then_some(name)
}

fn plain(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '.'])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_go_in_the_rule() {
        assert!(plain("johnnikitin"));
        assert!(plain("j.doe_2-x"));
        for bad in ["", "-ALL", ".hidden", "a b", "a,b", "a\nb", "a#b", "a\\b", "%wheel", "a=b"] {
            assert!(!plain(bad), "{bad:?} must not reach sudoers");
        }
    }

    #[test]
    fn grant_allows_only_the_two_switches() {
        let script = GRANT.to_str().unwrap();
        assert!(script.contains("disablesleep 0, /usr/bin/pmset -a disablesleep 1"));
        assert!(script.find("visudo -cf").unwrap() < script.find("mv ").unwrap(), "check before installing");
        assert!(script.trim_end().ends_with(DONE));
    }

    #[test]
    fn tests_never_show_the_dialog() {
        let reported = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = reported.clone();
        request(true, move |_| seen.store(true, std::sync::atomic::Ordering::SeqCst));
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!reported.load(std::sync::atomic::Ordering::SeqCst), "nothing should run under test");
    }
}
