#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
    echo "Usage: scripts/prune-target.sh [--dry-run]"
    echo "Removes what earlier versions left in target/: notchbase's own build output and the"
    echo "incremental caches that have not been written since Cargo.toml last changed, and"
    echo "build output of the other workspace crates that has not been rebuilt for a week."
}

dry=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) dry=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 1 ;;
    esac
done

cargo metadata --no-deps --format-version 1 --offline | python3 -c '
import fcntl, json, os, re, shutil, sys, time

dry = bool(sys.argv[1])
meta = json.load(sys.stdin)
target = meta["target_directory"]
manifest = os.path.join(meta["workspace_root"], "Cargo.toml")
changed = os.lstat(manifest).st_mtime
members = meta["packages"]
with open(os.path.join(meta["workspace_root"], "Cargo.lock")) as lock:
    foreign = re.findall(r"name = \"([^\"]+)\"\nversion = \"[^\"]+\"\nsource = ", lock.read())
foreign = {name.replace("-", "_") for name in foreign}


def hashed(names):
    return re.compile(r"^(?:lib)?(?:%s)-([0-9a-f]{16})(?:\..*)?$" % "|".join(map(re.escape, sorted(names))))


def rule(packages, cutoff):
    built = {t["name"] for p in packages for t in p["targets"] if "custom-build" not in t["kind"]}
    built = {name for name in built if name.replace("-", "_") not in foreign}
    built |= {name.replace("-", "_") for name in built}
    owners = {p["name"] for p in packages}
    return cutoff, {"deps": hashed(built), "examples": hashed(built), ".fingerprint": hashed(owners), "build": hashed(owners)}


rules = [
    rule([p for p in members if p["manifest_path"] == manifest], changed),
    rule([p for p in members if p["manifest_path"] != manifest], time.time() - 7 * 86400),
]


def newest(path):
    stamps = [os.lstat(path).st_mtime]
    if os.path.isdir(path) and not os.path.islink(path):
        stamps += [os.lstat(os.path.join(path, name)).st_mtime for name in os.listdir(path)]
    return max(stamps)


def stale(profile):
    units = {}
    for cutoff, folders in rules:
        for folder, pattern in folders.items():
            base = os.path.join(profile, folder)
            if os.path.isdir(base):
                for name in os.listdir(base):
                    found = pattern.match(name)
                    if found:
                        units.setdefault((cutoff, folder, found.group(1)), []).append(os.path.join(base, name))
    base = os.path.join(profile, "incremental")
    if os.path.isdir(base):
        for name in os.listdir(base):
            units[(changed, "incremental", name)] = [os.path.join(base, name)]
    return [path for key, paths in units.items() if max(map(newest, paths)) < key[0] for path in paths]


def size(path, seen):
    files = [path]
    if os.path.isdir(path) and not os.path.islink(path):
        files = [os.path.join(folder, name) for folder, _, names in os.walk(path) for name in names]
    total = 0
    for file in files:
        info = os.lstat(file)
        if (info.st_dev, info.st_ino) not in seen:
            seen.add((info.st_dev, info.st_ino))
            total += info.st_blocks * 512
    return total


def readable(count):
    return "%.1f GB" % (count / 1e9) if count >= 1e9 else "%.0f MB" % (count / 1e6)


profiles = []
if os.path.isdir(target):
    for top in sorted(os.listdir(target)):
        for path in [os.path.join(target, top)] + [os.path.join(target, top, name) for name in ("debug", "release")]:
            if os.path.isdir(os.path.join(path, ".fingerprint")):
                profiles.append(path)

freed = 0
seen = set()
for profile in profiles:
    shown = os.path.relpath(profile)
    lock = os.path.join(profile, ".cargo-lock")
    held = open(lock) if os.path.exists(lock) else None
    if held:
        try:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            print("Left %s alone: a build is using it" % shown)
            continue
    paths = stale(profile)
    if paths:
        here = sum(size(path, seen) for path in paths)
        freed += here
        if not dry:
            for path in paths:
                if os.path.isdir(path) and not os.path.islink(path):
                    shutil.rmtree(path)
                else:
                    os.remove(path)
        print("%s %s: %s" % ("Would prune" if dry else "Pruned", shown, readable(here)))
    if held:
        held.close()

if freed:
    print("%s %s in target/" % ("Would free" if dry else "Freed", readable(freed)))
else:
    print("Nothing to prune in target/")
' "$dry"
