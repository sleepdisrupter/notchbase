//! Unpacks the records getattrlistbulk returns into listing rows.

use crate::find::os::{Listed, Raw, raw};

use super::ffi::{ATTR_CMN_ERROR, SF_DATALESS, VDIR, VLNK};

pub const ASKED: u32 = libc::ATTR_CMN_RETURNED_ATTRS
    | libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_DEVID
    | libc::ATTR_CMN_OBJTYPE
    | libc::ATTR_CMN_CRTIME
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_FLAGS
    | libc::ATTR_CMN_FILEID
    | ATTR_CMN_ERROR;

const WORD: usize = 4;
const RETURNED_AT: usize = WORD;
const FIELDS_AT: usize = RETURNED_AT + 5 * WORD;
const NANOSECONDS: usize = 8;
const END_OF_NAME: u8 = 0;
const DOT: u8 = b'.';
const VANISHED: u32 = libc::ENOENT as u32;

pub fn unpack(buffer: &[u8], records: usize, devices: [u64; 2], into: &mut Listed) -> Option<i32> {
    let (mut rest, mut refused) = (buffer, None);
    for _ in 0..records {
        let Some(length) = word(rest, 0).map(|length| length as usize) else { break };
        if length < FIELDS_AT || length > rest.len() {
            break;
        }
        let (record, after) = rest.split_at(length);
        if let Err(code) = entry(record, devices, into) {
            refused.get_or_insert(code);
        }
        rest = after;
    }
    refused
}

fn entry(record: &[u8], devices: [u64; 2], into: &mut Listed) -> Result<(), i32> {
    let returned = word(record, RETURNED_AT).unwrap_or(0);
    let mut fields = Fields { record, at: FIELDS_AT, returned };
    let failure = fields.word(ATTR_CMN_ERROR).unwrap_or(0);
    let name = fields.name();
    let device = fields.word(libc::ATTR_CMN_DEVID).map(|device| device as i32 as u64);
    let kind = fields.word(libc::ATTR_CMN_OBJTYPE).unwrap_or(0);
    let created = fields.seconds(libc::ATTR_CMN_CRTIME);
    let modified = fields.seconds(libc::ATTR_CMN_MODTIME);
    let flags = fields.word(libc::ATTR_CMN_FLAGS).unwrap_or(0);
    let file_id = fields.long(libc::ATTR_CMN_FILEID).unwrap_or(0);
    match (failure, name) {
        (0, Some(name)) if !name.is_empty() => {
            let elsewhere = device.is_some_and(|device| devices != [0, 0] && !devices.contains(&device));
            let bits = bits(name, kind, flags, elsewhere);
            into.names.extend_from_slice(String::from_utf8_lossy(name).as_bytes());
            into.names.push(END_OF_NAME);
            into.rows.push(Raw { modified, created, file_id, bits });
            Ok(())
        }
        (0 | VANISHED, _) => Ok(()),
        (code, _) => Err(code as i32),
    }
}

fn bits(name: &[u8], kind: u32, flags: u32, elsewhere: bool) -> u8 {
    let folder = kind == VDIR;
    let mut bits = match kind {
        VDIR => raw::DIR,
        VLNK => raw::LINK,
        _ => 0,
    };
    if name.first() == Some(&DOT) || flags & libc::UF_HIDDEN != 0 {
        bits |= raw::HIDDEN;
    }
    if flags & SF_DATALESS != 0 {
        bits |= raw::DATALESS | raw::CLOUD;
    }
    if folder && elsewhere {
        bits |= raw::ELSEWHERE;
    }
    bits
}

struct Fields<'a> {
    record: &'a [u8],
    at: usize,
    returned: u32,
}

impl<'a> Fields<'a> {
    fn take(&mut self, attribute: u32, size: usize) -> Option<&'a [u8]> {
        if self.returned & attribute == 0 {
            return None;
        }
        let field = self.record.get(self.at..self.at.checked_add(size)?)?;
        self.at += size;
        Some(field)
    }

    fn word(&mut self, attribute: u32) -> Option<u32> {
        self.take(attribute, WORD).and_then(|field| word(field, 0))
    }

    fn long(&mut self, attribute: u32) -> Option<u64> {
        self.take(attribute, 2 * WORD).and_then(|field| Some(u64::from_ne_bytes(field.try_into().ok()?)))
    }

    fn seconds(&mut self, attribute: u32) -> u32 {
        let whole = self.take(attribute, 2 * WORD + NANOSECONDS).and_then(|field| field.get(..2 * WORD));
        let seconds = whole.and_then(|field| Some(i64::from_ne_bytes(field.try_into().ok()?))).unwrap_or(0);
        u32::try_from(seconds.max(0)).unwrap_or(u32::MAX)
    }

    fn name(&mut self) -> Option<&'a [u8]> {
        let reference = self.at;
        let field = self.take(libc::ATTR_CMN_NAME, 2 * WORD)?;
        let offset = usize::try_from(word(field, 0)? as i32).ok()?;
        let length = word(field, WORD)? as usize;
        let start = reference.checked_add(offset)?;
        let stored = self.record.get(start..start.checked_add(length)?)?;
        Some(stored.split(|byte| *byte == END_OF_NAME).next().unwrap_or(stored))
    }
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let field = bytes.get(at..at.checked_add(WORD)?)?;
    Some(u32::from_ne_bytes(field.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: u64 = 7;
    const NAME_FIELD: usize = 2 * WORD;
    const TIME_FIELD: usize = 2 * WORD + NANOSECONDS;
    const PLAIN: u32 = ASKED & !ATTR_CMN_ERROR;

    fn record(name: &str, kind: u32, flags: u32, device: i32, failure: Option<u32>) -> Vec<u8> {
        let returned = if failure.is_some() { ASKED } else { PLAIN };
        let mut fields = Vec::new();
        fields.extend(failure.into_iter().flat_map(u32::to_ne_bytes));
        let after_name = NAME_FIELD + WORD + WORD + 2 * TIME_FIELD + WORD + 2 * WORD;
        fields.extend((after_name as i32).to_ne_bytes());
        fields.extend((name.len() as u32 + 1).to_ne_bytes());
        fields.extend(device.to_ne_bytes());
        fields.extend(kind.to_ne_bytes());
        fields.extend(30i64.to_ne_bytes().into_iter().chain(0i64.to_ne_bytes()));
        fields.extend(40i64.to_ne_bytes().into_iter().chain(0i64.to_ne_bytes()));
        fields.extend(flags.to_ne_bytes());
        fields.extend(99u64.to_ne_bytes());
        fields.extend(name.bytes().chain([END_OF_NAME]));
        fields.resize(fields.len().next_multiple_of(WORD), 0);
        let mut record = ((FIELDS_AT + fields.len()) as u32).to_ne_bytes().to_vec();
        record.extend(returned.to_ne_bytes());
        record.extend([0; 4 * WORD]);
        record.extend(fields);
        record
    }

    fn names(listed: &Listed) -> Vec<String> {
        let names = listed.names.split(|byte| *byte == END_OF_NAME);
        names.zip(&listed.rows).map(|(name, _)| String::from_utf8_lossy(name).into_owned()).collect()
    }

    #[test]
    fn record_becomes_row() {
        let mut listed = Listed::default();
        let mut buffer = record("notes.txt", 1, 0, HERE as i32, None);
        buffer.extend(record(".hidden", VDIR, 0, HERE as i32, None));
        buffer.extend(record("Shown", VDIR, libc::UF_HIDDEN, HERE as i32, None));
        buffer.extend(record("link", VLNK, 0, HERE as i32, None));
        buffer.extend(record("cloud", VDIR, SF_DATALESS, HERE as i32, None));
        assert_eq!(unpack(&buffer, 5, [HERE, HERE], &mut listed), None);
        assert_eq!(names(&listed), ["notes.txt", ".hidden", "Shown", "link", "cloud"]);
        assert_eq!(listed.rows[0], Raw { modified: 40, created: 30, file_id: 99, bits: 0 });
        let bits: Vec<u8> = listed.rows.iter().map(|row| row.bits).collect();
        let hidden = raw::DIR | raw::HIDDEN;
        assert_eq!(bits, [0, hidden, hidden, raw::LINK, raw::DIR | raw::DATALESS | raw::CLOUD]);
    }

    #[test]
    fn only_foreign_folder_is_elsewhere() {
        let mut listed = Listed::default();
        let mut buffer = record("mounted", VDIR, 0, 8, None);
        buffer.extend(record("data", VDIR, 0, 9, None));
        buffer.extend(record("file", 1, 0, 8, None));
        unpack(&buffer, 3, [HERE, 9], &mut listed);
        assert_eq!(
            listed.rows.iter().map(|row| row.bits).collect::<Vec<_>>(),
            [raw::DIR | raw::ELSEWHERE, raw::DIR, 0]
        );
        let mut unfenced = Listed::default();
        unpack(&buffer, 3, [0, 0], &mut unfenced);
        assert!(unfenced.rows.iter().all(|row| row.bits & raw::ELSEWHERE == 0), "no pair means no fence");
    }

    #[test]
    fn unreadable_entry_reported() {
        let mut listed = Listed::default();
        let mut buffer = record("fine", 1, 0, 7, Some(0));
        buffer.extend(record("gone", 1, 0, 7, Some(VANISHED)));
        buffer.extend(record("broken", 1, 0, 7, Some(libc::EIO as u32)));
        buffer.extend(record("after", 1, 0, 7, None));
        assert_eq!(unpack(&buffer, 4, [0, 0], &mut listed), Some(libc::EIO));
        assert_eq!(names(&listed), ["fine", "after"]);
    }

    #[test]
    fn bad_record_ends_parse() {
        let whole = record("whole", 1, 0, 7, None);
        for cut in 0..whole.len() {
            let mut listed = Listed::default();
            let mut buffer = whole.clone();
            buffer.extend(&whole[..cut]);
            assert_eq!(unpack(&buffer, 2, [0, 0], &mut listed), None);
            assert_eq!(names(&listed), ["whole"], "cut at {cut}");
        }
        let mut lying = whole.clone();
        lying[FIELDS_AT..FIELDS_AT + WORD].copy_from_slice(&i32::MAX.to_ne_bytes());
        let mut listed = Listed::default();
        assert_eq!(unpack(&lying, 1, [0, 0], &mut listed), None);
        assert!(listed.rows.is_empty(), "a name that points outside its record is not a row");
    }
}
