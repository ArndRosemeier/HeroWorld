"""Writes the AssetGenerator specs of the Doctor's robots into assets/robots/specs/.

Each robot is a set of rigid parts, each its own spec with its pivot at the origin, so the
game hangs them on an Engine prefab, walks them with procedural gaits and knocks breakable
parts off. AssetGenerator's coordinates: metres, Z up, front is -Y (glTF +Z after export).

    python tools/robots/make_specs.py      # rewrite the specs
    tools/robots/build.sh                  # build them with AssetGenerator into assets/robots/
"""

from __future__ import annotations

import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "assets" / "robots" / "specs"

# The Doctor's livery: bottle-green enamel, brass trim, chrome, black rubber, and the green
# Aether glow of his tower (the game makes `glow` slots shine).
MATERIALS = {
    "enamel": {
        "base_color": [0.035, 0.11, 0.06, 1],
        "roughness": 0.38,
        "metallic": 0.35,
        "pattern": {
            "layers": [
                {"mottle": {"color": [0.02, 0.06, 0.035], "scale": 3, "coverage": 0.35, "contrast": 2}},
                {"facing": {"color": [0.16, 0.07, 0.03], "direction": [0, 0, -1], "angle": 55,
                            "soft": 30, "breakup": 0.6, "scale": 9, "amount": 0.5, "roughness": 0.85}},
                {"edges": {"color": [0.45, 0.4, 0.3], "radius": 0.012, "breakup": 0.6,
                           "amount": 0.7, "roughness": 0.3}},
            ]
        },
    },
    # The colossus's enamel: the same paint at its size (big soft patches, wide worn edges).
    "hull": {
        "base_color": [0.035, 0.11, 0.06, 1],
        "roughness": 0.4,
        "metallic": 0.35,
        "pattern": {
            "layers": [
                {"mottle": {"color": [0.025, 0.075, 0.04], "scale": 0.7, "coverage": 0.4,
                            "contrast": 1.5, "amount": 0.6}},
                {"mottle": {"color": [0.12, 0.08, 0.04], "scale": 5, "coverage": 0.15,
                            "contrast": 3, "amount": 0.5, "roughness": 0.8, "seed": 4}},
                {"facing": {"color": [0.16, 0.07, 0.03], "direction": [0, 0, -1], "angle": 55,
                            "soft": 30, "breakup": 0.6, "scale": 2, "amount": 0.55, "roughness": 0.85}},
                {"edges": {"color": [0.42, 0.38, 0.28], "radius": 0.08, "breakup": 0.6,
                           "amount": 0.6, "roughness": 0.3}},
            ]
        },
    },
    "brass": {
        "base_color": [0.6, 0.42, 0.14, 1],
        "roughness": 0.28,
        "metallic": 1.0,
        "pattern": {
            "layers": [
                {"mottle": {"color": [0.28, 0.2, 0.07], "scale": 12, "coverage": 0.3,
                            "contrast": 2, "roughness": 0.55}},
            ]
        },
    },
    "chrome": {
        "base_color": [0.82, 0.83, 0.86, 1],
        "roughness": 0.12,
        "metallic": 1.0,
        "pattern": {
            "layers": [
                {"mottle": {"color": [0.66, 0.67, 0.7], "scale": 10, "coverage": 0.2,
                            "contrast": 2, "amount": 0.5, "roughness": 0.25}},
            ]
        },
    },
    "rubber": {"base_color": [0.025, 0.025, 0.028, 1], "roughness": 0.75, "metallic": 0.0},
    "glow": {"base_color": [0.45, 1.0, 0.55, 1], "roughness": 0.3, "metallic": 0.0},
}


def spec(
    ident: str, pieces: list, slots: list[str], tris: int, size: float, bevel: float, detail: float
) -> dict:
    # Heroes fly right up to both robots, so texture for the close-up, but Engine keeps a
    # copy of every map per placed part (four legs, many sentries): cap the maps so a
    # fight with the colossus and its sentries stays within a few hundred MB.
    big = ident.startswith("colossus") and ident not in ("colossus_knee", "colossus_core")
    textures = {
        "texels_per_metre": 256 if big else 768,
        "max_size": 2048 if ident in ("colossus_hull", "colossus_turret") else 1024,
    }
    return {
        "textures": textures,
        "spec_version": 1,
        "id": ident,
        "generator": "prop.assembly",
        "params": {"settle": False, "bevel": bevel, "detail": detail, "pieces": pieces},
        "materials": {s: MATERIALS[s] for s in slots},
        "qa": {
            "max_triangles": tris,
            "require_uvs": True,
            "require_manifold": True,
            "require_origin_at_base": False,
            "max_dimension_m": size,
        },
    }


def cyl(a, b, r, slot, rb=None, **kw):
    c = {"a": a, "b": b, "radius": r}
    if rb is not None:
        c["radius_b"] = rb
    return {"cylinder": c, "slot": slot, **kw}


def box(size, at, slot, **kw):
    return {"box": {"size": size}, "at": at, "slot": slot, **kw}


def sphere(c, r, slot, **kw):
    return {"sphere": {"centre": c, "radius": r}, "slot": slot, **kw}


def torus(c, r, t, axis, slot, **kw):
    return {"torus": {"centre": c, "radius": r, "thickness": t, "axis": axis}, "slot": slot, **kw}


def lathe(profile, slot, **kw):
    return {"lathe": {"profile": profile}, "slot": slot, **kw}


# ---------------------------------------------------------------------------------------
# The robo-sentry: a 2.7 m chrome-and-enamel walker, the Doctor's lieutenant. A barrel torso
# with a domed head and glowing visor, accordion arms (a claw, a ray cannon), two legs, and a
# shield projector dish on a mast on its back (breakable: it shields the minions near it).
# Hips 1.2 m over the ground.

SENTRY_HIP = 1.2


def sentry_body() -> dict:
    p = [
        # Pelvis block and the hip sockets.
        box([0.62, 0.42, 0.24], [0, 0, 0.02], "enamel"),
        sphere([0.25, 0, 0], 0.13, "chrome", mirror="x"),
        # Accordion waist.
        cyl([0, 0, 0.12], [0, 0, 0.36], 0.2, "rubber"),
        torus([0, 0, 0.18], 0.21, 0.035, "z", "chrome"),
        torus([0, 0, 0.26], 0.21, 0.035, "z", "chrome"),
        torus([0, 0, 0.34], 0.21, 0.035, "z", "chrome"),
        # Barrel torso, bulging, with brass bands.
        lathe([[0.0, 0.34], [0.36, 0.34], [0.44, 0.45], [0.47, 0.65], [0.46, 0.88], [0.38, 1.04],
               [0.18, 1.08], [0.0, 1.08]], "enamel"),
        torus([0, 0, 0.47], 0.445, 0.03, "z", "brass"),
        torus([0, 0, 0.98], 0.415, 0.028, "z", "brass"),
        # Chest grille: a dark plate with chrome slats, and a glowing heart lamp.
        box([0.42, 0.06, 0.3], [0, -0.44, 0.7], "rubber"),
        box([0.4, 0.03, 0.026], [0, -0.475, 0.58], "chrome", repeat={"count": 6, "step": [0, 0, 0.048]}),
        sphere([0, -0.47, 0.92], 0.06, "glow"),
        torus([0, -0.46, 0.92], 0.07, 0.016, "y", "brass"),
        # Rivets round the upper band.
        sphere([0.42, 0, 0.9], 0.018, "brass", segments=12, repeat={"around": 18, "start": 10}),
        # Shoulder balls.
        sphere([0.5, 0, 0.9], 0.15, "chrome", mirror="x"),
        # Neck and domed head.
        cyl([0, 0, 1.06], [0, 0, 1.16], 0.12, "chrome"),
        lathe([[0.0, 1.14], [0.25, 1.14], [0.27, 1.2], [0.27, 1.34], [0.24, 1.44], [0.16, 1.51],
               [0.0, 1.53]], "chrome"),
        # The visor: a glowing band across the face, framed in brass.
        box([0.36, 0.08, 0.07], [0, -0.235, 1.31], "glow"),
        box([0.42, 0.06, 0.02], [0, -0.25, 1.36], "brass"),
        box([0.42, 0.06, 0.02], [0, -0.25, 1.26], "brass"),
        # Ear bolts and the antenna with its tip.
        cyl([0.26, 0, 1.3], [0.33, 0, 1.3], 0.05, "brass", mirror="x"),
        cyl([0, 0, 1.5], [0, 0, 1.78], 0.012, "chrome"),
        sphere([0, 0, 1.8], 0.035, "glow"),
        # The dish mount on the back.
        box([0.2, 0.1, 0.22], [0, 0.46, 0.8], "enamel"),
    ]
    return spec("sentry_body", p, ["enamel", "brass", "chrome", "rubber", "glow"], 64000, 2.0, 0.006, 2.0)


def sentry_dish() -> dict:
    # Origin at the mount on the body's back (0, 0.5, 0.8 in body space).
    p = [
        cyl([0, 0, 0], [0, 0.12, 0.62], 0.04, "chrome"),
        torus([0, 0.12, 0.62], 0.06, 0.02, "y", "brass"),
        # A shallow dish facing up and back, a glowing emitter at its focus.
        lathe([[0.0, 0.0], [0.34, 0.12], [0.36, 0.15], [0.33, 0.14], [0.0, 0.03]], "brass",
              rotate=[-20, 0, 0], at=[0, 0.14, 0.62]),
        cyl([0, 0.14, 0.64], [0, 0.2, 0.86], 0.012, "chrome", repeat={"around": 3}),
        sphere([0, 0.19, 0.86], 0.05, "glow"),
    ]
    return spec("sentry_dish", p, ["brass", "chrome", "glow"], 24000, 1.2, 0.003, 2.0)


def sentry_thigh() -> dict:
    # Origin at the hip ball, hangs down to the knee at z = -0.5.
    p = [
        cyl([0, 0, -0.05], [0, 0, -0.45], 0.1, "enamel", rb=0.085),
        torus([0, 0, -0.12], 0.1, 0.022, "z", "brass"),
        sphere([0, 0, -0.5], 0.1, "chrome"),
    ]
    return spec("sentry_thigh", p, ["enamel", "brass", "chrome"], 20000, 0.8, 0.004, 2.0)


def sentry_shin() -> dict:
    # Origin at the knee; the sole is 0.6 m below it (hip 1.2 = thigh 0.5 + shin 0.6 + 0.1).
    p = [
        lathe([[0.0, -0.46], [0.13, -0.46], [0.12, -0.2], [0.09, -0.02], [0.0, 0.0]], "enamel"),
        torus([0, 0, -0.35], 0.125, 0.02, "z", "brass"),
        # Ankle and a broad flat foot, toe forward (-Y).
        sphere([0, 0, -0.5], 0.08, "chrome"),
        box([0.26, 0.42, 0.1], [0, -0.06, -0.55], "rubber"),
        box([0.27, 0.12, 0.06], [0, -0.24, -0.52], "chrome"),
    ]
    return spec("sentry_shin", p, ["enamel", "brass", "chrome", "rubber"], 20000, 0.8, 0.004, 2.0)


def sentry_arm() -> dict:
    # The claw arm, origin at the shoulder; hangs down (the left arm; the game mirrors nothing,
    # the claw is the same either side).
    p = [
        cyl([0, 0, 0], [0, 0, -0.62], 0.07, "rubber"),
        torus([0, 0, -0.1], 0.08, 0.026, "z", "chrome", repeat={"count": 6, "step": [0, 0, -0.1]}),
        cyl([0, 0, -0.62], [0, 0, -0.74], 0.1, "brass"),
        # Two pincer jaws.
        cyl([0.0, 0, -0.74], [0.09, 0, -0.86], 0.026, "chrome", mirror="x"),
        cyl([0.09, 0, -0.86], [0.03, 0, -1.02], 0.024, "chrome", rb=0.012, mirror="x"),
    ]
    return spec("sentry_arm", p, ["rubber", "chrome", "brass"], 32000, 1.2, 0.003, 2.0)


def sentry_gun() -> dict:
    # The ray-cannon arm, origin at the shoulder, hanging down; the barrel's muzzle at the bottom.
    p = [
        cyl([0, 0, 0], [0, 0, -0.4], 0.07, "rubber"),
        torus([0, 0, -0.1], 0.08, 0.026, "z", "chrome", repeat={"count": 3, "step": [0, 0, -0.1]}),
        # The cannon: a fat brass body, cooling rings, a chrome barrel and a glowing muzzle dish.
        cyl([0, 0, -0.4], [0, 0, -0.7], 0.12, "enamel"),
        torus([0, 0, -0.48], 0.13, 0.02, "z", "brass", repeat={"count": 4, "step": [0, 0, -0.06]}),
        cyl([0, 0, -0.7], [0, 0, -0.98], 0.05, "chrome"),
        lathe([[0.0, -0.98], [0.1, -1.06], [0.1, -1.08], [0.0, -1.03]], "brass"),
        sphere([0, 0, -1.02], 0.04, "glow"),
    ]
    return spec("sentry_gun", p, ["rubber", "chrome", "enamel", "brass", "glow"], 32000, 1.2, 0.003, 2.0)


# ---------------------------------------------------------------------------------------
# The colossus: the Doctor's walking tank, about 16 m tall. A riveted saucer hull on four
# spider legs (thigh up and out to a glowing knee actuator, shin down to a clawed foot), a
# turret with a long cannon, and the Aether core hanging under the belly in a clamshell.
# Weak points (breakable): the four knee actuators, the cannon, then the core.
# The hull's origin is the middle of its underside, 6.5 m over the ground when it stands.

HULL_R = 5.0


def colossus_hull() -> dict:
    p = [
        # The saucer: a low drum, a domed top, a tucked belly.
        lathe([[0.0, 0.0], [3.2, 0.0], [4.6, 0.6], [5.0, 1.3], [5.0, 2.1], [4.6, 2.6],
               [3.6, 3.3], [2.2, 3.6], [0.0, 3.65]], "hull"),
        # A brass band of portholes, glowing green, and rivets.
        torus([0, 0, 1.7], 5.02, 0.12, "z", "brass"),
        sphere([4.98, 0, 1.7], 0.26, "glow", repeat={"around": 14, "start": 13}),
        torus([4.98, 0, 1.7], 0.3, 0.06, "x", "brass", repeat={"around": 14, "start": 13}),
        sphere([4.95, 0, 2.15], 0.05, "brass", segments=12, repeat={"around": 56}),
        sphere([4.95, 0, 1.25], 0.05, "brass", segments=12, repeat={"around": 56}),
        # Armour plates round the top, with dark seams between.
        torus([0, 0, 2.62], 4.55, 0.07, "z", "rubber"),
        torus([0, 0, 3.32], 3.55, 0.06, "z", "rubber"),
        # The face: a grille and two great lamp eyes under brass brows (front is -Y).
        box([3.4, 0.5, 1.0], [0, -4.8, 1.45], "rubber"),
        box([3.3, 0.2, 0.08], [0, -5.03, 1.08], "chrome", repeat={"count": 7, "step": [0, 0, 0.13]}),
        sphere([1.9, -4.6, 2.3], 0.5, "glow", mirror="x"),
        torus([1.9, -4.62, 2.3], 0.55, 0.12, "y", "brass", mirror="x"),
        box([1.3, 0.4, 0.22], [1.9, -4.7, 2.95], "brass", rotate=[0, 12, 0], mirror="x"),
        # Exhaust stacks at the back, belching (the game adds the smoke).
        cyl([1.2, 3.4, 2.6], [1.4, 3.9, 5.2], 0.35, "chrome", mirror="x"),
        torus([1.4, 3.9, 5.1], 0.37, 0.08, "z", "brass", mirror="x"),
        # Hip sockets on the four diagonals.
        sphere([3.2, 0, 0.9], 0.95, "brass", repeat={"around": 4, "start": 45}),
        # The belly ring round the core's clamshell.
        torus([0, 0, 0.0], 2.0, 0.22, "z", "brass"),
        # The turret ring on top.
        cyl([0, 0, 3.5], [0, 0, 3.8], 2.4, "brass"),
    ]
    return spec("colossus_hull", p, ["hull", "brass", "rubber", "chrome", "glow"], 360000, 11.0, 0.02, 1.0)


def colossus_turret() -> dict:
    # Origin on the turret ring; the cannon's trunnion 1.2 m up, 1.9 m forward.
    p = [
        lathe([[0.0, 0.0], [2.3, 0.0], [2.35, 0.4], [2.1, 1.3], [1.4, 2.0], [0.0, 2.1]], "hull"),
        torus([0, 0, 0.42], 2.33, 0.07, "z", "brass"),
        # The mantlet the cannon sits in.
        box([1.6, 0.9, 1.1], [0, -1.9, 1.2], "hull"),
        cyl([-0.9, -1.9, 1.2], [0.9, -1.9, 1.2], 0.42, "brass"),
        # A commander's cupola with a glowing slit, and an aerial.
        cyl([0.9, 0.6, 1.8], [0.9, 0.6, 2.4], 0.5, "hull"),
        box([0.6, 0.12, 0.08], [0.9, 0.13, 2.2], "glow"),
        cyl([-1.0, 0.8, 1.7], [-1.0, 0.9, 4.4], 0.035, "chrome"),
        sphere([-1.0, 0.9, 4.45], 0.09, "glow"),
        sphere([2.25, 0, 0.25], 0.04, "brass", segments=12, repeat={"around": 36}),
    ]
    return spec("colossus_turret", p, ["hull", "brass", "glow", "chrome"], 120000, 6.0, 0.015, 1.0)


def colossus_cannon() -> dict:
    # Origin at the trunnion; the barrel runs 6.5 m forward (-Y).
    p = [
        cyl([0, 0.5, 0], [0, -1.2, 0], 0.55, "hull"),
        cyl([0, -1.2, 0], [0, -6.2, 0], 0.32, "chrome", rb=0.28),
        torus([0, -1.6, 0], 0.36, 0.07, "y", "brass", repeat={"count": 5, "step": [0, -0.9, 0]}),
        # Muzzle brake with vents, and a dark bore.
        cyl([0, -6.1, 0], [0, -6.8, 0], 0.42, "brass"),
        box([0.9, 0.12, 0.16], [0, -6.45, 0], "rubber", repeat={"count": 2, "step": [0, -0.22, 0]}),
        cyl([0, -6.75, 0], [0, -6.82, 0], 0.2, "rubber"),
    ]
    return spec("colossus_cannon", p, ["hull", "chrome", "brass", "rubber"], 80000, 8.0, 0.012, 1.0)


# Leg geometry (in the leg's own frame: +X points out from the hull, Z up). The hip socket is
# the origin; the knee sits up and out; the foot straight under the knee and a little further out.
KNEE = (4.2, 0.0, 4.4)
FOOT = (5.6, 0.0, -6.5)


def colossus_thigh() -> dict:
    kx, _, kz = KNEE
    p = [
        # Twin box girders from hip to knee, braced, with a hydraulic ram under them.
        {"group": [
            cyl([0, 0.35, 0], [kx, 0.35, kz], 0.32, "hull"),
            cyl([0, -0.35, 0], [kx, -0.35, kz], 0.32, "hull"),
            cyl([kx * 0.3, -0.35, kz * 0.3], [kx * 0.3, 0.35, kz * 0.3], 0.16, "brass"),
            cyl([kx * 0.65, -0.35, kz * 0.65], [kx * 0.65, 0.35, kz * 0.65], 0.16, "brass"),
        ]},
        cyl([0.6, 0, -0.5], [kx * 0.55, 0, kz * 0.55 - 0.7], 0.22, "chrome"),
        cyl([kx * 0.55, 0, kz * 0.55 - 0.7], [kx - 0.4, 0, kz - 0.6], 0.3, "brass"),
        sphere([0, 0, 0], 0.8, "chrome"),
        # Knee yoke (the actuator itself is its own part, it can be blown off).
        cyl([kx, -0.85, kz], [kx, 0.85, kz], 0.45, "brass"),
    ]
    return spec("colossus_thigh", p, ["hull", "brass", "chrome"], 120000, 7.5, 0.015, 1.0)


def colossus_shin() -> dict:
    # Origin at the knee; the sole is at FOOT - KNEE.
    fx = FOOT[0] - KNEE[0]
    fz = FOOT[2] - KNEE[2]
    p = [
        cyl([0, 0, 0], [fx * 0.9, 0, fz * 0.9], 0.6, "hull", rb=0.4),
        torus([fx * 0.2, 0, fz * 0.2], 0.58, 0.1, "z", "brass"),
        torus([fx * 0.75, 0, fz * 0.75], 0.45, 0.09, "z", "brass"),
        # Armour fin down the front of the shin.
        cyl([0.3, 0, -0.3], [fx * 0.8 + 0.45, 0, fz * 0.8], 0.18, "brass"),
        # Ankle ball, foot pad and three claws.
        sphere([fx, 0, fz + 0.7], 0.55, "chrome"),
        cyl([fx, 0, fz + 0.45], [fx, 0, fz], 1.25, "hull", rb=1.35),
    ]
    # Claws splayed round the foot (out, forward, back).
    for dx, dy in ((1.0, 0.0), (0.55, 0.85), (0.55, -0.85)):
        p.append(cyl([fx + dx * 1.0, dy * 1.0, fz + 0.25], [fx + dx * 1.9, dy * 1.9, fz + 0.02],
                     0.2, "chrome", rb=0.04))
    return spec("colossus_shin", p, ["hull", "brass", "chrome"], 120000, 13.0, 0.015, 1.0)


def colossus_knee() -> dict:
    # The glowing actuator at the knee: a weak point. Origin at the knee.
    p = [
        sphere([0, 0, 0], 0.75, "glow"),
        torus([0, 0, 0], 0.8, 0.09, "x", "brass"),
        torus([0, 0, 0], 0.8, 0.09, "z", "brass"),
        cyl([0, -1.0, 0], [0, 1.0, 0], 0.2, "chrome"),
    ]
    return spec("colossus_knee", p, ["glow", "brass", "chrome"], 48000, 2.2, 0.01, 1.0)


def colossus_shell() -> dict:
    # The belly hatch: a bowl under the core, hinged at its front rim (the origin), so it
    # swings down and back to open. Closed, its rim meets the belly ring.
    p = [
        lathe([[0.0, -1.9], [0.9, -1.85], [1.6, -1.4], [1.95, -0.6], [2.0, 0.0], [0.0, 0.0]],
              "hull", at=[0, 2.0, 0]),
        torus([0, 2.0, -0.05], 1.98, 0.08, "z", "brass"),
    ]
    return spec("colossus_shell", p, ["hull", "brass"], 48000, 4.2, 0.0, 1.0)


def colossus_core() -> dict:
    # The Aether core in its cage, origin at its middle, hanging 1.3 m under the belly.
    p = [
        sphere([0, 0, 0], 1.0, "glow"),
        torus([0, 0, 0], 1.05, 0.08, "x", "brass"),
        torus([0, 0, 0], 1.05, 0.08, "y", "brass"),
        torus([0, 0, 0], 1.05, 0.08, "z", "brass"),
        cyl([0, 0, 1.0], [0, 0, 1.4], 0.3, "chrome"),
    ]
    return spec("colossus_core", p, ["glow", "brass", "chrome"], 48000, 3.0, 0.01, 1.0)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    makers = [sentry_body, sentry_dish, sentry_thigh, sentry_shin, sentry_arm, sentry_gun,
              colossus_hull, colossus_turret, colossus_cannon, colossus_thigh, colossus_shin,
              colossus_knee, colossus_shell, colossus_core]
    for make in makers:
        s = make()
        (OUT / f"{s['id']}.json").write_text(json.dumps(s, indent=1) + "\n")
        print(s["id"])


if __name__ == "__main__":
    main()
