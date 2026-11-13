//! Links the icon into the Windows executable, and on Windows carries notchbase dock inside it when the release build
//! names one in `NOTCHBASE_BUNDLED_DOCK`.

use std::path::Path;

const ICON: &str = "assets/icon/notchbase.ico";
const RESOURCE: &str = "notchbase.res";
const BUNDLED_DOCK: &str = "NOTCHBASE_BUNDLED_DOCK";

fn main() {
    notchbase_build::embed_icon(Path::new(ICON), RESOURCE);
    bundle_dock();
}

/// `cfg(bundled_dock)` when `NOTCHBASE_BUNDLED_DOCK` names a dock exe to carry.
fn bundle_dock() {
    println!("cargo::rustc-check-cfg=cfg(bundled_dock)");
    println!("cargo::rerun-if-env-changed={BUNDLED_DOCK}");
    let Some(path) = std::env::var_os(BUNDLED_DOCK).filter(|p| !p.is_empty()) else { return };
    let path = Path::new(&path);
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert!(path.is_file(), "{BUNDLED_DOCK} names {}, which isn't a file", path.display());
    let path = std::fs::canonicalize(path).expect("the dock exe's full path");
    println!("cargo::rerun-if-changed={}", path.display());
    println!("cargo::rustc-env={BUNDLED_DOCK}={}", path.display());
    println!("cargo::rustc-cfg=bundled_dock");
}
