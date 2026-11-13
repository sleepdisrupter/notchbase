//! What Find indexes. The plan knows the roots, the exclusions, the sealed system folders and the folders privacy
//! guards, and gives every entry a verdict and a tier: user files, system, builds or waste. When memory runs short,
//! higher levels hold back waste, then builds, then other drives, then the system.

mod order;

use std::path::{Path, PathBuf};

use crate::config::FindConfig;
use crate::find::os::{self, Root, raw, root};
use crate::stats::disk::scan::buildable;

use super::rules::{
    CARGO_OUTPUT, Exclusion, HOME_SYSTEM, SYSTEM_NAMES, SYSTEM_PLACES, VERSIONED, WASTE, WASTE_BELOW_SYSTEM,
    WASTE_PLACES, alike, backup, exclusion, listed, media, package, parts, placed, same, stormy, toolchain, under,
};

pub(super) use order::RANKS;

pub const MAX_DEPTH: usize = 64;
const SYSTEM_TIER: u8 = 1;
const BUILD_TIER: u8 = 2;
const WASTE_TIER: u8 = 3;
const DROP_WASTE: u8 = 1;
const DROP_BUILDS: u8 = 2;
const DROP_OTHER_VOLUMES: u8 = 3;
const DROP_SYSTEM: u8 = 4;

/// What to do with one entry of a listing: descend into it at a tier, keep it as a leaf, or leave it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Descend(u8),
    Leaf,
    Skip,
}

/// The fixed places the platform names: sealed system folders, the doors through them, the folders privacy
/// guards and where apps live.
#[derive(Clone, Debug, Default)]
pub struct Fences {
    pub sealed: Vec<PathBuf>,
    pub open: Vec<PathBuf>,
    pub guarded: Vec<PathBuf>,
    pub apps: Vec<PathBuf>,
}

pub struct Plan {
    roots: Vec<Root>,
    home: Option<PathBuf>,
    shut: Vec<PathBuf>,
    shut_names: Vec<String>,
    walls: Vec<(PathBuf, Option<usize>)>,
    doors: Vec<PathBuf>,
    guarded: Vec<PathBuf>,
    apps: Vec<PathBuf>,
    fussy: Vec<PathBuf>,
    limited: bool,
    resting: bool,
}

enum Standing {
    Open,
    Shut,
    Out,
}

impl Plan {
    pub fn new(
        config: &FindConfig,
        roots: &[Root],
        home: Option<&Path>,
        data: Option<&Path>,
        full_access: bool,
    ) -> Self {
        let guarded = home.map(os::guarded).unwrap_or_default();
        let fences = Fences { sealed: os::sealed(), open: os::unsealed(), guarded, apps: os::launchers(home) };
        Self::fenced(config, roots, home, data, full_access, fences)
    }

    pub fn fenced(
        config: &FindConfig,
        roots: &[Root],
        home: Option<&Path>,
        data: Option<&Path>,
        full_access: bool,
        fences: Fences,
    ) -> Self {
        let mut shut: Vec<PathBuf> = data.map(Path::to_path_buf).into_iter().collect();
        let mut shut_names = Vec::new();
        for excluded in config.excluded.iter().filter_map(|entry| exclusion(entry, home)) {
            match excluded {
                Exclusion::Below(path) => shut.push(path),
                Exclusion::Named(name) => shut_names.push(name),
            }
        }
        let walled = |path: &&Path| fences.sealed.iter().any(|wall| under(path, wall));
        let sieves = fences.open.iter().flat_map(|door| door.ancestors().skip(1)).filter(walled);
        let beside_shut = shut.iter().filter_map(|path| path.parent());
        let fussy = sieves.chain(beside_shut).map(Path::to_path_buf).collect();
        let (roots, home) = (roots.to_vec(), home.map(Path::to_path_buf));
        let Fences { sealed, open: doors, mut guarded, apps } = fences;
        if !full_access && home.is_none() {
            guarded.extend(roots.iter().map(|root| root.path.clone()));
        }
        let limited = !full_access;
        let walls = Vec::new();
        let mut plan =
            Self { roots, home, shut, shut_names, walls, doors, guarded, apps, fussy, limited, resting: false };
        let walls = sealed.into_iter().map(|wall| (plan.root_at(&wall), wall));
        plan.walls = walls.map(|(root, wall)| (wall, root)).collect();
        plan
    }

    pub fn resting() -> Self {
        let plan = Self::fenced(&FindConfig::default(), &[], None, None, false, Fences::default());
        Self { resting: true, ..plan }
    }

    pub fn verdict(&self, parent: &Path, parent_tier: u8, name: &str, bits: u8, depth: usize) -> Verdict {
        let folder = bits & raw::DIR != 0 && bits & raw::LINK == 0;
        if self.shut_names.iter().any(|shut| alike(shut, name)) {
            return Verdict::Skip;
        }
        if !folder && !self.fussy.iter().any(|fussy| fussy.as_os_str() == parent.as_os_str()) {
            return Verdict::Leaf;
        }
        let path = parent.join(name);
        let closed = bits & (raw::DATALESS | raw::ELSEWHERE) != 0 || depth >= MAX_DEPTH;
        match self.standing(&path, parent, name) {
            Standing::Out => Verdict::Skip,
            Standing::Open if folder && !closed => Verdict::Descend(self.tier(parent, parent_tier, name, &path)),
            Standing::Open | Standing::Shut => Verdict::Leaf,
        }
    }

    pub fn wanted(&self, path: &Path) -> bool {
        let root = self.root_at(path);
        let (boots, removable) = (self.boots(root), self.flagged(root, root::REMOVABLE));
        let named = |test: &dyn Fn(&str) -> bool| parts(path).any(|part| test(&part));
        let unrooted = root.is_none() && !self.roots.is_empty();
        let excluded = self.shut.iter().any(|shut| under(path, shut))
            || named(&|part| self.shut_names.iter().any(|shut| alike(shut, part)));
        let sealed = self.wall(path, root).is_some() && !self.passes(path);
        let guarded = self.limited && (removable || self.guards(path));
        let top = root.filter(|_| !boots).and_then(|root| parts(path).nth(self.roots[root].path.components().count()));
        !(self.resting || unrooted || excluded || sealed || guarded || top.is_some_and(|top| backup(&top)))
    }

    pub fn guarded(&self, path: &Path) -> bool {
        self.limited && (self.flagged(self.root_at(path), root::REMOVABLE) || self.guards(path))
    }

    fn guards(&self, path: &Path) -> bool {
        self.guarded.iter().any(|guard| under(path, guard)) || parts(path).any(|part| media(&part))
    }

    pub fn noisy(&self, path: &Path) -> bool {
        parts(path).any(|part| stormy(&part))
    }

    pub fn held_back(&self, level: u8, dir: &Path, tier: u8) -> bool {
        let on_another_volume = || !self.boots(self.root_at(dir));
        (level >= DROP_WASTE && tier >= WASTE_TIER)
            || (level >= DROP_BUILDS && tier >= BUILD_TIER)
            || (level >= DROP_SYSTEM && tier >= SYSTEM_TIER)
            || (level >= DROP_OTHER_VOLUMES && on_another_volume())
    }

    pub fn devices(&self, dir: &Path) -> [u64; 2] {
        self.root_at(dir).map_or([0, 0], |root| self.roots[root].devices)
    }

    pub fn inside(&self, dir: &Path) -> u8 {
        let rooted = self.root_at(dir).map_or(0, |root| self.roots[root].path.components().count());
        let (mut parent, mut tier) = (PathBuf::new(), 0);
        for (index, part) in dir.components().enumerate() {
            let path = parent.join(part);
            if index >= rooted {
                tier = self.tier(&parent, tier, &part.as_os_str().to_string_lossy(), &path);
            }
            parent = path;
        }
        tier
    }

    pub fn packaged(&self, dir: &Path) -> bool {
        parts(dir).any(|part| package(&part))
    }

    fn standing(&self, path: &Path, parent: &Path, name: &str) -> Standing {
        if self.shut.iter().any(|shut| under(path, shut)) {
            return Standing::Out;
        }
        let root = self.root_at(path);
        if let Some(wall) = self.wall(path, root)
            && !self.passes(path)
        {
            return if same(path, wall) { Standing::Shut } else { Standing::Out };
        }
        let guarded = self.limited && (media(name) || self.guarded.iter().any(|guard| under(path, guard)));
        let backed_up =
            backup(name) && root.is_some_and(|root| !self.boots(Some(root)) && same(parent, &self.roots[root].path));
        if guarded || backed_up { Standing::Shut } else { Standing::Open }
    }

    fn tier(&self, parent: &Path, parent_tier: u8, name: &str, path: &Path) -> u8 {
        let at_home = || self.home.as_deref().is_some_and(|home| same(parent, home));
        let waste = listed(&WASTE, name)
            || (parent_tier >= SYSTEM_TIER && listed(&WASTE_BELOW_SYSTEM, name))
            || placed(path, &WASTE_PLACES);
        let cargo = || !alike(name, CARGO_OUTPUT) || self.wanted(parent);
        let built = || alike(name, VERSIONED) || (toolchain(name) && at_home()) || (cargo() && buildable(name, parent));
        let system = || {
            listed(&SYSTEM_NAMES, name)
                || package(name)
                || placed(path, &SYSTEM_PLACES)
                || (alike(name, HOME_SYSTEM) && at_home())
        };
        let floor = if waste {
            WASTE_TIER
        } else if built() {
            BUILD_TIER
        } else if system() {
            SYSTEM_TIER
        } else {
            0
        };
        parent_tier.max(floor)
    }

    fn wall(&self, path: &Path, root: Option<usize>) -> Option<&Path> {
        let walls = self.walls.iter().filter(|(_, owner)| *owner == root);
        walls.map(|(wall, _)| wall.as_path()).find(|wall| under(path, wall))
    }

    fn passes(&self, path: &Path) -> bool {
        self.doors.iter().any(|door| under(path, door) || under(door, path))
    }

    fn boots(&self, root: Option<usize>) -> bool {
        root.is_none_or(|root| self.roots[root].flags & root::BOOT != 0)
    }

    fn flagged(&self, root: Option<usize>, flag: u8) -> bool {
        root.is_some_and(|root| self.roots[root].flags & flag != 0)
    }

    fn root_at(&self, path: &Path) -> Option<usize> {
        let holding = self.roots.iter().enumerate().filter(|(_, root)| under(path, &root.path));
        holding.max_by_key(|(_, root)| root.path.components().count()).map(|(index, _)| index)
    }
}
