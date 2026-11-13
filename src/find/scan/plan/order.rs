//! The order the scan takes: folders on the way to the apps first, then the home folder, then everything else.

use std::path::{Path, PathBuf};

use crate::find::scan::rules::under;

use super::Plan;

pub(in crate::find::scan) const RANKS: usize = 3;
const APPLICATIONS_RANK: usize = 0;
const HOME_RANK: usize = 1;
const OTHER_RANK: usize = 2;

impl Plan {
    pub fn leads_to_apps(&self, dir: &Path) -> bool {
        self.apps.iter().any(|place| under(place, dir))
    }

    pub fn app_places(&self) -> impl Iterator<Item = &Path> {
        self.apps.iter().chain(&self.doors).map(PathBuf::as_path)
    }

    pub(in crate::find::scan) fn rank(&self, dir: &Path) -> usize {
        let on_the_way = |place: &Path| under(dir, place) || under(place, dir);
        match &self.home {
            _ if self.apps.iter().any(|place| on_the_way(place)) => APPLICATIONS_RANK,
            Some(home) if on_the_way(home) => HOME_RANK,
            _ => OTHER_RANK,
        }
    }

    pub(in crate::find::scan) fn ahead(&self, dir: &Path) -> bool {
        let among_apps = || self.apps.iter().any(|place| under(dir, place)) && !self.packaged(dir);
        self.leads_to_apps(dir) || among_apps()
    }
}
