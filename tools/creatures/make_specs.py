"""Writes the AssetGenerator specs of the town's animals into assets/creatures/specs/.

    python tools/creatures/make_specs.py

Each animal is an `organic.sculpt` with a rig and clips. Coordinates as AssetGenerator's:
front -Y, up +Z, the creature's left +X, metres. Colours are linear. The colour variants of an
animal share its shape and rig and differ in their materials only.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPECS = ROOT / "assets" / "creatures" / "specs"


def lin(r: int, g: int, b: int) -> list[float]:
    """An sRGB colour (0..255) as the linear one a spec wants."""

    def f(c: int) -> float:
        x = c / 255.0
        return round(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4, 4)

    return [f(r), f(g), f(b)]


def sph(c, r):
    return {"sphere": {"centre": list(c), "radius": r}}


def ell(c, r):
    return {"ellipsoid": {"centre": list(c), "radii": list(r)}}


def cap(a, b, r, rb=None):
    d = {"a": list(a), "b": list(b), "radius": r}
    if rb is not None:
        d["radius_b"] = rb
    return {"capsule": d}


def ref(n):
    return {"ref": n}


def mx(n):
    return {"mirror_x": n}


def inside(shape, color, soft=0.004, **kw):
    return {"inside": {"shape": shape, "color": color, "soft": soft, **kw}}


def still(legs):
    """Feet planted where they stand."""
    return {"gait": {"legs": {leg: 0 for leg in legs}, "stride": 0.0, "lift": 0.0, "duty": 0.95}}


def turn(bone, wave="sine", cycles=1, phase=0.0, **angles):
    return {"turn": {"bone": bone, "wave": wave, "cycles": cycles, "phase": phase, **angles}}


def hold(bone, **angles):
    return {"turn": {"bone": bone, "wave": "constant", **angles}}


# ---------------------------------------------------------------- the wibbet

# A wibbet: a big-eyed climber the size of a cat, not quite a monkey: a round head, eyes like
# saucers, round ears, two feelers with a light at the tip, hands on all four limbs and a long
# ringed tail it carries high.
WIBBET_COATS = {
    # name: fur, cream (belly and face), rings and hands, inner ear, feeler light
    "teal": (lin(38, 150, 160), lin(238, 226, 190), lin(18, 62, 84), lin(240, 150, 150)),
    "coral": (lin(232, 112, 72), lin(250, 232, 200), lin(120, 40, 36), lin(250, 190, 170)),
    "lilac": (lin(150, 120, 210), lin(240, 232, 244), lin(62, 40, 110), lin(250, 170, 200)),
}


def wibbet(coat: str) -> dict:
    fur, cream, dark, pink = WIBBET_COATS[coat]
    eye_l = (0.052, -0.255, 0.372)
    tail = [(0, 0.15, 0.245), (0, 0.27, 0.28), (0, 0.345, 0.385), (0, 0.335, 0.495), (0, 0.265, 0.555)]
    ear_turn = {"degrees": [0, 0, -28], "about": [0.085, -0.165, 0.42]}
    parts = {
        "chest": ell((0, -0.07, 0.235), (0.085, 0.095, 0.085)),
        "hips": ell((0, 0.07, 0.23), (0.082, 0.1, 0.082)),
        "head": ell((0, -0.185, 0.355), (0.105, 0.095, 0.096)),
        "muzzle": ell((0, -0.268, 0.322), (0.042, 0.034, 0.03)),
        "nose": ell((0, -0.3, 0.332), (0.014, 0.009, 0.009)),
        "eye": sph(eye_l, 0.042),
        "ear": {"rotate": {**ear_turn, "shape": ell((0.115, -0.165, 0.455), (0.06, 0.017, 0.062))}},
        "feeler": {
            "sweep": {
                "points": [[0.03, -0.2, 0.44], [0.04, -0.212, 0.5], [0.05, -0.24, 0.545], [0.056, -0.275, 0.565]],
                "radius": [0.011, 0.009, 0.008, 0.008],
            }
        },
        "bulb": sph((0.057, -0.283, 0.567), 0.018),
        "arm": cap((0.072, -0.1, 0.215), (0.128, -0.085, 0.125), 0.03, 0.024),
        "forearm": cap((0.128, -0.085, 0.125), (0.128, -0.125, 0.036), 0.024, 0.019),
        "hand": ell((0.128, -0.15, 0.018), (0.03, 0.045, 0.018)),
        "thigh": cap((0.068, 0.085, 0.215), (0.135, 0.045, 0.125), 0.04, 0.028),
        "shin": cap((0.135, 0.045, 0.125), (0.13, 0.105, 0.038), 0.026, 0.019),
        "foot": ell((0.13, 0.07, 0.018), (0.03, 0.055, 0.018)),
        "tuft": sph((0, 0.255, 0.56), 0.036),
        "mouth": {
            "rotate": {
                "degrees": [-12, 0, 0],
                "about": [0, -0.29, 0.31],
                "shape": ell((0, -0.292, 0.31), (0.026, 0.03, 0.0035)),
            }
        },
        # Paint only.
        "belly": ell((0, -0.01, 0.165), (0.06, 0.18, 0.07)),
        "eyes": mx(ref("eye")),
        "mask": ell((0, -0.285, 0.35), (0.095, 0.06, 0.075)),
        "pupil": sph((0.055, -0.293, 0.372), 0.028),
        "inner_ear": {"rotate": {**ear_turn, "shape": ell((0.117, -0.184, 0.455), (0.042, 0.012, 0.045))}},
    }
    for i in range(4):
        parts[f"tail{i + 1}"] = cap(tail[i], tail[i + 1], 0.03 - 0.002 * i, 0.028 - 0.002 * i)
    rings = {"union": [sph([0, y, z], 0.042) for (y, z) in [(0.31, 0.33), (0.345, 0.44), (0.305, 0.53)]]}
    body = {
        "smooth_union": {
            "k": 0.035,
            "shapes": [
                ref("chest"),
                ref("hips"),
                {"smooth_union": {"k": 0.02, "shapes": [ref("head"), ref("muzzle")]}},
                mx(ref("ear")),
            ],
        }
    }
    limbs = [
        mx({"smooth_union": {"k": 0.012, "shapes": [ref("arm"), ref("forearm"), ref("hand")]}}),
        mx({"smooth_union": {"k": 0.012, "shapes": [ref("thigh"), ref("shin"), ref("foot")]}}),
        {"smooth_union": {"k": 0.012, "shapes": [ref(f"tail{i + 1}") for i in range(4)] + [ref("tuft")]}},
    ]
    shape = {
        "above": {
            "height": 0.0,
            "shape": {
                "union": [
                    {
                        "subtract": {
                            "from": {"smooth_union": {"k": 0.018, "shapes": [body, *limbs]}},
                            "cut": [ref("mouth")],
                            "k": 0.004,
                        }
                    },
                    {"smooth_union": {"k": 0.006, "shapes": [mx(ref("feeler")), mx(ref("bulb"))]}},
                    ref("nose"),
                    mx(ref("eye")),
                ]
            },
        }
    }
    fur_mat = {
        "base_color": [*fur, 1],
        "roughness": 0.82,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"mottle": {"color": [c * 0.72 for c in fur], "scale": 16, "coverage": 0.45, "contrast": 1.6, "amount": 0.55}},
                inside(ref("belly"), cream, soft=0.03),
                inside(ref("mask"), cream, soft=0.022),
                inside(rings, dark, soft=0.01),
                inside(mx({"union": [ref("hand"), ref("foot")]}), dark, soft=0.02, grow=0.012),
                inside(mx(ref("inner_ear")), pink, soft=0.008),
                inside(ref("mouth"), [c * 0.25 for c in pink], soft=0.003, grow=0.004),
                # Fur: fine streaks running along the body, raised a hair's breadth.
                {"mottle": {"color": [min(1.0, c * 1.25 + 0.02) for c in fur], "scale": 90, "stretch": [1, 5, 2], "coverage": 0.5, "contrast": 2.5, "amount": 0.22, "relief": 0.0012, "seed": 3}},
                {"cavities": {"color": [c * 0.4 for c in fur], "radius": 0.02, "amount": 0.45}},
            ]
        },
    }
    eye_mat = {
        "base_color": [*lin(255, 176, 46), 1],
        "roughness": 0.06,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"gradient": {"color": lin(150, 70, 10), "axis": "z", "from": 0.41, "to": 0.35, "amount": 0.7}},
                inside(mx(ref("pupil")), [0.004, 0.004, 0.006], soft=0.002),
                inside({"union": [sph((0.038, -0.29, 0.39), 0.0105), sph((-0.066, -0.29, 0.39), 0.0105)]}, [1, 1, 1], soft=0.0015),
                inside({"union": [sph((0.07, -0.286, 0.356), 0.0055), sph((-0.034, -0.286, 0.356), 0.0055)]}, [1, 1, 1], soft=0.0015),
            ]
        },
    }
    legs = ["fore.L", "fore.R", "hind.L", "hind.R"]
    tail_bones = ["tail1", "tail2", "tail3", "tail4"]
    tail_wave = lambda deg, cycles=1: {"wave": {"bones": tail_bones, "roll": deg, "cycles": cycles, "lag": 0.14, "grow": 1.25}}
    ears = lambda cycles=3: turn("ear.*", pitch=9, cycles=cycles, phase=0.1)
    feelers = lambda cycles=2: turn("feeler.*", pitch=10, cycles=cycles, phase=0.3)
    # Sitting up on its haunches: the body tipped up about the hips, the head level again,
    # the arms hanging in front of the chest.
    sat = [
        hold("body", pitch=52),
        hold("head", pitch=-46),
        hold("arm.*", pitch=-62),
        hold("forearm.*", pitch=40),
        hold("tail1", pitch=-70),
        {"move": {"z": -0.05, "wave": "constant"}},
        still(["hind.L", "hind.R"]),
    ]
    clips = {
        "Idle": {
            "seconds": 4.0,
            "loop": True,
            "motions": [
                {"move": {"z": 0.004, "cycles": 4}},
                turn("head", yaw=24, cycles=1),
                turn("head", roll=8, cycles=2, phase=0.2),
                ears(4),
                feelers(2),
                tail_wave(9, 2),
                still(legs),
            ],
        },
        "Walk": {
            "seconds": 0.4,
            "loop": True,
            "motions": [
                {"gait": {"legs": {"hind.L": 0.0, "fore.R": 0.06, "hind.R": 0.5, "fore.L": 0.56}, "stride": 0.16, "lift": 0.045, "duty": 0.42}},
                {"move": {"z": 0.014, "wave": "bounce", "cycles": 2}},
                turn("body", pitch=4, cycles=2),
                turn("head", pitch=-4, cycles=2),
                tail_wave(7, 1),
                feelers(1),
            ],
        },
        "Sit": {
            "seconds": 4.0,
            "loop": True,
            "motions": [*sat, turn("head", yaw=30, cycles=1), ears(4), feelers(2), tail_wave(8, 2)],
        },
        "Beg": {
            "seconds": 2.0,
            "loop": True,
            "motions": [
                *sat,
                turn("head", roll=16, cycles=1),
                turn("head", pitch=6, cycles=2),
                turn("arm.L", pitch=22, cycles=2),
                turn("forearm.L", pitch=18, cycles=2, phase=0.15),
                ears(2),
                feelers(2),
                tail_wave(12, 2),
            ],
        },
    }
    rig = {
        "bones": {
            "body": {"head": [0, 0.11, 0.235], "tail": [0, -0.12, 0.245]},
            "head": {"parent": "body", "head": [0, -0.125, 0.31], "tail": [0, -0.28, 0.35]},
            "ear.L": {"parent": "head", "head": [0.08, -0.165, 0.42], "tail": [0.14, -0.165, 0.51]},
            "feeler.L": {"parent": "head", "head": [0.03, -0.2, 0.44], "tail": [0.056, -0.275, 0.565]},
            "arm.L": {"parent": "body", "from": "arm"},
            "forearm.L": {"parent": "arm.L", "from": "forearm"},
            "hand.L": {"parent": "forearm.L", "head": [0.128, -0.125, 0.03], "tail": [0.128, -0.19, 0.012]},
            "thigh.L": {"parent": "body", "from": "thigh"},
            "shin.L": {"parent": "thigh.L", "from": "shin"},
            "foot.L": {"parent": "shin.L", "head": [0.13, 0.105, 0.03], "tail": [0.13, 0.02, 0.012]},
            "tail1": {"parent": "body", "from": "tail1"},
            "tail2": {"parent": "tail1", "from": "tail2"},
            "tail3": {"parent": "tail2", "from": "tail3"},
            "tail4": {"parent": "tail3", "from": "tail4"},
        },
        "carry": {
            "body": ["chest", "hips"],
            "head": ["head", "muzzle", "nose", "eyes"],
            "ear.L": ["ear"],
            "feeler.L": ["feeler", "bulb"],
            "hand.L": ["hand"],
            "foot.L": ["foot"],
            "tail4": ["tail4", "tuft"],
        },
        "legs": {
            "fore.L": {"upper": "arm.L", "lower": "forearm.L", "foot": "hand.L"},
            "hind.L": {"upper": "thigh.L", "lower": "shin.L", "foot": "foot.L"},
        },
        "softness": {"default": 0.01, "head": 0.008},
        "clips": clips,
    }
    return {
        "spec_version": 1,
        "id": f"wibbet_{coat}",
        "generator": "organic.sculpt",
        "textures": {"texels_per_metre": 2048, "max_size": 1024},
        "materials": {
            "fur": fur_mat,
            "eye": eye_mat,
            "nose": {"base_color": [0.02, 0.015, 0.02, 1], "roughness": 0.25, "metallic": 0.0},
            "glow": {"base_color": [*lin(255, 236, 150), 1], "roughness": 0.3, "metallic": 0.0},
        },
        "params": {
            "parts": parts,
            "shape": shape,
            "base_slot": "fur",
            "paint": {"eye": mx(ref("eye")), "nose": ref("nose"), "glow": mx(ref("bulb"))},
            "paint_reach": 0.002,
            "max_triangles": 14000,
            "detail": 6,
            "rig": rig,
        },
        "lods": [{"max_triangles": 3200}, {"max_triangles": 700}],
        "qa": {
            "max_triangles": 16000,
            "require_uvs": True,
            "require_manifold": True,
            "require_origin_at_base": True,
            "max_dimension_m": 1.0,
        },
    }


# --------------------------------------------------------------- the drakelet

# A drakelet: a pet dragon the size of a big parrot, flown on a lead. Modelled standing with
# its wings spread; in the air it keeps that pose and beats them. A collar for the lead.
DRAKELET_COATS = {
    # name: scales, belly, wing skin, horns and spines, collar
    "ember": (lin(196, 52, 40), lin(250, 196, 96), lin(250, 140, 50), lin(70, 30, 34), lin(40, 120, 200)),
    "jade": (lin(40, 150, 96), lin(236, 226, 130), lin(170, 214, 90), lin(30, 70, 60), lin(200, 50, 50)),
    "azure": (lin(52, 110, 214), lin(214, 234, 250), lin(214, 130, 220), lin(34, 40, 96), lin(240, 190, 60)),
}


def drakelet(coat: str) -> dict:
    scales, belly, skin, horn, collar = DRAKELET_COATS[coat]
    tail = [(0, 0.1, 0.3), (0, 0.23, 0.292), (0, 0.35, 0.3), (0, 0.455, 0.325)]
    tail_r = [0.045, 0.032, 0.021, 0.012]
    shoulder, wrist, tip = (0.088, 0.005, 0.36), (0.27, -0.072, 0.392), (0.5, 0.012, 0.385)
    spine = [(-0.055, 0.378), (-0.01, 0.384), (0.035, 0.382), (0.08, 0.37)]
    parts = {
        "body": ell((0, 0, 0.305), (0.078, 0.132, 0.074)),
        "neck": cap((0, -0.112, 0.338), (0, -0.2, 0.425), 0.04, 0.035),
        "head": ell((0, -0.25, 0.45), (0.068, 0.072, 0.062)),
        "snout": ell((0, -0.322, 0.432), (0.037, 0.046, 0.03)),
        "nostril": sph((0.017, -0.356, 0.448), 0.009),
        "brow": ell((0.045, -0.27, 0.5), (0.03, 0.032, 0.012)),
        "eye": sph((0.046, -0.283, 0.47), 0.032),
        "horn": cap((0.032, -0.212, 0.495), (0.058, -0.135, 0.555), 0.013, 0.004),
        "cheek": cap((0.05, -0.225, 0.435), (0.082, -0.165, 0.43), 0.011, 0.004),
        "collar": {
            "rotate": {
                "degrees": [-42, 0, 0],
                "about": [0, -0.148, 0.378],
                "shape": {"torus": {"centre": [0, -0.148, 0.378], "radius": 0.039, "thickness": 0.011, "axis": "y"}},
            }
        },
        "tag": sph((0, -0.176, 0.336), 0.014),
        "wing": cap(shoulder, wrist, 0.019, 0.014),
        "finger": cap(wrist, tip, 0.014, 0.006),
        # The wing's skin: two thin sheets, the inner on the arm and the outer on the finger,
        # their trailing edge scalloped between the ribs.
        "skin_in": {
            "subtract": {
                "from": ell((0.185, 0.07, 0.378), (0.125, 0.135, 0.0075)),
                "cut": [{"cylinder": {"a": [0.13, 0.235, 0.3], "b": [0.13, 0.235, 0.46], "radius": 0.07}}],
                "k": 0.004,
            }
        },
        "skin_out": {
            "subtract": {
                "from": ell((0.375, 0.05, 0.387), (0.135, 0.12, 0.0065)),
                "cut": [
                    {"cylinder": {"a": [0.3, 0.215, 0.3], "b": [0.3, 0.215, 0.46], "radius": 0.075}},
                    {"cylinder": {"a": [0.445, 0.185, 0.3], "b": [0.445, 0.185, 0.46], "radius": 0.07}},
                ],
                "k": 0.004,
            }
        },
        "rib": cap(wrist, (0.36, 0.145, 0.386), 0.009, 0.004),
        "foreleg": cap((0.05, -0.075, 0.262), (0.088, -0.09, 0.2), 0.026, 0.02),
        "forearm": cap((0.088, -0.09, 0.2), (0.088, -0.118, 0.152), 0.02, 0.016),
        "forepaw": ell((0.088, -0.136, 0.14), (0.021, 0.032, 0.013)),
        "thigh": cap((0.055, 0.07, 0.265), (0.102, 0.045, 0.198), 0.033, 0.023),
        "shin": cap((0.102, 0.045, 0.198), (0.1, 0.088, 0.152), 0.022, 0.016),
        "hindpaw": ell((0.1, 0.066, 0.14), (0.022, 0.038, 0.013)),
        "fin": ell((0, 0.49, 0.335), (0.012, 0.045, 0.036)),
        "spines": {"union": [ell((0, y, z), (0.008, 0.024, 0.022)) for (y, z) in spine]},
        # Paint only.
        "underside": ell((0, -0.02, 0.24), (0.055, 0.2, 0.06)),
        # Both sides of the head's pairs, for the head bone to carry.
        "face": mx({"union": [ref("nostril"), ref("brow"), ref("eye"), ref("horn"), ref("cheek")]}),
        "throat": cap((0, -0.13, 0.3), (0, -0.24, 0.395), 0.03),
        "pupil": ell((0.066, -0.306, 0.47), (0.013, 0.013, 0.023)),
    }
    for i in range(3):
        parts[f"tail{i + 1}"] = cap(tail[i], tail[i + 1], tail_r[i], tail_r[i + 1])
    wing = {
        "smooth_union": {
            "k": 0.006,
            "shapes": [ref("wing"), ref("finger"), ref("rib"), ref("skin_in"), ref("skin_out")],
        }
    }
    trunk = {
        "smooth_union": {
            "k": 0.03,
            "shapes": [
                ref("body"),
                ref("neck"),
                {"smooth_union": {"k": 0.02, "shapes": [ref("head"), ref("snout"), mx(ref("brow"))]}},
                {"smooth_union": {"k": 0.012, "shapes": [ref("tail1"), ref("tail2"), ref("tail3")]}},
            ],
        }
    }
    shape = {
        "above": {
            "height": 0.0,
            "shape": {
                "union": [
                    {
                        "smooth_union": {
                            "k": 0.012,
                            "shapes": [
                                trunk,
                                mx(wing),
                                mx({"smooth_union": {"k": 0.01, "shapes": [ref("foreleg"), ref("forearm"), ref("forepaw")]}}),
                                mx({"smooth_union": {"k": 0.01, "shapes": [ref("thigh"), ref("shin"), ref("hindpaw")]}}),
                                mx(ref("nostril")),
                            ],
                        }
                    },
                    {"smooth_union": {"k": 0.004, "shapes": [ref("spines"), ref("fin"), mx(ref("horn")), mx(ref("cheek"))]}},
                    {"union": [ref("collar"), ref("tag")]},
                    mx(ref("eye")),
                ]
            },
        }
    }
    dark = [c * 0.45 for c in scales]
    scale_mat = {
        "base_color": [*scales, 1],
        "roughness": 0.42,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"mottle": {"color": [c * 0.7 for c in scales], "scale": 14, "coverage": 0.45, "contrast": 1.6, "amount": 0.5}},
                inside(ref("underside"), belly, soft=0.02),
                inside(ref("throat"), belly, soft=0.012),
                {"cells": {"color": dark, "size": 0.016, "width": 0.0022, "amount": 0.55, "relief": -0.0009}},
                {"cells": {"color": [min(1.0, c * 1.5 + 0.03) for c in scales], "size": 0.016, "width": 0.0, "fill": 0.35, "amount": 0.3}},
                {"cavities": {"color": dark, "radius": 0.015, "amount": 0.5}},
            ]
        },
    }
    skin_mat = {
        "base_color": [*skin, 1],
        "roughness": 0.5,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"gradient": {"color": [c * 0.5 for c in skin], "axis": "x", "from": 0.02, "to": 0.3, "amount": 0.55}},
                {"cells": {"color": [c * 0.4 for c in skin], "size": 0.07, "width": 0.003, "amount": 0.45, "relief": -0.0006}},
                {"mottle": {"color": [min(1.0, c * 1.3) for c in skin], "scale": 30, "coverage": 0.4, "amount": 0.3}},
            ]
        },
    }
    eye_mat = {
        "base_color": [*lin(250, 214, 60), 1],
        "roughness": 0.06,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                inside(mx(ref("pupil")), [0.004, 0.004, 0.006], soft=0.0015),
                inside({"union": [sph((0.055, -0.31, 0.485), 0.007), sph((-0.07, -0.306, 0.485), 0.007)]}, [1, 1, 1], soft=0.001),
            ]
        },
    }
    legs = ["fore.L", "fore.R", "hind.L", "hind.R"]
    tails = ["tail1", "tail2", "tail3"]
    beat = lambda deg: [
        turn("wing.*", roll=-deg, cycles=1),
        turn("finger.*", roll=-deg * 0.7, cycles=1, phase=-0.18),
    ]
    dangle = [
        turn("thigh.*", pitch=8, cycles=1, phase=0.3),
        turn("foreleg.*", pitch=8, cycles=1, phase=0.55),
    ]
    clips = {
        # In the air on the spot: nose up, wings beating hard, tail hanging.
        "Hover": {
            "seconds": 0.5,
            "loop": True,
            "motions": [
                hold("body", pitch=24),
                hold("neck", pitch=-12),
                hold("head", pitch=-12),
                hold("tail1", pitch=-16),
                *beat(40),
                # Clear of the ground at the bottom of a beat (a game flies it higher).
                {"move": {"z": 0.07, "wave": "constant"}},
                {"move": {"z": 0.022, "cycles": 1, "phase": 0.3}},
                {"wave": {"bones": tails, "pitch": 6, "cycles": 1, "lag": 0.15, "grow": 1.3}},
                *dangle,
            ],
        },
        # Flying along: level, legs tucked back, the tail streaming.
        "Fly": {
            "seconds": 0.5,
            "loop": True,
            "motions": [
                hold("thigh.*", pitch=-35),
                hold("foreleg.*", pitch=-40),
                hold("neck", pitch=-14),
                hold("head", pitch=8),
                *beat(34),
                {"move": {"z": 0.07, "wave": "constant"}},
                {"move": {"z": 0.018, "cycles": 1, "phase": 0.3}},
                {"wave": {"bones": tails, "pitch": 7, "cycles": 1, "lag": 0.18, "grow": 1.4}},
            ],
        },
        # On the ground: wings folded back along the body, looking about.
        "Sit": {
            "seconds": 4.0,
            "loop": True,
            "motions": [
                hold("wing.*", roll=38),
                hold("finger.*", roll=-75),
                turn("head", yaw=28, cycles=1),
                turn("neck", pitch=6, cycles=2),
                turn("wing.*", roll=4, cycles=2),
                {"wave": {"bones": tails, "yaw": 12, "cycles": 2, "lag": 0.15, "grow": 1.3}},
                {"move": {"z": 0.003, "cycles": 4}},
                still(legs),
            ],
        },
    }
    rig = {
        "bones": {
            "body": {"head": [0, 0.1, 0.3], "tail": [0, -0.09, 0.315]},
            "neck": {"parent": "body", "from": "neck"},
            "head": {"parent": "neck", "head": [0, -0.2, 0.425], "tail": [0, -0.36, 0.435]},
            "wing.L": {"parent": "body", "from": "wing"},
            "finger.L": {"parent": "wing.L", "from": "finger"},
            "foreleg.L": {"parent": "body", "from": "foreleg"},
            "forearm.L": {"parent": "foreleg.L", "from": "forearm"},
            "forepaw.L": {"parent": "forearm.L", "head": [0.088, -0.118, 0.148], "tail": [0.088, -0.165, 0.134]},
            "thigh.L": {"parent": "body", "from": "thigh"},
            "shin.L": {"parent": "thigh.L", "from": "shin"},
            "hindpaw.L": {"parent": "shin.L", "head": [0.1, 0.088, 0.148], "tail": [0.1, 0.03, 0.134]},
            "tail1": {"parent": "body", "from": "tail1"},
            "tail2": {"parent": "tail1", "from": "tail2"},
            "tail3": {"parent": "tail2", "from": "tail3"},
        },
        "carry": {
            "body": ["body", "spines"],
            "neck": ["neck", "collar", "tag"],
            "head": ["head", "snout", "face"],
            "wing.L": ["wing", "skin_in"],
            "finger.L": ["finger", "skin_out", "rib"],
            "forepaw.L": ["forepaw"],
            "hindpaw.L": ["hindpaw"],
            "tail3": ["tail3", "fin"],
        },
        "legs": {
            "fore.L": {"upper": "foreleg.L", "lower": "forearm.L", "foot": "forepaw.L"},
            "hind.L": {"upper": "thigh.L", "lower": "shin.L", "foot": "hindpaw.L"},
        },
        "softness": {"default": 0.01},
        "clips": clips,
    }
    return {
        "spec_version": 1,
        "id": f"drakelet_{coat}",
        "generator": "organic.sculpt",
        "textures": {"texels_per_metre": 1536, "max_size": 1024},
        "materials": {
            "scales": scale_mat,
            "wingskin": skin_mat,
            "eye": eye_mat,
            "horn": {"base_color": [*horn, 1], "roughness": 0.35, "metallic": 0.0},
            "collar": {"base_color": [*collar, 1], "roughness": 0.5, "metallic": 0.0},
            "brass": {"base_color": [*lin(240, 196, 90), 1], "roughness": 0.25, "metallic": 1.0},
        },
        "params": {
            "parts": parts,
            "shape": shape,
            "base_slot": "scales",
            "paint": {
                "eye": mx(ref("eye")),
                "wingskin": mx({"union": [ref("skin_in"), ref("skin_out")]}),
                "horn": {"union": [ref("spines"), ref("fin"), mx(ref("horn")), mx(ref("cheek"))]},
                "collar": ref("collar"),
                "brass": ref("tag"),
            },
            "paint_reach": 0.002,
            "max_triangles": 16000,
            "detail": 6,
            "rig": rig,
        },
        "lods": [{"max_triangles": 3600}, {"max_triangles": 800}],
        "qa": {
            "max_triangles": 18000,
            "require_uvs": True,
            "require_manifold": True,
            "require_origin_at_base": True,
            "max_dimension_m": 1.5,
        },
    }



# -------------------------------------------------------------- the driftbell

# A driftbell: a sky jelly that floats over the streets, its bell a metre across, a glowing
# heart under it, ruffled arms and six long trailing tentacles. Modelled hanging upright.
DRIFTBELL_COATS = {
    # name: bell, spots and rim, heart, tentacles
    "rose": (lin(236, 120, 170), lin(255, 226, 140), lin(255, 190, 110), lin(190, 90, 170)),
    "aqua": (lin(70, 190, 220), lin(220, 255, 250), lin(150, 255, 230), lin(60, 120, 200)),
    "amber": (lin(240, 160, 60), lin(255, 244, 190), lin(255, 220, 120), lin(210, 100, 50)),
}


def driftbell(coat: str) -> dict:
    import math

    bell, spot, heart, tent = DRIFTBELL_COATS[coat]
    parts = {
        "dome": {
            "subtract": {
                "from": {"above": {"height": 0.93, "shape": ell((0, 0, 1.0), (0.37, 0.37, 0.27))}},
                "cut": [ell((0, 0, 0.9), (0.315, 0.315, 0.2))],
                "k": 0.01,
            }
        },
        "rim": {"torus": {"centre": [0, 0, 0.945], "radius": 0.35, "thickness": 0.03, "axis": "z"}},
        "heart": ell((0, 0, 0.98), (0.15, 0.15, 0.11)),
        # Paint only: spots on the bell.
        "spots": {"repeat": {"around": 7, "shape": sph((0, 0.25, 1.21), 0.055)}},
    }
    # The ruffled arms under the heart (one, mirrored) and three tentacles a side.
    arm = [(0.085, 0.0, 0.9), (0.12, 0.03, 0.7), (0.09, -0.03, 0.48), (0.125, 0.01, 0.28)]
    arm_r = [0.05, 0.042, 0.032, 0.018]
    for i in range(3):
        parts[f"arm{i + 1}"] = cap(arm[i], arm[i + 1], arm_r[i], arm_r[i + 1])
    chains = {"a": 32.0, "b": 90.0, "c": 148.0}
    drop = [0.94, 0.66, 0.36, 0.04]
    reach = [0.33, 0.37, 0.33, 0.36]
    tent_r = [0.02, 0.016, 0.012, 0.008]
    for name, deg in chains.items():
        sx, sy = math.sin(math.radians(deg)), math.cos(math.radians(deg))
        pts = [(round(reach[i] * sx, 4), round(reach[i] * sy, 4), drop[i]) for i in range(4)]
        for i in range(3):
            parts[f"{name}{i + 1}"] = cap(pts[i], pts[i + 1], tent_r[i], tent_r[i + 1])
    limbs = ["arm", *chains]
    parts["trail"] = mx({"union": [ref(f"{n}{i}") for n in limbs for i in (1, 2, 3)]})
    parts["bellparts"] = {"union": [ref("dome"), ref("rim"), ref("heart")]}
    shape = {
        "above": {
            "height": 0.0,
            "shape": {
                "union": [
                    {
                        "displace": {
                            "amount": 0.008,
                            "scale": 9.0,
                            "seed": 5,
                            "shape": {"smooth_union": {"k": 0.02, "shapes": [ref("dome"), ref("rim")]}},
                        }
                    },
                    ref("heart"),
                    {
                        "displace": {
                            "amount": 0.012,
                            "scale": 16.0,
                            "seed": 2,
                            "shape": mx({"smooth_union": {"k": 0.012, "shapes": [ref("arm1"), ref("arm2"), ref("arm3")]}}),
                        }
                    },
                    *[
                        mx({"smooth_union": {"k": 0.008, "shapes": [ref(f"{n}1"), ref(f"{n}2"), ref(f"{n}3")]}})
                        for n in chains
                    ],
                ]
            },
        }
    }
    bell_mat = {
        "base_color": [*bell, 1],
        "roughness": 0.22,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"gradient": {"color": [c * 0.45 for c in bell], "axis": "z", "from": 0.93, "to": 1.2, "amount": 0.6}},
                {"grain": {"color": [min(1.0, c * 1.6 + 0.05) for c in bell], "spacing": 0.05, "axis": "radial", "warp": 0.4, "sharpness": 3, "amount": 0.35}},
                inside(ref("spots"), spot, soft=0.012),
                inside(sph((0, 0, 1.27), 0.09), spot, soft=0.02),
                inside(ref("rim"), spot, soft=0.01, grow=0.004),
            ]
        },
    }
    tent_mat = {
        "base_color": [*tent, 1],
        "roughness": 0.35,
        "metallic": 0.0,
        "pattern": {
            "layers": [
                {"gradient": {"color": spot, "axis": "z", "from": 0.0, "to": 0.35, "amount": 0.8}},
                {"mottle": {"color": [c * 0.55 for c in tent], "scale": 20, "coverage": 0.4, "amount": 0.5}},
            ]
        },
    }
    bones = {"bell": {"head": [0, 0, 0.93], "tail": [0, 0, 1.27]}}
    for n in limbs:
        for i in (1, 2, 3):
            bones[f"{n}{i}.L"] = {"parent": "bell" if i == 1 else f"{n}{i - 1}.L", "from": f"{n}{i}"}

    def sway(deg: float, cycles: int) -> list:
        """Every limb curling in and out, the two sides as mirror images, one after another."""
        out = []
        for k, n in enumerate(limbs):
            for side, sign in ((".L", 1.0), (".R", -1.0)):
                bones_ = [f"{n}{i}{side}" for i in (1, 2, 3)]
                out.append({"wave": {"bones": bones_, "roll": sign * deg, "cycles": cycles, "lag": 0.14 + 0.02 * k, "grow": 1.35}})
        return out

    clips = {
        "Drift": {
            "seconds": 4.0,
            "loop": True,
            "motions": [
                # Clear of the ground when a tentacle swings low (a game floats it high).
                {"move": {"z": 0.1, "wave": "constant"}},
                {"move": {"z": 0.05, "cycles": 1}},
                turn("bell", roll=4, cycles=1, phase=0.2),
                turn("bell", pitch=3, cycles=1),
                *sway(11, 1),
            ],
        },
        "Swim": {
            "seconds": 2.0,
            "loop": True,
            "motions": [
                {"move": {"z": 0.1, "wave": "constant"}},
                {"move": {"z": 0.06, "cycles": 1}},
                hold("bell", pitch=9),
                turn("bell", pitch=4, cycles=1),
                *sway(16, 1),
            ],
        },
    }
    rig = {
        "bones": bones,
        "carry": {"bell": ["bellparts"]},
        "softness": {"default": 0.015},
        "clips": clips,
    }
    return {
        "spec_version": 1,
        "id": f"driftbell_{coat}",
        "generator": "organic.sculpt",
        "textures": {"texels_per_metre": 768, "max_size": 1024},
        "materials": {
            "glowbell": bell_mat,
            "glowheart": {"base_color": [*heart, 1], "roughness": 0.3, "metallic": 0.0},
            "tentacle": tent_mat,
        },
        "params": {
            "parts": parts,
            "shape": shape,
            "base_slot": "glowbell",
            "paint": {"glowheart": ref("heart"), "tentacle": ref("trail")},
            "paint_reach": 0.002,
            "max_triangles": 18000,
            "detail": 5,
            "rig": rig,
        },
        "lods": [{"max_triangles": 3000}, {"max_triangles": 700}],
        "qa": {
            "max_triangles": 20000,
            "require_uvs": True,
            "require_manifold": True,
            "require_origin_at_base": True,
            "max_dimension_m": 2.0,
        },
    }



def main() -> None:
    SPECS.mkdir(parents=True, exist_ok=True)
    specs = [wibbet(c) for c in WIBBET_COATS] + [drakelet(c) for c in DRAKELET_COATS] + [driftbell(c) for c in DRIFTBELL_COATS]
    for spec in specs:
        path = SPECS / f"{spec['id']}.json"
        path.write_text(json.dumps(spec, indent=1) + "\n")
        print(path.relative_to(ROOT))


if __name__ == "__main__":
    main()
