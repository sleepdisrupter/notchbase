#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Build the Mac installer on a Mac. On Windows, run build-installer.bat." >&2
    exit 1
fi

targets=(aarch64-apple-darwin x86_64-apple-darwin)
for target in "${targets[@]}"; do
    rustup target add "$target" >/dev/null
    MACOSX_DEPLOYMENT_TARGET=14.2 cargo build --release -p notchbase-setup --target "$target"
done

app="dist/Install notchbase.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/notchbase-setup" \
    target/aarch64-apple-darwin/release/notchbase-setup \
    target/x86_64-apple-darwin/release/notchbase-setup
cp macos/Setup-Info.plist "$app/Contents/Info.plist"
cp macos/notchbase.icns "$app/Contents/Resources/notchbase.icns"
codesign --force --sign "${NOTCHBASE_SIGN_IDENTITY:--}" --timestamp=none "$app"

dmg="dist/notchbase-setup.dmg"
rm -f "$dmg"
hdiutil create -volname "Install notchbase" -srcfolder "$app" -ov -format UDZO "$dmg" >/dev/null

echo "Built $dmg"
echo "It downloads and installs the newest published notchbase for the Mac it runs on."
