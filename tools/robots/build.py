"""Builds the robot parts with AssetGenerator into assets/robots/<id>.glb.

    BLENDER_PYTHON=... python tools/robots/build.py [--ag ../AssetGenerator] [ids...]

Runs `ag.py generate` on each spec in assets/robots/specs/, copies the .glb here and makes
every material called `glow` emissive (AssetGenerator bakes colour, not light), so the
visors, portholes and the Aether core shine in Engine.
"""

from __future__ import annotations

import argparse
import json
import shutil
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPECS = ROOT / "assets" / "robots" / "specs"
OUT = ROOT / "assets" / "robots"
GLOW = [0.45, 1.0, 0.55]
GLOW_STRENGTH = 1.6


def make_glow(path: Path) -> None:
    data = path.read_bytes()
    magic, version, _ = struct.unpack_from("<III", data, 0)
    assert magic == 0x46546C67 and version == 2, f"{path} is not a glb"
    jlen, jtype = struct.unpack_from("<II", data, 12)
    assert jtype == 0x4E4F534A
    doc = json.loads(data[20 : 20 + jlen])
    rest = data[20 + jlen :]
    for m in doc.get("materials", []):
        if m.get("name", "").lower().startswith("glow"):
            m["emissiveFactor"] = [1.0, 1.0, 1.0]
            m.setdefault("extensions", {})["KHR_materials_emissive_strength"] = {
                "emissiveStrength": GLOW_STRENGTH
            }
            pbr = m.setdefault("pbrMetallicRoughness", {})
            # Emissive colour comes from the base colour texture if there is one.
            if "baseColorTexture" in pbr:
                m["emissiveTexture"] = dict(pbr["baseColorTexture"])
            else:
                m["emissiveFactor"] = GLOW
    used = doc.setdefault("extensionsUsed", [])
    if "KHR_materials_emissive_strength" not in used:
        used.append("KHR_materials_emissive_strength")
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * ((4 - len(js) % 4) % 4)
    out = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + len(rest))
    out += struct.pack("<II", len(js), 0x4E4F534A) + js + rest
    path.write_bytes(out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ag", default=str(ROOT.parent / "AssetGenerator"))
    ap.add_argument("ids", nargs="*")
    args = ap.parse_args()
    ag = Path(args.ag)
    ids = args.ids or sorted(p.stem for p in SPECS.glob("*.json"))
    failed = []
    for ident in ids:
        spec = SPECS / f"{ident}.json"
        r = subprocess.run(
            [sys.executable, str(ag / "tools" / "ag.py"), "generate", str(spec)],
            cwd=ag,
            capture_output=True,
            text=True,
        )
        glb = ag / "assets" / "out" / f"{ident}.glb"
        if r.returncode != 0 or not glb.is_file():
            print(r.stdout[-3000:], r.stderr[-2000:])
            failed.append(ident)
            continue
        shutil.copy(glb, OUT / f"{ident}.glb")
        make_glow(OUT / f"{ident}.glb")
        print(f"{ident}: {(OUT / f'{ident}.glb').stat().st_size // 1024} KB")
    if failed:
        print("failed:", failed)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
