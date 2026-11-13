//! Searches the stored paths for every word typed, whatever its case. The folded copy of the paths is built again
//! whenever they change.

use memchr::memmem;

use crate::find::key::Key;
use crate::find::model::fold_into;

use super::model::Model;

const END_OF_PATH: u8 = 0;

#[derive(Default)]
pub struct Folded {
    bytes: Vec<u8>,
    starts: Vec<u32>,
    built_from: Option<(u64, u64)>,
}

impl Model {
    pub(super) fn matching(&mut self, needles: &[&[u8]]) -> Vec<(Key, Box<str>)> {
        let Some(longest) =
            needles.iter().copied().max_by_key(|needle| needle.len()).filter(|needle| !needle.is_empty())
        else {
            return Vec::new();
        };
        self.fold_again_if_paths_changed();
        let (bytes, starts) = (&self.folded.bytes, &self.folded.starts);
        let finder = memmem::Finder::new(longest);
        let (mut found, mut from) = (Vec::new(), 0);
        while let Some(hit) = bytes.get(from..).and_then(|rest| finder.find(rest)) {
            let slot = starts.partition_point(|start| *start as usize <= from + hit).saturating_sub(1);
            let (Some(start), Some(next)) = (starts.get(slot), starts.get(slot + 1)) else { break };
            let path = &bytes[*start as usize..(*next as usize).saturating_sub(1)];
            if needles.iter().all(|needle| memmem::find(path, needle).is_some()) {
                found.extend(self.stored_at(slot));
            }
            from = *next as usize;
        }
        found
    }

    fn stored_at(&self, slot: usize) -> Option<(Key, Box<str>)> {
        let file = self.files.at(slot).filter(|(record, _)| !record.gone()).map(|(record, path)| (record.key, path));
        let folder = || slot.checked_sub(self.files.len()).and_then(|slot| self.folders.at(slot));
        let stored = if slot < self.files.len() { file } else { folder().map(|(record, path)| (record.key, path)) };
        stored.map(|(key, path)| (key, path.into()))
    }

    fn fold_again_if_paths_changed(&mut self) {
        let editions = (self.files.edition(), self.folders.edition());
        if self.folded.built_from == Some(editions) {
            return;
        }
        let folded = &mut self.folded;
        folded.bytes.clear();
        folded.starts.clear();
        folded.bytes.push(END_OF_PATH);
        let paths = self.files.each().map(|(_, path)| path).chain(self.folders.each().map(|(_, path)| path));
        for path in paths {
            folded.starts.push(folded.bytes.len() as u32);
            fold_into(path, &mut folded.bytes);
            folded.bytes.push(END_OF_PATH);
        }
        folded.starts.push(folded.bytes.len() as u32);
        folded.built_from = Some(editions);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::find::key::key_of;
    use crate::find::usage::record::{FolderRecord, state};

    const NOON: u32 = 1_704_110_400;

    fn model(files: &[&str], folders: &[&str]) -> Model {
        let mut model = Model::default();
        for file in files {
            assert!(model.admitted(Path::new(file), true, NOON).is_some());
        }
        for folder in folders {
            model.folders.put(FolderRecord::new(key_of(Path::new(folder))), folder);
        }
        model
    }

    fn paths(model: &mut Model, needles: &[&str]) -> Vec<String> {
        let needles: Vec<&[u8]> = needles.iter().map(|needle| needle.as_bytes()).collect();
        let mut paths: Vec<String> = model.matching(&needles).into_iter().map(|(_, path)| path.into()).collect();
        paths.sort();
        paths
    }

    #[test]
    fn matches_every_needle_any_case() {
        let mut model =
            model(&["/Users/john/GitHub/notchbase/Cargo.toml", "/Users/john/notes/cargo.md"], &["/Users/john/GitHub"]);
        assert_eq!(
            paths(&mut model, &["cargo"]),
            ["/Users/john/GitHub/notchbase/Cargo.toml", "/Users/john/notes/cargo.md"]
        );
        assert_eq!(paths(&mut model, &["cargo", "notch"]), ["/Users/john/GitHub/notchbase/Cargo.toml"]);
        assert_eq!(paths(&mut model, &["github"]), ["/Users/john/GitHub", "/Users/john/GitHub/notchbase/Cargo.toml"]);
        assert!(paths(&mut model, &["cargo", "zebra"]).is_empty());
        assert!(paths(&mut model, &[]).is_empty() && paths(&mut model, &[""]).is_empty());
    }

    #[test]
    fn match_carries_its_key() {
        let mut model = model(&["/Users/john/todo.txt"], &[]);
        let found = model.matching(&[b"todo"]);
        assert_eq!(found, [(key_of(Path::new("/Users/john/todo.txt")), "/Users/john/todo.txt".into())]);
    }

    #[test]
    fn needle_stays_in_one_path() {
        let mut model = model(&["/a/end", "/start/b"], &[]);
        assert!(paths(&mut model, &["end/start"]).is_empty());
        assert!(paths(&mut model, &["end", "start"]).is_empty());
        assert_eq!(paths(&mut model, &["/"]).len(), 2, "one row per path however often the needle occurs");
    }

    #[test]
    fn folded_copy_follows_changes() {
        let mut model = model(&["/Users/john/todo.txt", "/Users/john/plan.txt"], &[]);
        assert_eq!(paths(&mut model, &["txt"]).len(), 2);
        model.drop_files(&[key_of(Path::new("/Users/john/todo.txt"))]);
        assert_eq!(paths(&mut model, &["txt"]), ["/Users/john/plan.txt"]);
        assert!(model.admitted(Path::new("/Users/john/list.txt"), true, NOON).is_some());
        assert_eq!(paths(&mut model, &["txt"]), ["/Users/john/list.txt", "/Users/john/plan.txt"]);
    }

    #[test]
    fn missing_file_is_not_offered() {
        let mut model = model(&["/Users/john/todo.txt", "/Users/john/plan.txt"], &[]);
        if let Some(record) = model.files.get_mut(key_of(Path::new("/Users/john/todo.txt"))) {
            record.flags |= state::GONE;
        }
        assert_eq!(paths(&mut model, &["txt"]), ["/Users/john/plan.txt"]);
    }
}
