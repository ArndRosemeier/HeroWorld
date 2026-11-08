//! Heroes and citizens built from the Actors packs. Skipped (with a note)
//! when the packs are not there: `scripts/fetch-packs.sh` gets them.

use engine::anim::sample_joint_matrices;
use hw_client::people::{self, Who, CITIZEN_CLIPS, HERO_CLIPS};

fn builder() -> Option<actors_engine::CharacterBuilder> {
    let Some(dir) = people::packs_dir() else {
        eprintln!("no Actors packs: skipped (run scripts/fetch-packs.sh)");
        return None;
    };
    Some(actors_engine::CharacterBuilder::load(dir).expect("load the packs"))
}

/// A joint's position in model space at `t` seconds into `clip`.
fn joint_at(m: &engine::anim::AnimatedModel, clip: &str, joint: &str, t: f32) -> glam::Vec3 {
    let c = m.clips.iter().position(|c| c.name == clip).expect("clip");
    let j = m.skeleton.joint_index(joint).expect("joint");
    let skin = sample_joint_matrices(m, c, t);
    // Skin matrix = global * inverse bind; the joint sits at global's origin.
    let global = skin[j] * m.skeleton.inverse_bind[j].inverse();
    global.w_axis.truncate()
}

#[test]
fn heroes_have_every_clip_and_fly_fist_first() {
    let Some(mut b) = builder() else { return };
    let hero = people::build(&mut b, &Who::Hero("player 1".into()), 0).expect("hero");
    let m = &hero.model;
    for clip in HERO_CLIPS.iter().chain(&["fly", "hover"]) {
        assert!(m.find_clip(clip).is_some(), "hero has no {clip}");
    }
    for t in [0.0, 0.7, 1.9] {
        let fist = joint_at(m, "fly", "hand_r", t);
        let head = joint_at(m, "fly", "head", t);
        assert!(
            fist.y > head.y + 0.2,
            "the fist leads above the head: {fist} {head}"
        );
        // Hovering, the feet hang below the hips.
        let foot = joint_at(m, "hover", "foot_l", t);
        let hips = joint_at(m, "hover", "pelvis", t);
        assert!(foot.y < hips.y - 0.6);
    }
    // Same name, same hero.
    let again = people::build(&mut b, &Who::Hero("player 1".into()), 0).expect("hero");
    assert_eq!(again.model.meshes.len(), m.meshes.len());
}

#[test]
fn citizens_dress_for_the_era() {
    let Some(mut b) = builder() else { return };
    for look in 0..8 {
        b.allow(&people::allow(false));
        let c = people::golden_age(&b, 1000 + look, false);
        assert!(
            c.outfit.iter().any(|g| g != "briefs" && g != "socks"),
            "citizen {look} is undressed: {:?}",
            c.outfit
        );
        for g in &c.outfit {
            assert!(
                ![
                    "tshirt",
                    "sneakers",
                    "lace-thong",
                    "skirt-mini",
                    "sportsuit"
                ]
                .contains(&g.as_str()),
                "citizen {look} wears {g}"
            );
        }
    }
    let one = people::build(&mut b, &Who::Citizen(3), 1).expect("citizen");
    for clip in CITIZEN_CLIPS {
        assert!(one.model.find_clip(clip).is_some(), "citizen has no {clip}");
    }
}
