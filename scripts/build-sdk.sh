#!/bin/sh
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
stage=$(mktemp -d)
kit="$stage/notchbase-tools"
zip="$root/Distributable/notchbase-sdk.zip"

mkdir -p "$kit/notchbase-sdk" "$kit/notes" "$kit/docs"
cp "$root/sdk/Cargo.toml" "$kit/notchbase-sdk/"
cp -R "$root/sdk/src" "$kit/notchbase-sdk/"
cp "$root/sdk/KIT.md" "$kit/README.md"

cp -R "$root/tools/notes/src" "$kit/notes/"
grep -q 'path = "../../sdk"' "$root/tools/notes/Cargo.toml" || { echo "The notes example no longer depends on ../../sdk" >&2; exit 1; }
sed 's|path = "../../sdk"|path = "../notchbase-sdk"|' "$root/tools/notes/Cargo.toml" > "$kit/notes/Cargo.toml"
printf '\n[workspace]\n' >> "$kit/notes/Cargo.toml"

cp "$root"/wiki/tools/*.md "$kit/docs/"
cp "$root/skill/notchbase/scripts/install.ps1" "$root/skill/notchbase/scripts/install.sh" "$kit/"

mkdir -p "$root/Distributable"
rm -f "$zip"
(cd "$stage" && zip -qr "$zip" notchbase-tools)
rm -rf "$stage"
echo "Built $zip"
