#!/bin/sh
set -e
usage='Usage: install.sh [--native] [<tool folder>]'
native=0
folder=.
while [ $# -gt 0 ]; do
    case "$1" in
        --native) native=1 ;;
        -*)
            echo "install.sh doesn't take $1. $usage" >&2
            exit 2 ;;
        *) folder=$1 ;;
    esac
    shift
done

[ -d "$folder" ] || { echo "There is no folder named $folder here. Run this from the notchbase-tools folder with the name of the tool's folder. $usage" >&2; exit 1; }
cd "$folder"
[ -f Cargo.toml ] || { echo "$(pwd) has no Cargo.toml, so it isn't a tool. Name the tool's own folder. $usage" >&2; exit 1; }
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
fi
command -v cargo >/dev/null 2>&1 || { echo "Rust isn't installed (there is no cargo). Get it from https://rustup.rs, open a new terminal and run this again." >&2; exit 1; }

name=$(sed -n 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
[ -n "$name" ] || { echo "Cargo.toml has no package name" >&2; exit 1; }
crate=$(printf '%s' "$name" | tr '-' '_')

case "$(uname -s)" in
    Darwin) library="lib$crate.dylib"; data="$HOME/Library/Application Support/notchbase" ;;
    *) library="lib$crate.so"; data="${XDG_DATA_HOME:-$HOME/.local/share}/notchbase" ;;
esac
portable="$crate.wasm"
target="${CARGO_TARGET_DIR:-target}"

if [ "$native" = 1 ]; then
    cargo build --release || { echo "The build failed: the first error above says where. Nothing was installed." >&2; exit 1; }
    built="$target/release/$library"
    kept="$library"
    other="$portable"
else
    rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
    cargo build --release --target wasm32-unknown-unknown || {
        echo "The portable build failed: the first error above says where. Nothing was installed." >&2
        echo "If it says the wasm32-unknown-unknown target may not be installed, run: rustup target add wasm32-unknown-unknown" >&2
        echo "If the error is inside a crate the tool depends on, that crate can't be built portable: add --native." >&2
        exit 1
    }
    built="$target/wasm32-unknown-unknown/release/$portable"
    kept="$portable"
    other="$library"
fi
[ -f "$built" ] || { echo "Couldn't find $built. Is crate-type set to cdylib?" >&2; exit 1; }

tools="${NOTCHBASE_TOOLS_DIR:-$data/tools}"
folder="$tools/$name"
fresh=0
[ -n "${NOTCHBASE_TOOLS_DIR:-}" ] || [ -d "$data" ] || fresh=1
mkdir -p "$folder"
rm -f "${folder:?}/${other:?}"
cp "$built" "$folder/$kept"
echo "Installed $name into $folder as $kept"
if [ "$fresh" = 1 ]; then
    echo "notchbase hasn't run on this computer yet. Start it and the tool is there."
else
    echo "notchbase picks it up within a second. Look for 'Reloaded $name' in the notch."
fi
