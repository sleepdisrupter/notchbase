//! The installer's build script: links the icon into the Windows executable, the same way the app's does.

use std::path::Path;

const ICON: &str = "../assets/icon/notchbase.ico";
const RESOURCE: &str = "setup.res";

fn main() {
    notchbase_build::embed_icon(Path::new(ICON), RESOURCE);
}
