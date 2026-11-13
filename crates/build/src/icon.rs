//! Builds a Windows .res from the app icon by hand, so the build doesn't need rc.exe.

use std::path::{Path, PathBuf};

// Ids and memory flags match what rc.exe writes.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const GROUP_ID: u16 = 1;
const ENGLISH: u16 = 0x0409;
const ICON_FLAGS: u16 = 0x1010;
const GROUP_FLAGS: u16 = 0x1030;
const DIR_LEN: usize = 6;
const ENTRY_LEN: usize = 16;

pub fn embed_icon(icon: &Path, res_name: &str) {
    println!("cargo:rerun-if-changed={}", icon.display());
    let os = std::env::var("CARGO_CFG_TARGET_OS").ok();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").ok();
    if !links_resources(os.as_deref(), env.as_deref()) {
        return;
    }
    let ico = std::fs::read(icon).unwrap_or_else(|err| panic!("can't read {}: {err}", icon.display()));
    let res = resources(&ico).unwrap_or_else(|| panic!("{} isn't a valid icon", icon.display()));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join(res_name);
    std::fs::write(&out, res).expect("write the icon resource");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

fn links_resources(os: Option<&str>, env: Option<&str>) -> bool {
    os == Some("windows") && env == Some("msvc")
}

fn resources(ico: &[u8]) -> Option<Vec<u8>> {
    let count = usize::from(u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]));
    let mut res = Vec::new();
    header(&mut res, 0, 0, 0, 0);
    let mut group = vec![0, 0, 1, 0];
    group.extend((count as u16).to_le_bytes());
    for index in 0..count {
        let entry = ico.get(DIR_LEN + index * ENTRY_LEN..DIR_LEN + (index + 1) * ENTRY_LEN)?;
        let size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let id = index as u16 + 1;
        resource(&mut res, RT_ICON, id, ICON_FLAGS, ico.get(offset..offset + size)?);
        group.extend(&entry[..12]);
        group.extend(id.to_le_bytes());
    }
    resource(&mut res, RT_GROUP_ICON, GROUP_ID, GROUP_FLAGS, &group);
    Some(res)
}

fn resource(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
    header(res, data.len() as u32, kind, id, flags);
    res.extend(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

// Every resource starts with a 32 byte RESOURCEHEADER.
fn header(res: &mut Vec<u8>, size: u32, kind: u16, id: u16, flags: u16) {
    res.extend(size.to_le_bytes());
    res.extend(32u32.to_le_bytes());
    res.extend([0xFFFF, kind, 0xFFFF, id].iter().flat_map(|word: &u16| word.to_le_bytes()));
    res.extend(0u32.to_le_bytes());
    res.extend(flags.to_le_bytes());
    res.extend(if size == 0 { 0u16 } else { ENGLISH }.to_le_bytes());
    res.extend([0u8; 8]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER_LEN: usize = 32;
    const GROUP_ENTRY_LEN: usize = 14;

    fn ico(images: &[&[u8]]) -> Vec<u8> {
        let mut file = vec![0, 0, 1, 0];
        file.extend((images.len() as u16).to_le_bytes());
        let mut offset = DIR_LEN + images.len() * ENTRY_LEN;
        for (index, image) in images.iter().enumerate() {
            let side = 16 * (index as u8 + 1);
            file.extend([side, side, 0, 0, 1, 0, 32, 0]);
            file.extend((image.len() as u32).to_le_bytes());
            file.extend((offset as u32).to_le_bytes());
            offset += image.len();
        }
        for image in images {
            file.extend(*image);
        }
        file
    }

    fn word(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn long(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn only_msvc_links_the_resource() {
        assert!(links_resources(Some("windows"), Some("msvc")));
        assert!(!links_resources(Some("windows"), Some("gnu")));
        assert!(!links_resources(Some("macos"), Some("")));
        assert!(!links_resources(Some("linux"), Some("gnu")));
        assert!(!links_resources(None, Some("msvc")));
        assert!(!links_resources(Some("windows"), None));
    }

    #[test]
    fn icon_is_not_read_outside_msvc() {
        embed_icon(Path::new("no/such/icon.ico"), "unused.res");
    }

    #[test]
    fn starts_with_empty_header() {
        let res = resources(&ico(&[b"abcd"])).unwrap();
        assert_eq!(long(&res, 0), 0);
        assert_eq!(long(&res, 4), HEADER_LEN as u32);
        assert_eq!([word(&res, 8), word(&res, 10), word(&res, 12), word(&res, 14)], [0xFFFF, 0, 0xFFFF, 0]);
        assert_eq!(word(&res, 22), 0, "the empty header names no language");
        assert_eq!(&res[24..HEADER_LEN], &[0; 8]);
    }

    #[test]
    fn images_become_numbered_icons() {
        let res = resources(&ico(&[b"abcd", b"efgh"])).unwrap();
        for (index, data) in [b"abcd", b"efgh"].iter().enumerate() {
            let at = HEADER_LEN + index * (HEADER_LEN + 4);
            assert_eq!(long(&res, at), 4);
            assert_eq!([word(&res, at + 10), word(&res, at + 14)], [RT_ICON, index as u16 + 1]);
            assert_eq!([word(&res, at + 20), word(&res, at + 22)], [ICON_FLAGS, ENGLISH]);
            assert_eq!(&res[at + HEADER_LEN..at + HEADER_LEN + 4], *data);
        }
    }

    #[test]
    fn group_lists_every_image() {
        let file = ico(&[b"abcd", b"efgh"]);
        let res = resources(&file).unwrap();
        let at = HEADER_LEN + 2 * (HEADER_LEN + 4);
        let group_len = DIR_LEN + 2 * GROUP_ENTRY_LEN;
        assert_eq!(long(&res, at) as usize, group_len);
        assert_eq!([word(&res, at + 10), word(&res, at + 14)], [RT_GROUP_ICON, GROUP_ID]);
        assert_eq!(word(&res, at + 20), GROUP_FLAGS);
        let group = &res[at + HEADER_LEN..at + HEADER_LEN + group_len];
        assert_eq!(&group[..DIR_LEN], &[0, 0, 1, 0, 2, 0]);
        for index in 0..2 {
            let entry = &group[DIR_LEN + index * GROUP_ENTRY_LEN..DIR_LEN + (index + 1) * GROUP_ENTRY_LEN];
            let source = &file[DIR_LEN + index * ENTRY_LEN..DIR_LEN + index * ENTRY_LEN + 12];
            assert_eq!(&entry[..12], source);
            assert_eq!(word(entry, 12), index as u16 + 1);
        }
        assert_eq!(res.len(), at + HEADER_LEN + group_len.next_multiple_of(4));
    }

    #[test]
    fn resources_are_padded_to_four_bytes() {
        let res = resources(&ico(&[b"a", b"abcde"])).unwrap();
        assert!(res.len().is_multiple_of(4));
        let second = HEADER_LEN + HEADER_LEN + 4;
        assert_eq!(&res[HEADER_LEN + HEADER_LEN..second], &[b'a', 0, 0, 0]);
        assert_eq!(long(&res, second), 5);
        assert_eq!(&res[second + HEADER_LEN..second + HEADER_LEN + 8], b"abcde\0\0\0");
    }

    #[test]
    fn no_images_still_writes_a_group() {
        let res = resources(&ico(&[])).unwrap();
        assert_eq!(long(&res, HEADER_LEN), DIR_LEN as u32);
        assert_eq!(&res[2 * HEADER_LEN..2 * HEADER_LEN + DIR_LEN], &[0, 0, 1, 0, 0, 0]);
        assert_eq!(res.len(), 2 * HEADER_LEN + 8);
    }

    #[test]
    fn rejects_truncated_header() {
        assert_eq!(resources(&[]), None);
        assert_eq!(resources(&[0, 0, 1, 0, 1]), None);
    }

    #[test]
    fn rejects_short_directory() {
        let mut file = ico(&[b"abcd"]);
        file[4] = 2;
        file.truncate(DIR_LEN + ENTRY_LEN + 1);
        assert_eq!(resources(&file), None);
    }

    #[test]
    fn rejects_image_past_end() {
        let mut file = ico(&[b"abcd"]);
        file.truncate(file.len() - 1);
        assert_eq!(resources(&file), None);
    }

    #[test]
    fn shipped_icon_converts() {
        let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon/notchbase.ico");
        let file = std::fs::read(icon).unwrap();
        let res = resources(&file).expect("a valid icon");
        let count = usize::from(word(&file, 4));
        assert!(count > 0);
        assert!(res.len() > file.len() - DIR_LEN - count * ENTRY_LEN);
        assert!(res.len().is_multiple_of(4));
    }
}
