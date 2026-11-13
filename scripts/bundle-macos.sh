#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

target="${1:-}"
if [ -n "$target" ]; then
    MACOSX_DEPLOYMENT_TARGET=14.2 cargo build --release --target "$target"
    out="target/$target/release"
else
    MACOSX_DEPLOYMENT_TARGET=14.2 cargo build --release
    out="target/release"
fi

app="$out/notchbase.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$out/notchbase" "$app/Contents/MacOS/notchbase"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
sed "/<key>CFBundleShortVersionString<\/key>/{n;s/<string>[^<]*<\/string>/<string>$version<\/string>/;}" macos/Info.plist > "$app/Contents/Info.plist"
cp macos/notchbase.icns "$app/Contents/Resources/notchbase.icns"

# notchbase dock rides inside, in Contents/Helpers, signed first with the same identity so the outer signature seals it.
# notchbase starts it from there once someone turns it on in Settings > Dock.
./scripts/bundle-dock-macos.sh ${target:+"$target"} >/dev/null
mkdir -p "$app/Contents/Helpers"
ditto "$out/notchbase dock.app" "$app/Contents/Helpers/notchbase dock.app"

identity="${NOTCHBASE_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
    found=$(security find-identity -p codesigning || true)
    case "$found" in *'"notchbase Local Signing"'*) identity="notchbase Local Signing" ;; esac
fi
codesign --force --sign "${identity:--}" --timestamp=none "$app"
if [ -z "$identity" ]; then
    echo "Signed ad hoc: macOS forgets the app's permissions after every build (see wiki/Platform-macOS.md)." >&2
fi

echo "Built $app"
echo "Run it with: open \"$app\""
