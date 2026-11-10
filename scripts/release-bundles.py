#!/usr/bin/env python3
"""Pack the Windows build into update bundles and write manifest.json.

The launcher reads manifest.json from the release and downloads only the
bundles whose files changed: most nights that is just "game" (the exes),
while the people packs, robots and music stay as they are.

Usage: release-bundles.py <staged game folder> <launcher exe> <out dir>

The staged folder is what the zip holds (heroworld.exe, assets/...). Each
bundle is a zip of some of its files, with paths relative to the install
folder. A bundle's "content" hash covers the files inside it (paths, sizes,
SHA-256), not the zip bytes, so a rebuilt zip of the same files keeps it.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import zipfile
from datetime import datetime, timezone

# Which bundle a file goes in: the first prefix that matches wins.
BUNDLES = [
    ("packs", "assets/packs/"),
    ("robots", "assets/robots/"),
    ("music", "assets/music/"),
    ("game", ""),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def content_hash(files):
    """The same rule as the launcher's `content_hash`."""
    h = hashlib.sha256()
    for f in sorted(files, key=lambda f: f["path"]):
        h.update(f"{f['path']}\t{f['size']}\t{f['sha256']}\n".encode())
    return h.hexdigest()


def main():
    src, launcher, out = sys.argv[1:4]
    root = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.dirname(root)
    os.makedirs(out, exist_ok=True)
    version = open(os.path.join(repo, "VERSION")).read().strip()
    whats_new = open(os.path.join(repo, "WHATS_NEW.md"), encoding="utf-8").read()
    toml = open(os.path.join(repo, "crates/hw-launcher/Cargo.toml")).read()
    launcher_version = re.search(r'^version = "([^"]+)"', toml, re.M).group(1)
    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    except Exception:
        commit = ""

    groups = {name: [] for name, _ in BUNDLES}
    for dirpath, _, names in os.walk(src):
        for n in names:
            full = os.path.join(dirpath, n)
            rel = os.path.relpath(full, src).replace(os.sep, "/")
            name = next(b for b, prefix in BUNDLES if rel.startswith(prefix))
            groups[name].append({"path": rel, "size": os.path.getsize(full), "sha256": sha256_file(full)})

    bundles = []
    for name, _ in BUNDLES:
        files = sorted(groups[name], key=lambda f: f["path"])
        if not files:
            continue
        zpath = os.path.join(out, f"{name}.zip")
        with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
            for f in files:
                z.write(os.path.join(src, f["path"]), f["path"])
        bundles.append({
            "name": name,
            "file": f"{name}.zip",
            "size": os.path.getsize(zpath),
            "sha256": sha256_file(zpath),
            "content": content_hash(files),
            "files": files,
        })
        print(f"{name}.zip: {len(files)} files, {os.path.getsize(zpath) / 1e6:.1f} MB")

    manifest = {
        "game": "HeroWorld",
        "version": version,
        "commit": commit,
        "built": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "whats_new": whats_new,
        "launcher": {
            "version": launcher_version,
            "file": os.path.basename(launcher),
            "size": os.path.getsize(launcher),
            "sha256": sha256_file(launcher),
        },
        "bundles": bundles,
    }
    with open(os.path.join(out, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print(f"manifest.json: HeroWorld {version}, launcher {launcher_version}")


if __name__ == "__main__":
    main()
