#!/usr/bin/env bash
# Builds the Mac release and publishes it as a GitHub release: both app builds, the installer, the SDK and skill
# zips, and the latest.json that installed copies read to find it. Needs the GitHub CLI (gh), logged in.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
    echo "Usage: scripts/release-github.sh [--notes <text>] [--target <triple>]... [--dry-run]"
    echo "Targets: aarch64-apple-darwin, x86_64-apple-darwin (both by default)."
    echo "Releases to NOTCHBASE_RELEASES_REPO, else the GitHub repository of the origin remote, else $OFFICIAL."
}

OFFICIAL="sleepdisrupter/notchbase"
targets=()
notes=""
dry=false
while [ $# -gt 0 ]; do
    case "$1" in
        --notes) notes="$2"; shift 2 ;;
        --target) targets+=("$2"); shift 2 ;;
        --dry-run) dry=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 1 ;;
    esac
done

fail() {
    echo "$1" >&2
    exit 1
}

[ "$(uname -s)" = "Darwin" ] || fail "Build the Mac release on a Mac. Release Windows with scripts\\release-github.bat."
[ ${#targets[@]} -gt 0 ] || targets=(aarch64-apple-darwin x86_64-apple-darwin)

origin_repo() {
    git remote get-url origin 2>/dev/null | sed -nE 's#^(https://github\.com/|git@github\.com:|ssh://git@github\.com/)([^/]+/[^/]+)$#\2#p' |
        sed 's/\.git$//'
}

repo="${NOTCHBASE_RELEASES_REPO:-}"
[ -n "$repo" ] || repo="$(origin_repo)"
[ -n "$repo" ] || repo="$OFFICIAL"
# The app and the installer read their updates from this repository.
export NOTCHBASE_RELEASES_REPO="$repo"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag="v$version"
download="https://github.com/$repo/releases/download/$tag"

if ! $dry; then
    command -v gh >/dev/null || fail "Install the GitHub CLI (https://cli.github.com) and run gh auth login first."
    gh auth status >/dev/null 2>&1 || fail "Log in to GitHub first: gh auth login"
fi

# Releases of notchbase itself must be signed with the pinned certificate, or every Mac user loses their
# permissions on update. A fork signs with NOTCHBASE_SIGN_IDENTITY, or ad hoc.
signer=$(tr -d '[:space:]' < macos/signing-certificate.sha1 | tr 'A-F' 'a-f')
if [ "$repo" = "$OFFICIAL" ]; then
    export NOTCHBASE_SIGN_IDENTITY="${NOTCHBASE_SIGN_IDENTITY:-notchbase Local Signing}"
elif [ -z "${NOTCHBASE_SIGN_IDENTITY:-}" ]; then
    echo "Warning: NOTCHBASE_SIGN_IDENTITY isn't set, so this release is signed ad hoc." >&2
    echo "macOS will ask for every permission again after each update. See wiki/Platform-macOS.md." >&2
    export NOTCHBASE_SIGN_IDENTITY="-"
fi
signed_by_release_certificate() {
    codesign -d -r- "$1" 2>/dev/null | grep -q "certificate leaf = H\"$signer\""
}

stage="target/github-release/$tag"
rm -rf "$stage"
mkdir -p "$stage"
assets=()
builds=()

for target in "${targets[@]}"; do
    case "$target" in
        *-apple-darwin) platform="macos-${target%%-*}" ;;
        *) fail "This script releases Mac targets only, not $target." ;;
    esac
    rustup target add "$target" >/dev/null
    ./scripts/bundle-macos.sh "$target"
    app="target/$target/release/notchbase.app"
    if [ "$repo" = "$OFFICIAL" ]; then
        for bundle in "$app" "$app/Contents/Helpers/notchbase dock.app"; do
            signed_by_release_certificate "$bundle" ||
                fail "Not releasing: $bundle isn't signed with the release certificate ($signer)."
        done
    fi
    name="notchbase-$version-$platform.zip"
    ditto -c -k --keepParent "$app" "$stage/$name"
    assets+=("$stage/$name")
    builds+=("$platform" "$name")
done

./build-installer.sh
cp dist/notchbase-setup.dmg "$stage/notchbase-setup.dmg"
sh scripts/build-sdk.sh
sh scripts/build-skill.sh
cp Distributable/notchbase-sdk.zip Distributable/notchbase-skill.zip "$stage/"
assets+=("$stage/notchbase-setup.dmg" "$stage/notchbase-sdk.zip" "$stage/notchbase-skill.zip")

# A release that already exists (Windows went first) keeps its other platforms in latest.json.
existing=false
if ! $dry && gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
    existing=true
    gh release download "$tag" --repo "$repo" --pattern latest.json --dir "$stage/previous" 2>/dev/null || true
fi

python3 - "$stage" "$version" "$notes" "$download" "${builds[@]}" <<'PY'
import hashlib, json, os, sys

stage, version, notes, download, *pairs = sys.argv[1:]
previous = os.path.join(stage, "previous", "latest.json")
manifest = {"version": version, "notes": notes, "builds": {}}
if os.path.exists(previous):
    with open(previous) as f:
        old = json.load(f)
    if old.get("version") == version:
        manifest["builds"] = old.get("builds", {})
        manifest["notes"] = notes or old.get("notes", "")
for platform, name in zip(pairs[::2], pairs[1::2]):
    path = os.path.join(stage, name)
    with open(path, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    manifest["builds"][platform] = {"url": f"{download}/{name}", "sha256": digest, "size": os.path.getsize(path)}
    print(f"{platform}: {name}, {os.path.getsize(path)} bytes, sha256 {digest}")
with open(os.path.join(stage, "latest.json"), "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY
assets+=("$stage/latest.json")

echo "Release $tag for $repo:"
for asset in "${assets[@]}"; do
    echo "  $(basename "$asset")"
done
if $dry; then
    echo "Dry run: nothing was uploaded. The files are in $stage."
    exit 0
fi

if $existing; then
    gh release upload "$tag" --repo "$repo" --clobber "${assets[@]}"
else
    gh release create "$tag" --repo "$repo" --title "notchbase $version" --notes "${notes:-notchbase $version}" \
        --latest "${assets[@]}"
fi
echo "Published https://github.com/$repo/releases/tag/$tag"
