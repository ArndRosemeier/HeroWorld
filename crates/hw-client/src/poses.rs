//! Clips the packs don't have, posed here on a character's own skeleton:
//! flying ("fly", the classic one fist ahead) and hovering ("hover", legs
//! loose, arms out for balance).
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
        }
    })?;
    let hover = make_clip(model, "hover", 3.2, |t| {
        let w = (t * std::f32::consts::TAU).sin();
        let v = (t * std::f32::consts::TAU + 0.8).sin();
        Pose {
            aims: vec![
                // Arms a little out, open hands, as if treading air.
                aim("upperarm_l", "lowerarm_l", 0.5, -1.0, 0.08 + 0.05 * v),
                aim("lowerarm_l", "hand_l", 0.35, -1.0, 0.45 + 0.08 * v),
                aim("upperarm_r", "lowerarm_r", -0.5, -1.0, 0.08 - 0.05 * v),
                aim("lowerarm_r", "hand_r", -0.35, -1.0, 0.45 - 0.08 * v),
                // One knee forward, the other leg hanging back, toes down.
                aim("thigh_l", "calf_l", 0.07, -1.0, -0.05 + 0.04 * w),
                aim("calf_l", "foot_l", 0.06, -1.0, -0.35 - 0.04 * w),
                aim("thigh_r", "calf_r", -0.05, -1.0, 0.3 - 0.04 * w),
                aim("calf_r", "foot_r", -0.04, -1.0, -0.12 + 0.04 * w),
                aim("foot_l", "ball_l", 0.0, -1.0, 0.6),
                aim("foot_r", "ball_r", 0.0, -1.0, 0.75),
            ],
            turns: vec![],
            lift: 0.05 * (t * std::f32::consts::TAU).sin(),
        }
    })?;
    model.clips.retain(|c| c.name != "fly" && c.name != "hover");
    model.clips.push(fly);
    model.clips.push(hover);
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
        }
    })?;
    model.clips.retain(|c| c.name != "cuffed");
    model.clips.push(cuffed);
    Ok(())
}

/// A pose at one moment: aims, then turns of whole joints (axis in model
/// space, radians), then a lift of the hips.
struct Pose {
    aims: Vec<Aim>,
    turns: Vec<(&'static str, Vec3, f32)>,
    lift: f32,
}

/// Samples "idle" `duration` seconds long at 30 fps and lays `pose(t)` over
/// it (`t` runs 0..1 over the loop).
fn make_clip(
    model: &AnimatedModel,
    name: &str,
    duration: f32,
    pose: impl Fn(f32) -> Pose,
) -> Result<AnimationClip, String> {
    let base = model
        .clips
        .iter()
        .find(|c| c.name == "idle")
        .ok_or("the model has no idle clip")?;
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
