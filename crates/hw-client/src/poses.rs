//! Clips the packs don't have, posed here on a character's own skeleton:
//! flying ("fly", the classic one fist ahead), hovering ("hover", upright,
//! legs together, arms easy at the sides) and the hero's stances.
//!
//! Each is laid over the idle clip, so the breathing and the small shifts
//! of weight stay. Limbs are posed by aiming bones in the character's own
//! space (+Y up, +Z forward, +X its left), which works whatever the rig's
//! bone rolls; the joints in between follow. A flier is drawn upright here:
//! the game tips the whole body into the flight direction.

use engine::anim::{AnimatedModel, AnimationClip, JointChannels, QuatTrack, Vec3Track};
use glam::{Quat, Vec3};

/// One aim: point `bone` (towards `child`) along `dir` in model space.
struct Aim {
    bone: &'static str,
    child: &'static str,
    dir: Vec3,
}

const fn aim(bone: &'static str, child: &'static str, x: f32, y: f32, z: f32) -> Aim {
    Aim {
        bone,
        child,
        dir: Vec3::new(x, y, z),
    }
}

/// Adds "fly" and "hover" made from the model's "idle".
pub fn add_flight_clips(model: &mut AnimatedModel) -> Result<(), String> {
    let fly = make_clip(model, "fly", 2.4, |t| {
        // A slow flutter of the legs and the trailing arm.
        let w = (t * std::f32::consts::TAU).sin();
        let v = (t * std::f32::consts::TAU + 1.3).sin();
        Pose {
            aims: vec![
                // The leading fist, straight ahead of the head.
                aim("upperarm_r", "lowerarm_r", -0.12, 1.0, 0.16),
                aim("lowerarm_r", "hand_r", -0.08, 1.0, 0.12),
                // The other arm along the body, a little bent.
                aim("upperarm_l", "lowerarm_l", 0.22, -1.0, -0.12 + 0.03 * v),
                aim("lowerarm_l", "hand_l", 0.12, -1.0, 0.18),
                // Legs together, one knee bent, toes pointed.
                aim("thigh_l", "calf_l", 0.05, -1.0, -0.04 + 0.03 * w),
                aim("calf_l", "foot_l", 0.04, -1.0, -0.18 - 0.05 * w),
                aim("thigh_r", "calf_r", -0.04, -1.0, 0.1 - 0.03 * w),
                aim("calf_r", "foot_r", -0.03, -1.0, -0.55 + 0.06 * w),
                aim("foot_l", "ball_l", 0.0, -1.0, 0.35),
                aim("foot_r", "ball_r", 0.0, -1.0, 0.3),
            ],
            // The head up to look where it flies (the body lies flat).
            turns: vec![("neck_01", Vec3::X, -0.55), ("head", Vec3::X, -0.35)],
            lift: 0.0,
            seated: false,
            lay: None,
        }
    })?;
    let hover = make_clip(model, "hover", 3.2, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        let v = (t * std::f32::consts::TAU + 0.8).sin();
        Pose {
            aims: vec![
                // Upright and at ease on the air: arms down and a little
                // out, drifting, fists loose.
                aim("upperarm_l", "lowerarm_l", 0.3, -1.0, 0.03 + 0.03 * v),
                aim("lowerarm_l", "hand_l", 0.2, -1.0, 0.16 + 0.04 * v),
                aim("upperarm_r", "lowerarm_r", -0.3, -1.0, 0.03 - 0.03 * v),
                aim("lowerarm_r", "hand_r", -0.2, -1.0, 0.16 - 0.04 * v),
                // Legs long and together, one knee a touch soft, toes
                // pointed down: no crouch.
                aim("thigh_l", "calf_l", 0.03, -1.0, 0.02 + 0.015 * w),
                aim("calf_l", "foot_l", 0.02, -1.0, -0.05 - 0.015 * w),
                aim("thigh_r", "calf_r", -0.03, -1.0, 0.07 - 0.015 * w),
                aim("calf_r", "foot_r", -0.02, -1.0, -0.13 + 0.015 * w),
                aim("foot_l", "ball_l", 0.0, -1.0, 0.45),
                aim("foot_r", "ball_r", 0.0, -1.0, 0.5),
            ],
            // Chest out, chin up.
            turns: vec![
                ("spine_03", Vec3::X, -0.06 - 0.01 * v),
                ("head", Vec3::X, -0.08),
            ],
            lift: 0.04 * w,
            seated: false,
            lay: None,
        }
    })?;
    model.clips.retain(|c| c.name != "fly" && c.name != "hover");
    model.clips.push(fly);
    model.clips.push(hover);
    Ok(())
}

/// Adds "heroic": the classic hero's stance for the title and the creator,
/// fists on the hips, feet apart, chest out and chin up, breathing slowly.
pub fn add_heroic_clip(model: &mut AnimatedModel) -> Result<(), String> {
    let heroic = make_clip(model, "heroic", 4.0, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        Pose {
            aims: vec![
                // Elbows out, fists on the hips.
                aim("upperarm_l", "lowerarm_l", 0.8, -0.55, -0.22 + 0.02 * w),
                aim("lowerarm_l", "hand_l", -0.5, -0.8, -0.12),
                aim("upperarm_r", "lowerarm_r", -0.8, -0.55, -0.22 + 0.02 * w),
                aim("lowerarm_r", "hand_r", 0.5, -0.8, -0.12),
                // Feet planted apart.
                aim("thigh_l", "calf_l", 0.2, -1.0, 0.0),
                aim("calf_l", "foot_l", 0.17, -1.0, -0.03),
                aim("thigh_r", "calf_r", -0.2, -1.0, 0.0),
                aim("calf_r", "foot_r", -0.17, -1.0, -0.03),
                aim("foot_l", "ball_l", 0.15, -0.3, 1.0),
                aim("foot_r", "ball_r", -0.15, -0.3, 1.0),
            ],
            // Chest out, chin up.
            turns: vec![
                ("spine_03", Vec3::X, -0.08 - 0.015 * w),
                ("head", Vec3::X, -0.14),
            ],
            lift: -0.02,
            seated: false,
            lay: None,
        }
    })?;
    model.clips.retain(|c| c.name != "heroic");
    model.clips.push(heroic);
    Ok(())
}

/// Adds "power pose": a fist punched up to the sky, the other on the hip,
/// chin up to follow it, for a level up. Played once; it ends held.
pub fn add_power_pose_clip(model: &mut AnimatedModel) -> Result<(), String> {
    let clip = make_clip(model, "power pose", 2.4, |t| {
        // The punch goes up in the first fifth, then holds, breathing.
        let k = (t / 0.2).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let up = |from: Vec3, to: Vec3| from.lerp(to, k);
        let b = (t * std::f32::consts::TAU * 1.5).sin();
        // Out to the side of the head, so the arm reads from every side.
        let shoulder = up(Vec3::new(-0.7, 0.2, 0.5), Vec3::new(-0.42, 1.0, 0.1));
        let fore = up(Vec3::new(-0.1, 0.9, 0.5), Vec3::new(-0.14, 1.0, 0.04));
        Pose {
            aims: vec![
                aim(
                    "upperarm_r",
                    "lowerarm_r",
                    shoulder.x,
                    shoulder.y,
                    shoulder.z,
                ),
                aim("lowerarm_r", "hand_r", fore.x, fore.y, fore.z),
                // The other fist on the hip.
                aim("upperarm_l", "lowerarm_l", 0.8, -0.55, -0.2 + 0.02 * b),
                aim("lowerarm_l", "hand_l", -0.5, -0.8, -0.12),
                // Feet planted apart.
                aim("thigh_l", "calf_l", 0.2, -1.0, 0.0),
                aim("calf_l", "foot_l", 0.17, -1.0, -0.03),
                aim("thigh_r", "calf_r", -0.2, -1.0, 0.0),
                aim("calf_r", "foot_r", -0.17, -1.0, -0.03),
                aim("foot_l", "ball_l", 0.15, -0.3, 1.0),
                aim("foot_r", "ball_r", -0.15, -0.3, 1.0),
            ],
            // Chest out, the head tipped back to the fist.
            turns: vec![
                ("spine_03", Vec3::X, -0.1 - 0.015 * b),
                ("head", Vec3::X, -0.12 - 0.2 * k),
            ],
            lift: -0.02,
            seated: false,
            lay: None,
        }
    })?;
    model.clips.retain(|c| c.name != "power pose");
    model.clips.push(clip);
    Ok(())
}

/// Adds "cuffed": hands bound behind the back in energy cuffs, legs
/// dangling, a slow sway, for a villain hanging from a police drone.
pub fn add_cuffed_clip(model: &mut AnimatedModel) -> Result<(), String> {
    let cuffed = make_clip(model, "cuffed", 3.0, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        Pose {
            aims: vec![
                aim("upperarm_l", "lowerarm_l", 0.18, -0.85, -0.45),
                aim("lowerarm_l", "hand_l", -0.6, -0.3, -0.5),
                aim("upperarm_r", "lowerarm_r", -0.18, -0.85, -0.45),
                aim("lowerarm_r", "hand_r", 0.6, -0.3, -0.5),
                aim("thigh_l", "calf_l", 0.05, -1.0, 0.1 + 0.08 * w),
                aim("calf_l", "foot_l", 0.03, -1.0, -0.2 + 0.05 * w),
                aim("thigh_r", "calf_r", -0.05, -1.0, 0.05 - 0.08 * w),
                aim("calf_r", "foot_r", -0.03, -1.0, -0.25 - 0.05 * w),
                aim("foot_l", "ball_l", 0.0, -1.0, 0.5),
                aim("foot_r", "ball_r", 0.0, -1.0, 0.5),
            ],
            // Head hung low.
            turns: vec![("head", Vec3::X, 0.35)],
            lift: 0.0,
            seated: false,
            lay: None,
        }
    })?;
    model.clips.retain(|c| c.name != "cuffed");
    model.clips.push(cuffed);
    Ok(())
}

/// Adds "hands up": someone being robbed, hands raised beside the head,
/// shoulders hunched, leaning away, trembling a little.
pub fn add_hands_up_clip(model: &mut AnimatedModel) -> Result<(), String> {
    let up = make_clip(model, "hands up", 1.6, |t| {
        let w = (t * std::f32::consts::TAU * 3.0).sin() * 0.03;
        Pose {
            aims: vec![
                // Upper arms out and up, forearms straight up, palms out.
                aim("upperarm_l", "lowerarm_l", 0.95, 0.1 + w, 0.2),
                aim("lowerarm_l", "hand_l", 0.1, 1.0, 0.15 + w),
                aim("upperarm_r", "lowerarm_r", -0.95, 0.1 - w, 0.2),
                aim("lowerarm_r", "hand_r", -0.1, 1.0, 0.15 - w),
                // Knees a little bent, one foot back.
                aim("thigh_l", "calf_l", 0.08, -1.0, 0.08),
                aim("calf_l", "foot_l", 0.06, -1.0, -0.1),
                aim("thigh_r", "calf_r", -0.08, -1.0, -0.12),
                aim("calf_r", "foot_r", -0.06, -1.0, -0.2),
            ],
            // Leaning back from the muggers, head turned half away.
            turns: vec![("spine_03", Vec3::X, -0.12), ("head", Vec3::Y, 0.35)],
            lift: -0.04,
            seated: false,
            lay: None,
        }
    })?;
    model.clips.retain(|c| c.name != "hands up");
    model.clips.push(up);
    Ok(())
}

/// The seated clips, "drive" (hands on the wheel) and "ride" (hands in the
/// lap), for people in cars. Their hips are held still at the model's
/// origin, so a rider is placed by its hip point on the seat (see
/// [`crate::carmesh::seats`]); thighs and shins run forward, the back
/// leans into the backrest. Only the head and the hands
/// move a little.
pub fn add_seated_clips(model: &mut AnimatedModel) -> Result<(), String> {
    // Short loops: each is baked for every look's crowd, and a sitter
    // hardly moves.
    let legs = || {
        vec![
            // Knees a little apart, thighs along the cushion.
            aim("thigh_l", "calf_l", 0.12, 0.02, 1.0),
            aim("thigh_r", "calf_r", -0.12, 0.02, 1.0),
            // Shins stretched out forward to the pedals, low cars' way
            // (a hover car's floor is a hand under the seat).
            aim("calf_l", "foot_l", 0.03, -0.15, 0.99),
            aim("calf_r", "foot_r", -0.03, -0.14, 0.99),
            aim("foot_l", "ball_l", 0.06, -0.15, 1.0),
            aim("foot_r", "ball_r", -0.06, -0.15, 1.0),
        ]
    };
    let drive = make_clip(model, "drive", 3.0, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        let mut aims = legs();
        aims.extend([
            // Forearms out to the wheel at ten to two.
            aim("upperarm_l", "lowerarm_l", 0.1, -0.62, 0.78),
            aim("lowerarm_l", "hand_l", -0.22, 0.12 + 0.02 * w, 1.0),
            aim("upperarm_r", "lowerarm_r", -0.1, -0.62, 0.78),
            aim("lowerarm_r", "hand_r", 0.22, 0.12 - 0.02 * w, 1.0),
        ]);
        Pose {
            aims,
            turns: vec![("head", Vec3::Y, 0.06 * w)],
            lift: 0.0,
            seated: true,
            lay: None,
        }
    })?;
    let ride = make_clip(model, "ride", 3.0, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        let mut aims = legs();
        aims.extend([
            // Hands resting on the thighs.
            aim("upperarm_l", "lowerarm_l", 0.14, -0.92, 0.32),
            aim("lowerarm_l", "hand_l", -0.04, -0.42, 1.0),
            aim("upperarm_r", "lowerarm_r", -0.14, -0.92, 0.32),
            aim("lowerarm_r", "hand_r", 0.04, -0.42, 1.0),
        ]);
        Pose {
            aims,
            // Watching the street go by.
            turns: vec![("head", Vec3::Y, 0.14 * w)],
            lift: 0.0,
            seated: true,
            lay: None,
        }
    })?;
    model
        .clips
        .retain(|c| c.name != "drive" && c.name != "ride");
    model.clips.push(drive);
    model.clips.push(ride);
    Ok(())
}

/// The rescue clips for townsfolk: "wave" (trapped, an arm up waving for
/// help), "beckon" (the medic waving the hero over), "lie" (hurt, on
/// their back on the ground; the model's origin is the ground under the
/// hips, the head towards -Z) and "carried" (in a hero's arms: tipped back,
/// knees over one arm, an arm hanging; the hips at the origin, the legs
/// towards +Z), and "thanks" (a hand on the heart and a wave, for their
/// hero).
pub fn add_rescue_clips(model: &mut AnimatedModel) -> Result<(), String> {
    use std::f32::consts::TAU;
    let wave = make_clip(model, "wave", 1.2, |t| {
        // Two waves a loop, side to side over the head.
        let w = (t * TAU * 2.0).sin();
        Pose {
            aims: vec![
                aim("upperarm_r", "lowerarm_r", -0.35, 0.95, 0.12),
                aim("lowerarm_r", "hand_r", -0.12 - 0.5 * w, 1.0, 0.08),
                // The other hand held out, pleading.
                aim("upperarm_l", "lowerarm_l", 0.45, -0.35, 0.6),
                aim("lowerarm_l", "hand_l", 0.15, 0.25, 1.0),
            ],
            turns: vec![("head", Vec3::X, -0.12), ("head", Vec3::Y, 0.1 * w)],
            lift: 0.0,
            seated: false,
            lay: None,
        }
    })?;
    let beckon = make_clip(model, "beckon", 1.2, |t| {
        // "Over here!": the forearm swings up and back towards the chest.
        let k = 0.5 + 0.5 * (t * TAU * 2.0).sin();
        Pose {
            aims: vec![
                aim("upperarm_r", "lowerarm_r", -0.6, 0.05, 0.8),
                aim("lowerarm_r", "hand_r", -0.15, 0.8, 0.5 - 0.9 * k),
            ],
            turns: vec![("head", Vec3::Y, -0.15)],
            lift: 0.0,
            seated: false,
            lay: None,
        }
    })?;
    let lie = make_clip(model, "lie", 2.0, |t| {
        let w = (t * TAU).sin();
        Pose {
            aims: vec![
                // Arms along the sides, a little out; one knee drawn up.
                aim("upperarm_l", "lowerarm_l", 0.3, -1.0, 0.0),
                aim("lowerarm_l", "hand_l", 0.25, -1.0, 0.12),
                aim("upperarm_r", "lowerarm_r", -0.3, -1.0, 0.0),
                aim("lowerarm_r", "hand_r", -0.2, -1.0, 0.2),
                aim("thigh_l", "calf_l", 0.1, -1.0, 0.0),
                aim("calf_l", "foot_l", 0.08, -1.0, 0.0),
                aim("thigh_r", "calf_r", -0.08, -1.0, 0.45),
                aim("calf_r", "foot_r", -0.05, -1.0, -0.4),
            ],
            // On the back (tipped back a quarter turn about the hips), the
            // head rolled to one side, moving a little.
            turns: vec![
                ("pelvis", Vec3::X, -std::f32::consts::FRAC_PI_2),
                ("head", Vec3::Z, 0.45 + 0.05 * w),
            ],
            lift: 0.0,
            seated: false,
            lay: Some(0.13),
        }
    })?;
    let carried = make_clip(model, "carried", 2.0, |t| {
        // Swaying with the hero's steps.
        let w = (t * TAU).sin();
        // Aimed as if standing, then tipped back 72 degrees about the
        // hips: the thighs come out level over the hero's arm, the shins
        // hang, one arm hangs limp and the other lies across the belly.
        Pose {
            aims: vec![
                aim("thigh_l", "calf_l", 0.1, -0.78, 0.63),
                aim("thigh_r", "calf_r", -0.06, -0.78, 0.63),
                aim("calf_l", "foot_l", 0.05, -0.46, -0.9 + 0.05 * w),
                aim("calf_r", "foot_r", -0.03, -0.46, -0.9 - 0.05 * w),
                aim("upperarm_l", "lowerarm_l", 0.15, -0.22, -0.98),
                aim("lowerarm_l", "hand_l", 0.05, -0.2 + 0.08 * w, -1.0),
                aim("upperarm_r", "lowerarm_r", -0.2, -0.87, 0.39),
                aim("lowerarm_r", "hand_r", 0.9, -0.08, 0.4),
            ],
            turns: vec![
                ("pelvis", Vec3::X, -1.25),
                ("neck_01", Vec3::X, -0.25),
                ("head", Vec3::X, -0.3 - 0.05 * w),
            ],
            lift: 0.0,
            seated: false,
            lay: Some(0.0),
        }
    })?;
    let thanks = make_clip(model, "thanks", 1.4, |t| {
        // A hand on the heart, the other raised in a happy little wave,
        // a small bow of the head and shoulders.
        let w = (t * TAU * 2.0).sin();
        Pose {
            aims: vec![
                aim("upperarm_r", "lowerarm_r", -0.25, -0.75, 0.45),
                aim("lowerarm_r", "hand_r", 0.75, 0.45, 0.35),
                aim("upperarm_l", "lowerarm_l", 0.55, 0.35, 0.45),
                aim("lowerarm_l", "hand_l", 0.15 + 0.35 * w, 1.0, 0.2),
            ],
            turns: vec![("spine_03", Vec3::X, 0.12), ("head", Vec3::X, 0.1)],
            lift: 0.0,
            seated: false,
            lay: None,
        }
    })?;
    let names = ["wave", "beckon", "lie", "carried", "thanks"];
    model.clips.retain(|c| !names.contains(&c.name.as_str()));
    model.clips.extend([wave, beckon, lie, carried, thanks]);
    Ok(())
}

/// The hero's clips for carrying someone hurt in both arms: "carry idle",
/// "carry walk", "carry jog" and "carry sprint", each its plain clip with
/// the arms held forward as a cradle at chest height (see
/// [`add_rescue_clips`]'s "carried") and the back leaning a little against
/// the weight.
pub fn add_carry_clips(model: &mut AnimatedModel) -> Result<(), String> {
    let mut made = Vec::new();
    for base in ["idle", "walk", "jog", "sprint"] {
        if !model.clips.iter().any(|c| c.name == base) {
            continue;
        }
        let name = format!("carry {base}");
        let clip = make_clip_over(model, base, &name, None, |_| Pose {
            aims: vec![
                aim("upperarm_l", "lowerarm_l", 0.35, -0.55, 0.75),
                aim("lowerarm_l", "hand_l", -0.55, 0.15, 0.82),
                aim("upperarm_r", "lowerarm_r", -0.35, -0.55, 0.75),
                aim("lowerarm_r", "hand_r", 0.55, 0.15, 0.82),
            ],
            turns: vec![("spine_03", Vec3::X, -0.06)],
            lift: 0.0,
            seated: false,
            lay: None,
        })?;
        made.push(clip);
    }
    let names: Vec<String> = made.iter().map(|c| c.name.clone()).collect();
    model.clips.retain(|c| !names.contains(&c.name));
    model.clips.extend(made);
    Ok(())
}

/// The clip a hero carrying someone plays for its plain clip `clip`.
pub fn carry_clip(clip: &str) -> Option<&'static str> {
    Some(match clip {
        "idle" => "carry idle",
        "walk" => "carry walk",
        "jog" => "carry jog",
        "sprint" => "carry sprint",
        _ => return None,
    })
}

/// How a sitter leans back (bone, radians about its left-right axis; a
/// negative angle tips it back) and holds its head up again.
const SEATED_LEAN: [(&str, f32); 4] = [
    ("spine_01", -0.27),
    ("spine_02", -0.14),
    ("neck_01", 0.22),
    ("head", 0.16),
];

/// A pose at one moment: aims, then turns of whole joints (axis in model
/// space, radians), then a lift of the hips. `seated`: the hips are held
/// still (no shifting of weight) with the pelvis at the model's origin.
struct Pose {
    aims: Vec<Aim>,
    turns: Vec<(&'static str, Vec3, f32)>,
    lift: f32,
    seated: bool,
    /// Last of all, the hips moved to this height over the model's origin
    /// (a body laid down: see [`add_rescue_clips`]).
    lay: Option<f32>,
}

/// Gives the posed clips' capes their swing (Actors simulates the cloth
/// once per clip): in level flight the air streams from the head to the feet
/// (the game pitches the body about 80 degrees forward), hovering is nearly
/// still, and the heroic stance gets a breeze from the front so the cape
/// billows. A hero without a cape is left as is.
pub fn swing_cape(c: &mut actors_engine::CharacterModel) {
    use actors_engine::{swing_cape, CapeWind};
    let Some(rig) = c.cape.clone() else { return };
    let winds = [
        ("fly", Vec3::new(0.0, 0.99, 0.14) * 14.0),
        ("hover", Vec3::new(0.0, 0.3, 0.6)),
        ("heroic", Vec3::new(0.6, 0.0, 2.6)),
        ("power pose", Vec3::new(0.8, -0.4, 3.2)),
    ];
    for (name, travel) in winds {
        if let Some(i) = c.model.clips.iter().position(|k| k.name == name) {
            swing_cape(
                &mut c.model,
                &rig,
                i,
                CapeWind {
                    travel,
                    root_motion: false,
                },
            );
        }
    }
}

/// Samples "idle" `duration` seconds long at 30 fps and lays `pose(t)` over
/// it (`t` runs 0..1 over the loop).
fn make_clip(
    model: &AnimatedModel,
    name: &str,
    duration: f32,
    pose: impl Fn(f32) -> Pose,
) -> Result<AnimationClip, String> {
    make_clip_over(model, "idle", name, duration, pose)
}

/// [`make_clip`] over clip `base` (`duration`: `None` for the base's own
/// length, so a walk still loops on its steps).
fn make_clip_over(
    model: &AnimatedModel,
    base: &str,
    name: &str,
    duration: impl Into<Option<f32>>,
    pose: impl Fn(f32) -> Pose,
) -> Result<AnimationClip, String> {
    let base = model
        .clips
        .iter()
        .find(|c| c.name == base)
        .ok_or(format!("the model has no {base} clip"))?;
    let duration = duration.into().unwrap_or(base.duration).max(0.1);
    let sk = &model.skeleton;
    let n = sk.joint_names.len();
    let joint = |name: &str| {
        sk.joint_names
            .iter()
            .position(|j| j == name)
            .ok_or(format!("no bone {name}"))
    };
    let frames = (duration * 30.0).round() as usize;
    let mut times = Vec::with_capacity(frames + 1);
    let mut rot: Vec<Vec<Quat>> = vec![Vec::with_capacity(frames + 1); n];
    let mut pos: Vec<Vec<Vec3>> = vec![Vec::with_capacity(frames + 1); n];
    for f in 0..=frames {
        let t = f as f32 / frames as f32;
        // The idle loops at its own length; sample it over ours.
        let bt = (t * duration).rem_euclid(base.duration.max(1e-3));
        let mut locals: Vec<(Vec3, Quat)> = (0..n)
            .map(|j| {
                let (bp, br, _) = model.bind_local[j];
                let ch = &base.channels[j];
                let p = ch.translation.as_ref().map_or(bp, |tr| sample_v(tr, bt));
                let r = ch.rotation.as_ref().map_or(br, |tr| sample_q(tr, bt));
                (p, r)
            })
            .collect();
        let p = pose(t);
        if p.seated {
            // Hips still at the origin: the bind pose's pelvis, lowered by
            // its own height.
            if let Ok(pelvis) = joint("pelvis") {
                let (bp, br, _) = model.bind_local[pelvis];
                locals[pelvis] = (bp, br);
                let g = globals(sk, &locals);
                let parent = match sk.parents[pelvis] {
                    Some(pp) => g[pp].1,
                    None => sk.root_transforms[pelvis].to_scale_rotation_translation().1,
                };
                locals[pelvis].0 += parent.inverse() * Vec3::new(0.0, -g[pelvis].0.y, 0.0);
            }
        }
        if p.seated {
            // Leaning back into the seat (a car seat's back is tipped
            // about 20 degrees), the head held up to look ahead. Before
            // the aims, which then set the limbs in the leaning body.
            for (name, angle) in SEATED_LEAN {
                let b = joint(name)?;
                let g = globals(sk, &locals);
                turn(
                    sk,
                    &mut locals,
                    &g,
                    b,
                    Quat::from_axis_angle(Vec3::X, angle),
                );
            }
        }
        for a in &p.aims {
            let (b, c) = (joint(a.bone)?, joint(a.child)?);
            let g = globals(sk, &locals);
            let from = (g[c].0 - g[b].0).normalize_or_zero();
            let to = a.dir.normalize_or_zero();
            if from == Vec3::ZERO || to == Vec3::ZERO {
                continue;
            }
            turn(sk, &mut locals, &g, b, Quat::from_rotation_arc(from, to));
        }
        for (name, axis, angle) in &p.turns {
            let b = joint(name)?;
            let g = globals(sk, &locals);
            turn(sk, &mut locals, &g, b, Quat::from_axis_angle(*axis, *angle));
        }
        if let Ok(pelvis) = joint("pelvis") {
            locals[pelvis].0.y += p.lift;
            if let Some(h) = p.lay {
                let g = globals(sk, &locals);
                let parent = match sk.parents[pelvis] {
                    Some(pp) => g[pp].1,
                    None => sk.root_transforms[pelvis].to_scale_rotation_translation().1,
                };
                let at = g[pelvis].0;
                locals[pelvis].0 += parent.inverse() * Vec3::new(-at.x, h - at.y, -at.z);
            }
        }
        times.push(t * duration);
        for j in 0..n {
            rot[j].push(locals[j].1);
            pos[j].push(locals[j].0);
        }
    }
    let channels = (0..n)
        .map(|j| JointChannels {
            translation: Some(Vec3Track {
                times: times.clone(),
                values: std::mem::take(&mut pos[j]),
            }),
            rotation: Some(QuatTrack {
                times: times.clone(),
                values: std::mem::take(&mut rot[j]),
            }),
            scale: base.channels[j].scale.clone(),
        })
        .collect();
    Ok(AnimationClip {
        name: name.into(),
        duration,
        channels,
    })
}

/// Turns joint `j` by `r` (model space) about itself: its local rotation
/// becomes parent⁻¹ · r · parent · local.
fn turn(
    sk: &engine::anim::Skeleton,
    locals: &mut [(Vec3, Quat)],
    g: &[(Vec3, Quat)],
    j: usize,
    r: Quat,
) {
    let parent = sk.parents[j].map_or(Quat::IDENTITY, |p| g[p].1);
    locals[j].1 = (parent.inverse() * r * parent * locals[j].1).normalize();
}

/// Model-space position and rotation of every joint (parents come first).
fn globals(sk: &engine::anim::Skeleton, locals: &[(Vec3, Quat)]) -> Vec<(Vec3, Quat)> {
    let mut out: Vec<(Vec3, Quat)> = vec![(Vec3::ZERO, Quat::IDENTITY); locals.len()];
    let mut done = vec![false; locals.len()];
    fn visit(
        j: usize,
        sk: &engine::anim::Skeleton,
        locals: &[(Vec3, Quat)],
        out: &mut [(Vec3, Quat)],
        done: &mut [bool],
    ) {
        if done[j] {
            return;
        }
        let (lp, lr) = locals[j];
        out[j] = match sk.parents[j] {
            Some(p) => {
                visit(p, sk, locals, out, done);
                let (pp, pr) = out[p];
                (pp + pr * lp, pr * lr)
            }
            None => {
                let root = sk.root_transforms[j];
                let (_, rr, rt) = root.to_scale_rotation_translation();
                (rt + rr * lp, rr * lr)
            }
        };
        done[j] = true;
    }
    for j in 0..locals.len() {
        visit(j, sk, locals, &mut out, &mut done);
    }
    out
}

fn span(times: &[f32], t: f32) -> (usize, usize, f32) {
    if times.len() < 2 || t <= times[0] {
        return (0, 0, 0.0);
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return (last, last, 0.0);
    }
    let i = times
        .partition_point(|&x| x <= t)
        .saturating_sub(1)
        .min(last - 1);
    let f = (t - times[i]) / (times[i + 1] - times[i]).max(1e-6);
    (i, i + 1, f)
}

fn sample_v(tr: &Vec3Track, t: f32) -> Vec3 {
    let (a, b, f) = span(&tr.times, t);
    tr.values[a].lerp(tr.values[b], f)
}

fn sample_q(tr: &QuatTrack, t: f32) -> Quat {
    let (a, b, f) = span(&tr.times, t);
    tr.values[a].slerp(tr.values[b], f)
}
