//! Links the icon into the Windows executable.

use std::path::Path;

const ICON: &str = "assets/icon/notchbase.ico";
const RESOURCE: &str = "notchbase.res";

fn main() {
    notchbase_build::embed_icon(Path::new(ICON), RESOURCE);
}
