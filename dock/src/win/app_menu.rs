//! An app's own actions for its dock menu. macOS reads them from the app's menu bar; Windows apps have no shared
//! menu bar to read, so their menus show the dock's own items only.

/// One action from an app's menu bar.
#[derive(Clone, Debug, PartialEq)]
pub struct AppItem {
    pub menu: String,
    pub title: String,
    pub activates: bool,
}

pub fn press(_pid: i32, _item: AppItem) {}

#[derive(Default)]
pub struct Reader;

impl Reader {
    pub fn new() -> Self {
        Self
    }

    pub fn want(&mut self, _pid: i32) {}

    pub fn collect(&mut self) -> Vec<i32> {
        Vec::new()
    }

    pub fn get(&self, _pid: i32) -> &[AppItem] {
        &[]
    }
}
