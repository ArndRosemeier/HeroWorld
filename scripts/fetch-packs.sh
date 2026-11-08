#!/usr/bin/env bash
# Downloads the Actors packs (bodies, clothes, hair, clips) that people are
# built from into assets/packs. The version is in assets/actors-packs.version;
# nothing happens if that version is already there. The packs are large
# (~440 MB) and never committed.
set -euo pipefail
cd "$(dirname "$0")/.."
version="$(tr -d '[:space:]' < assets/actors-packs.version)"
dest=assets/packs
if [ -f "$dest/.version" ] && [ "$(cat "$dest/.version")" = "$version" ]; then
  exit 0
fi
url="https://github.com/ArndRosemeier/Actors/releases/download/v$version/actors-packs-$version.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading Actors packs $version (about 440 MB, once)..."
curl -fL --retry 4 -o "$tmp/packs.tar.gz" "$url"
mkdir -p "$tmp/x"
tar -xzf "$tmp/packs.tar.gz" -C "$tmp/x"
rm -rf "$dest"
mv "$tmp/x/packs" "$dest"
echo "$version" > "$dest/.version"
echo "Actors packs $version are in $dest"
