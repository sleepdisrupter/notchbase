# notchbase-build

A helper for build scripts. The app's `build.rs` and the installer's `setup/build.rs` both call it, so the two binaries get their icon the same way.

- `embed_icon(icon, res_name)` turns an `.ico` into a Windows resource and links it in, so Explorer and the taskbar show the icon. It only does anything when building for Windows with MSVC.

```rust
use std::path::Path;

fn main() {
    notchbase_build::embed_icon(Path::new("assets/icon/notchbase.ico"), "notchbase.res");
}
```

Run the tests with `cargo test -p notchbase-build`.
