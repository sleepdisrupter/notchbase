//! Turns a row into its path, and a path back into its row.

use std::path::{Path, PathBuf};

use crate::find::key::{Key, START, chain, key_of};

use super::fold::fold_into;
use super::{EntryId, Live};

impl Live {
    pub fn path_into(&self, id: EntryId, out: &mut PathBuf) {
        out.clear();
        let mut name = String::new();
        for step in self.lineage(id) {
            self.name_into(step, &mut name);
            out.push(&name);
        }
    }

    pub fn key(&self, id: EntryId) -> Key {
        let mut name = String::new();
        let mut steps = self.lineage(id).into_iter();
        let root = steps.next().map_or(START, |root| {
            self.name_into(root, &mut name);
            key_of(Path::new(&name))
        });
        steps.fold(root, |key, step| {
            self.name_into(step, &mut name);
            chain(key, &name)
        })
    }

    pub fn resolve(&self, path: &Path) -> Option<EntryId> {
        let (mut name, mut folded) = (String::new(), Vec::new());
        let mut root: Option<(EntryId, PathBuf)> = None;
        for id in self.roots.iter().copied().filter(|id| !self.is_dead(*id)) {
            self.name_into(id, &mut name);
            let longer = root.as_ref().is_none_or(|(_, prefix)| name.len() > prefix.as_os_str().len());
            if longer && path.starts_with(&name) {
                root = Some((id, PathBuf::from(&name)));
            }
        }
        let (root, prefix) = root?;
        path.strip_prefix(prefix).ok()?.components().try_fold(root, |dir, step| {
            let name = step.as_os_str().to_string_lossy();
            folded.clear();
            fold_into(&name, &mut folded);
            self.child(dir, &folded, &name)
        })
    }

    fn child(&self, dir: EntryId, folded: &[u8], name: &str) -> Option<EntryId> {
        let first = self.base.len();
        let settled = if (dir as usize) < first { self.base.child(dir, folded, name) } else { None };
        settled.or_else(|| self.overlay.find(dir, name).map(|row| first + row)).map(|row| row as EntryId)
    }

    fn lineage(&self, id: EntryId) -> Vec<EntryId> {
        let mut steps: Vec<EntryId> = std::iter::successors(Some(id), |step| self.parent(*step)).collect();
        steps.reverse();
        steps
    }
}
