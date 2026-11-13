"""Builds the town's animals with AssetGenerator into assets/creatures/<id>.glb.

    python tools/creatures/make_specs.py
    BLENDER_PYTHON=... python tools/creatures/build.py [--ag ../AssetGenerator] [ids...]

Runs `ag.py generate` on each spec in assets/creatures/specs/ and copies the model and its
two lighter levels (`<id>_lod1.glb`, `<id>_lod2.glb`) here. AssetGenerator bakes colour, not
light, so every material whose name starts with `glow` is made to shine with its own colours
(a wibbet's feeler lights, a driftbell's bell and heart).
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
SPECS = ROOT / "assets" / "creatures" / "specs"
OUT = ROOT / "assets" / "creatures"
# How brightly each glowing material shines (linear; bloom shows from about 1).
GLOW = {"glow": 4.0, "glowheart": 3.0, "glowbell": 0.55}


def make_glow(path: Path) -> None:
    data = path.read_bytes()
    magic, version, _ = struct.unpack_from("<III", data, 0)
    assert magic == 0x46546C67 and version == 2, f"{path} is not a glb"
    jlen, jtype = struct.unpack_from("<II", data, 12)
    assert jtype == 0x4E4F534A
    doc = json.loads(data[20 : 20 + jlen])
    rest = data[20 + jlen :]
    changed = False
    for m in doc.get("materials", []):
        strength = GLOW.get(m.get("name", "").lower().split(".")[0])
        if strength is None:
            continue
        changed = True
        m.setdefault("extensions", {})["KHR_materials_emissive_strength"] = {"emissiveStrength": strength}
        pbr = m.setdefault("pbrMetallicRoughness", {})
        if "baseColorTexture" in pbr:
            m["emissiveFactor"] = [1.0, 1.0, 1.0]
            m["emissiveTexture"] = dict(pbr["baseColorTexture"])
        else:
            m["emissiveFactor"] = list(pbr.get("baseColorFactor", [1, 1, 1, 1])[:3])
    if not changed:
        return
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
            [sys.executable, str(ag / "tools" / "ag.py"), "generate", str(spec), "--no-preview"],
            cwd=ag,
            capture_output=True,
            text=True,
        )
        levels = [ident, f"{ident}_lod1", f"{ident}_lod2"]
        if r.returncode != 0 or not all((ag / "assets" / "out" / f"{n}.glb").is_file() for n in levels):
            print(r.stdout[-3000:], r.stderr[-2000:])
            failed.append(ident)
            continue
        for n in levels:
            shutil.copy(ag / "assets" / "out" / f"{n}.glb", OUT / f"{n}.glb")
            make_glow(OUT / f"{n}.glb")
        sizes = ", ".join(f"{(OUT / f'{n}.glb').stat().st_size // 1024} KB" for n in levels)
        print(f"{ident}: {sizes}", flush=True)
    if failed:
        print("failed:", failed)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
