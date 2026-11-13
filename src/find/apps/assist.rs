//! Reads the UserAssist records Windows keeps: how often and when each app was last started.

use std::path::{Path, PathBuf};

use crate::find::context::Stamp;

use super::Used;

#[cfg_attr(not(windows), expect(dead_code))]
pub const BRANCHES: [&str; 2] = ["{F4E57C4B-2036-45F0-A9AB-443BCFE33D9F}", "{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}"];
pub const OWN_PROGRAMS: &str = "{A77F5D77-2E2B-44C3-A6A2-ABA601054A51}";
pub const EVERYONES_PROGRAMS: &str = "{0139D44E-6AFE-49F2-8690-3DAFCAE6FFB8}";

const RECORD: usize = 72;
const TIMES_AT: usize = 4;
const LAST_AT: usize = 60;
const TICKS_A_SECOND: u64 = 10_000_000;
const SECONDS_TO_1970: u64 = 11_644_473_600;
const HALF_THE_LETTERS: u8 = 13;
const SEPARATOR: char = '\\';

pub fn rot13(text: &str) -> String {
    let turned = |letter: char, first: u8| char::from(first + (letter as u8 - first + HALF_THE_LETTERS) % 26);
    let plain = |letter: char| match letter {
        'a'..='z' => turned(letter, b'a'),
        'A'..='Z' => turned(letter, b'A'),
        other => other,
    };
    text.chars().map(plain).collect()
}

pub fn counted(record: &[u8]) -> Option<Used> {
    let record = record.get(..RECORD)?;
    let times = u32::from_le_bytes(record[TIMES_AT..TIMES_AT + 4].try_into().ok()?);
    let last = u64::from_le_bytes(record[LAST_AT..LAST_AT + 8].try_into().ok()?);
    let used = Used { last: unix(last), times };
    (used != Used::default()).then_some(used)
}

pub fn placed(name: &str, folders: &[(&str, PathBuf)]) -> Option<PathBuf> {
    let known = folders.iter().find_map(|(id, folder)| {
        let below = name.get(..id.len()).filter(|start| start.eq_ignore_ascii_case(id)).map(|_| &name[id.len()..])?;
        Some(folder.join(below.trim_start_matches(SEPARATOR)))
    });
    known.or_else(|| Some(PathBuf::from(name)).filter(|path| is_rooted(path)))
}

fn is_rooted(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let mut letters = text.chars();
    let drive = letters.next().is_some_and(|letter| letter.is_ascii_alphabetic()) && letters.next() == Some(':');
    drive && letters.next() == Some(SEPARATOR)
}

fn unix(filetime: u64) -> Stamp {
    let seconds = (filetime / TICKS_A_SECOND).saturating_sub(SECONDS_TO_1970);
    if filetime == 0 { 0 } else { Stamp::try_from(seconds).unwrap_or(Stamp::MAX) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY_NOON: u32 = 1_704_110_400;

    fn record(times: u32, last: u64) -> Vec<u8> {
        let mut record = vec![0u8; RECORD];
        record[TIMES_AT..TIMES_AT + 4].copy_from_slice(&times.to_le_bytes());
        record[LAST_AT..LAST_AT + 8].copy_from_slice(&last.to_le_bytes());
        record
    }

    #[test]
    fn names_are_rot13() {
        assert_eq!(
            rot13("{N77S5Q77-2R2O-44P3-N6N2-NON601054N51}\\Fgrnz\\Fgrnz.yax"),
            format!("{OWN_PROGRAMS}\\Steam\\Steam.lnk")
        );
        assert_eq!(
            rot13("Zvpebfbsg.JvaqbjfPnyphyngbe_8jrxlo3q8oojr!Ncc"),
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        );
        assert_eq!(rot13(&rot13("C:\\Tools\\caf\u{e9} 7.exe")), "C:\\Tools\\caf\u{e9} 7.exe", "only letters turn");
    }

    #[test]
    fn record_holds_count_and_last() {
        let noon = (u64::from(MONDAY_NOON) + SECONDS_TO_1970) * TICKS_A_SECOND;
        assert_eq!(counted(&record(42, noon)), Some(Used { last: MONDAY_NOON, times: 42 }));
        assert_eq!(counted(&record(3, 0)), Some(Used { last: 0, times: 3 }), "counted, though never dated");
        assert_eq!(counted(&record(0, 0)), None, "never started");
        assert_eq!(counted(&record(1, noon)[..RECORD - 1]), None, "an older, shorter record is not guessed at");
        assert_eq!(counted(&record(7, 5)), Some(Used { last: 0, times: 7 }), "a date before 1970 is no date");
    }

    #[test]
    fn known_folder_name_becomes_path() {
        let menu = PathBuf::from("C:\\Users\\me\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs");
        let folders = [(OWN_PROGRAMS, menu.clone()), (EVERYONES_PROGRAMS, PathBuf::from("C:\\ProgramData\\Programs"))];
        let own = format!("{}\\Steam\\Steam.lnk", OWN_PROGRAMS.to_lowercase());
        assert_eq!(placed(&own, &folders), Some(menu.join("Steam\\Steam.lnk")));
        let shared = format!("{EVERYONES_PROGRAMS}\\Notepad.lnk");
        assert_eq!(placed(&shared, &folders), Some(PathBuf::from("C:\\ProgramData\\Programs").join("Notepad.lnk")));
        assert_eq!(placed("C:\\Tools\\run.exe", &folders), Some(PathBuf::from("C:\\Tools\\run.exe")));
        assert_eq!(placed("UEME_CTLSESSION", &folders), None);
        assert_eq!(placed("{6D809377-6AF0-444B-8957-A3773F02200E}\\app.exe", &folders), None, "a folder nobody named");
        assert_eq!(placed("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", &folders), None, "no path at all");
    }
}
