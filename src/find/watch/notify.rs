//! Parses the change records ReadDirectoryChangesExW fills on Windows. An old name followed by its new name is a move,
//! and a buffer that is cut short or lies about its lengths rescans the drive.

use std::path::{Path, PathBuf};

use super::{Change, Event};

pub mod action {
    #[cfg(test)]
    pub const ADDED: u32 = 1;
    #[cfg(test)]
    pub const REMOVED: u32 = 2;
    #[cfg(test)]
    pub const MODIFIED: u32 = 3;
    pub const RENAMED_OLD_NAME: u32 = 4;
    pub const RENAMED_NEW_NAME: u32 = 5;
}

const NEXT_AT: usize = 0;
const ACTION_AT: usize = 4;
const NAME_LENGTH_AT: usize = 80;
pub const HEADER: usize = 84;
const WIDE: usize = 2;
const SEPARATOR: char = '\\';

#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    action: u32,
    name: String,
}

pub fn parse(buffer: &[u8], root: &Path, counter: &mut u64) -> Vec<Event> {
    let Some(records) = records(buffer) else { return vec![numbered(Change::Overflow(root.to_path_buf()), counter)] };
    let mut out = Vec::with_capacity(records.len());
    let mut pending_old: Option<PathBuf> = None;
    for record in records {
        let path = beneath(root, &record.name);
        let change = match record.action {
            action::RENAMED_OLD_NAME => {
                if let Some(old) = pending_old.replace(path) {
                    out.push(numbered(Change::Touched(old), counter));
                }
                continue;
            }
            action::RENAMED_NEW_NAME => match pending_old.take() {
                Some(from) => Change::Moved { from, to: path },
                None => Change::Touched(path),
            },
            _ => Change::Touched(path),
        };
        if let Some(old) = pending_old.take() {
            out.push(numbered(Change::Touched(old), counter));
        }
        out.push(numbered(change, counter));
    }
    if let Some(old) = pending_old {
        out.push(numbered(Change::Touched(old), counter));
    }
    out
}

fn records(buffer: &[u8]) -> Option<Vec<Record>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let entry = buffer.get(at..)?;
        let next = word(entry, NEXT_AT)? as usize;
        let action = word(entry, ACTION_AT)?;
        let length = word(entry, NAME_LENGTH_AT)? as usize;
        let name = entry.get(HEADER..HEADER.checked_add(length)?)?;
        if !length.is_multiple_of(WIDE) || (next != 0 && next < HEADER + length) {
            return None;
        }
        let units: Vec<u16> = name.as_chunks::<WIDE>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        out.push(Record { action, name: String::from_utf16_lossy(&units) });
        if next == 0 {
            return Some(out);
        }
        at = at.checked_add(next)?;
    }
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
}

fn beneath(root: &Path, name: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    name.split(SEPARATOR).filter(|part| !part.is_empty()).for_each(|part| path.push(part));
    path
}

fn numbered(change: Change, counter: &mut u64) -> Event {
    *counter += 1;
    Event { change, id: *counter }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALIGN: usize = 4;

    fn entry(action: u32, name: &str, last: bool) -> Vec<u8> {
        let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let size = (HEADER + wide.len()).next_multiple_of(ALIGN);
        let mut bytes = vec![0; size];
        let next = if last { 0 } else { size as u32 };
        bytes[NEXT_AT..NEXT_AT + 4].copy_from_slice(&next.to_le_bytes());
        bytes[ACTION_AT..ACTION_AT + 4].copy_from_slice(&action.to_le_bytes());
        bytes[NAME_LENGTH_AT..NAME_LENGTH_AT + 4].copy_from_slice(&(wide.len() as u32).to_le_bytes());
        bytes[HEADER..HEADER + wide.len()].copy_from_slice(&wide);
        bytes
    }

    fn buffer(entries: &[(u32, &str)]) -> Vec<u8> {
        let last = entries.len().saturating_sub(1);
        entries.iter().enumerate().flat_map(|(at, (action, name))| entry(*action, name, at == last)).collect()
    }

    fn changes(bytes: &[u8]) -> Vec<Change> {
        parse(bytes, Path::new("C:"), &mut 0).into_iter().map(|event| event.change).collect()
    }

    fn under(parts: &[&str]) -> PathBuf {
        parts.iter().fold(PathBuf::from("C:"), |path, part| path.join(part))
    }

    #[test]
    fn records_become_numbered_hints() {
        let bytes = buffer(&[(action::ADDED, "Users\\me\\a.txt"), (action::MODIFIED, "b.txt"), (action::REMOVED, "c")]);
        let events = parse(&bytes, Path::new("C:"), &mut 10);
        let expected = [
            Event { change: Change::Touched(under(&["Users", "me", "a.txt"])), id: 11 },
            Event { change: Change::Touched(under(&["b.txt"])), id: 12 },
            Event { change: Change::Touched(under(&["c"])), id: 13 },
        ];
        assert_eq!(events, expected);
    }

    #[test]
    fn old_then_new_name_is_a_move() {
        let bytes =
            buffer(&[(action::RENAMED_OLD_NAME, "Work\\draft.md"), (action::RENAMED_NEW_NAME, "Work\\final.md")]);
        let moved = Change::Moved { from: under(&["Work", "draft.md"]), to: under(&["Work", "final.md"]) };
        assert_eq!(changes(&bytes), [moved]);
    }

    #[test]
    fn split_rename_halves_stand_alone() {
        assert_eq!(
            changes(&buffer(&[(action::RENAMED_OLD_NAME, "gone.txt")])),
            [Change::Touched(under(&["gone.txt"]))]
        );
        assert_eq!(
            changes(&buffer(&[(action::RENAMED_NEW_NAME, "came.txt")])),
            [Change::Touched(under(&["came.txt"]))]
        );
        let interrupted = buffer(&[(action::RENAMED_OLD_NAME, "a"), (action::ADDED, "b")]);
        assert_eq!(changes(&interrupted), [Change::Touched(under(&["a"])), Change::Touched(under(&["b"]))]);
    }

    #[test]
    fn wide_unicode_names_survive() {
        assert_eq!(changes(&buffer(&[(action::ADDED, "résumé 📄.pdf")])), [Change::Touched(under(&["résumé 📄.pdf"]))]);
    }

    #[test]
    fn bad_buffer_rescans_the_drive() {
        let whole = buffer(&[(action::ADDED, "a.txt"), (action::ADDED, "b.txt")]);
        let overflow = vec![Change::Overflow(PathBuf::from("C:"))];
        assert_eq!(changes(&whole[..whole.len() - 3]), overflow, "cut short");
        assert_eq!(changes(&whole[..HEADER - 1]), overflow, "not even a header");
        let mut lying = buffer(&[(action::ADDED, "a.txt")]);
        lying[NAME_LENGTH_AT..NAME_LENGTH_AT + 4].copy_from_slice(&4_000u32.to_le_bytes());
        assert_eq!(changes(&lying), overflow, "a name longer than the buffer");
        let mut looping = buffer(&[(action::ADDED, "a.txt"), (action::ADDED, "b.txt")]);
        looping[NEXT_AT..NEXT_AT + 4].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(changes(&looping), overflow, "an entry that points into itself");
        assert_eq!(changes(&[]), overflow);
    }
}
