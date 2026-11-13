#!/usr/bin/env bash
# Builds "notchbase dock.app" from the dock/ crate: an accessory app (no Dock icon of its own), signed like
# notchbase is. Pass a target triple to build for another architecture.
set -euo pipefail
cd "$(dirname "$0")/.."

target="${1:-}"
if [ -n "$target" ]; then
    MACOSX_DEPLOYMENT_TARGET=14.2 cargo build --release -p notchbase-dock --target "$target"
    out="target/$target/release"
else
    MACOSX_DEPLOYMENT_TARGET=14.2 cargo build --release -p notchbase-dock
    out="target/release"
fi

app="$out/notchbase dock.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$out/notchbase-dock" "$app/Contents/MacOS/notchbase-dock"
cp macos/notchbase.icns "$app/Contents/Resources/notchbase.icns"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' dock/Cargo.toml | head -1)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>notchbase dock</string>
    <key>CFBundleDisplayName</key>
    <string>notchbase dock</string>
    <key>CFBundleIdentifier</key>
    <string>app.notchbase.dock</string>
    <key>CFBundleExecutable</key>
    <string>notchbase-dock</string>
    <key>CFBundleIconFile</key>
    <string>notchbase.icns</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.2</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSAppleEventsUsageDescription</key>
    <string>notchbase dock asks Finder to empty the Trash when you choose Empty Trash.</string>
</dict>
</plist>
PLIST

identity="${NOTCHBASE_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
    found=$(security find-identity -p codesigning || true)
    case "$found" in *'"notchbase Local Signing"'*) identity="notchbase Local Signing" ;; esac
fi
codesign --force --sign "${identity:--}" --timestamp=none "$app"

echo "Built $app"
echo "Run it with: open \"$app\"   Quit it with: \"$app/Contents/MacOS/notchbase-dock\" --quit"
