//! Other systems have no GPU reading. The disk is the volume mounted at /.

use std::path::Path;

use sysinfo::{DiskRefreshKind, Disks};

use super::Disk;

pub(super) enum Gpu {}

impl Gpu {
    pub(super) fn open() -> Option<Self> {
        None
    }

    pub(super) fn sample(&mut self) -> Option<f32> {
        match *self {}
    }
}

pub(super) fn gpu_name() -> Option<String> {
    None
}

pub(super) fn system_disk() -> Option<Disk> {
    let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());
    let root = disks.list().iter().find(|disk| disk.mount_point() == Path::new("/"))?;
    let total = root.total_space();
    (total > 0).then(|| Disk { free: root.available_space().min(total), total })
}
