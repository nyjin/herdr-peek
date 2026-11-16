#!/bin/sh
# semantic-release's prepare step (.releaserc.json): write the release version into Cargo.toml and herdr-plugin.toml,
# build the release binary (which also updates Cargo.lock) and put it with its SHA-256 in dist/ for the GitHub release.
set -eu
version="${1:-}"
case "$version" in
  "" | *[!0-9A-Za-z.+-]*) echo "usage: scripts/set-version.sh <version>" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/.."
for file in Cargo.toml herdr-plugin.toml; do
  # the first `version = "..."` line: the package's in Cargo.toml, the plugin's in herdr-plugin.toml
  awk -v v="$version" '!done && /^version = "/ { print "version = \"" v "\""; done = 1; next } { print }' "$file" > "$file.tmp"
  mv "$file.tmp" "$file"
done
[ "$(uname -s)-$(uname -m)" = Darwin-arm64 ] || { echo "release builds run on macOS arm64" >&2; exit 1; }
cargo build --release
mkdir -p dist
cp target/release/peek dist/peek-macos-aarch64
(cd dist && shasum -a 256 peek-macos-aarch64 > peek-macos-aarch64.sha256)
