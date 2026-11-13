#!/bin/sh
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
stage=$(mktemp -d)
skill="$stage/notchbase"
zip="$root/Distributable/notchbase-skill.zip"

cp -R "$root/skill/notchbase" "$skill"
mkdir -p "$skill/references/tools"
cp "$root"/wiki/*.md "$skill/references/"
cp "$root"/wiki/tools/*.md "$skill/references/tools/"

example="$skill/references/tools/example-notes.md"
{
    printf '# Example: the Notes plugin\n\n'
    printf 'A complete plugin from the notchbase repository (tools/notes): a page with a text field and a list, pinning a note as a live activity, a folder setting shown only while its toggle is on, reading the notch state, holding the notch open, writing a file in the background, and full Harness tests.\n\n'
    for file in Cargo.toml $(cd "$root/tools/notes" && ls src/*.rs | sort); do
        case "$file" in *.toml) language=toml ;; *) language=rust ;; esac
        printf '## %s\n\n```%s\n' "$file" "$language"
        cat "$root/tools/notes/$file"
        printf '```\n\n'
    done
} > "$example"

mkdir -p "$root/Distributable"
rm -f "$zip"
(cd "$stage" && zip -qr "$zip" notchbase)
rm -rf "$stage"
echo "Built $zip"
