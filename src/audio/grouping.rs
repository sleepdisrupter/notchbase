//! Folds an app's sessions or processes into one row: the loudest volume, the highest peak, muted only if all are.

use std::collections::HashMap;

use super::AppId;
use super::keys::app_id;

pub struct Member<S> {
    pub key: String,
    pub system: bool,
    pub volume: f32,
    pub muted: bool,
    pub peak: f32,
    pub handle: S,
}

pub struct Group<S> {
    pub id: AppId,
    pub key: String,
    pub system: bool,
    pub volume: f32,
    pub muted: bool,
    pub peak: f32,
    pub members: Vec<S>,
}

impl<S> Group<S> {
    fn founded_by(member: Member<S>) -> Self {
        Self {
            id: app_id(&member.key),
            key: member.key,
            system: member.system,
            volume: member.volume,
            muted: member.muted,
            peak: member.peak,
            members: vec![member.handle],
        }
    }

    fn absorb(&mut self, member: Member<S>) {
        self.volume = self.volume.max(member.volume);
        self.muted &= member.muted;
        self.peak = self.peak.max(member.peak);
        self.system |= member.system;
        self.members.push(member.handle);
    }
}

pub fn group_members<S>(members: impl IntoIterator<Item = Member<S>>) -> Vec<Group<S>> {
    let mut groups: Vec<Group<S>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for member in members {
        let folded = member.key.to_lowercase();
        match index.get(&folded) {
            Some(&i) => groups[i].absorb(member),
            None => {
                index.insert(folded, groups.len());
                groups.push(Group::founded_by(member));
            }
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(key: &str, volume: f32, muted: bool, peak: f32, handle: u32) -> Member<u32> {
        Member { key: key.to_owned(), system: false, volume, muted, peak, handle }
    }

    #[test]
    fn merges_one_executables_processes() {
        let groups = group_members(vec![
            member(r"c:\chrome.exe", 0.4, true, 0.1, 1),
            member(r"c:\spotify.exe", 1.0, false, 0.0, 2),
            member(r"C:\Chrome.exe", 0.8, false, 0.5, 3),
            member(r"c:\chrome.exe", 0.2, true, 0.0, 4),
        ]);
        assert_eq!(groups.len(), 2);
        let chrome = &groups[0];
        assert_eq!(chrome.members, vec![1, 3, 4]);
        assert_eq!(chrome.volume, 0.8);
        assert!(!chrome.muted, "one unmuted session means the app is audible");
        assert_eq!(chrome.peak, 0.5);
        assert_eq!(chrome.id, app_id(r"c:\chrome.exe"));
        assert_eq!(groups[1].members, vec![2]);

        let all_muted = group_members(vec![member("x", 1.0, true, 0.0, 1), member("x", 1.0, true, 0.0, 2)]);
        assert!(all_muted[0].muted);
    }

    #[test]
    fn keeps_first_members_key() {
        let groups = group_members(vec![
            member("com.Example.App", 1.0, false, 0.0, 1),
            member("COM.EXAMPLE.APP", 1.0, false, 0.0, 2),
        ]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "com.Example.App");
    }
}
