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
    let hero = people::build(&mut b, &Who::Hero("player 1".into()), 0, None).expect("hero");
    let m = &hero.model;
    for clip in HERO_CLIPS.iter().chain(&["fly", "hover", "heroic"]) {
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
    // Standing heroic: fists at the hips, elbows out wide.
    let hand = joint_at(m, "heroic", "hand_l", 1.0);
    let elbow = joint_at(m, "heroic", "lowerarm_l", 1.0);
    let hips = joint_at(m, "heroic", "pelvis", 1.0);
    assert!(
        (hand.y - hips.y).abs() < 0.25,
        "fist at the hip: {hand} {hips}"
    );
    assert!(elbow.x > hand.x + 0.1, "elbow out: {elbow} {hand}");
    // Same name, same hero.
    let again = people::build(&mut b, &Who::Hero("player 1".into()), 0, None).expect("hero");
    assert_eq!(again.model.meshes.len(), m.meshes.len());
}

#[test]
fn citizens_dress_for_the_era() {
    let Some(mut b) = builder() else { return };
    for look in 0..8 {
        b.allow(&people::allow(false));
        let c = people::golden_age(b.catalog(), 1000 + look, false);
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
    let one = people::build(&mut b, &Who::Citizen(3), 1, None).expect("citizen");
    for clip in CITIZEN_CLIPS {
        assert!(one.model.find_clip(clip).is_some(), "citizen has no {clip}");
    }
}

#[test]
fn the_creator_dresses_heroes_for_the_era() {
    use hw_client::creator::Creator;
    use hw_client::looks;
    let Some(mut b) = builder() else { return };
    b.allow(&people::allow(true));
    let cat = people::hero_catalog(b.catalog());
    for g in ["tshirt", "lace-thong", "skirt-mini", "sportsuit"] {
        assert!(cat.garment(g).is_none(), "the creator offers {g}");
    }
    assert!(!cat.outfits.is_empty(), "no outfits to pick");
    eprintln!(
        "hero outfits: {:?}",
        cat.outfits.iter().map(|o| &o.label).collect::<Vec<_>>()
    );
    eprintln!(
        "hero garments: {:?}",
        cat.garments.iter().map(|g| &g.id).collect::<Vec<_>>()
    );
    for seed in 0..6 {
        let l = Creator::random_look(&cat, seed);
        assert!(!l.anatomy);
        assert!(l.outfit.iter().all(|g| cat.garment(g).is_some()));
        assert!(
            l.outfit
                .iter()
                .any(|g| cat.garment(g).is_some_and(|g| g.covers_anatomy)),
            "hero {seed} is undressed: {:?}",
            l.outfit
        );
        // Same seed, same hero.
        assert_eq!(l, Creator::random_look(&cat, seed));
    }
    // Looks from a friend are kept to the rules: nobody runs round bare.
    let mut bare = Creator::random_look(&cat, 1);
    bare.outfit.clear();
    bare.anatomy = true;
    bare.body.age_years = 3.0;
    let kept = looks::hero_rules(&cat, &bare);
    assert!(!kept.anatomy);
    assert!(kept.outfit.iter().any(|g| g == "briefs"));
    assert!(kept.body.age_years >= 18.0);
    // A made hero builds, from its compact JSON, with every clip.
    let made = Creator::random_look(&cat, 4);
    let bytes = looks::encode(&made);
    let back = looks::decode(&bytes).expect("looks");
    let hero = people::build(&mut b, &Who::Look(looks::key_of(&bytes)), 0, Some(&back))
        .expect("made hero");
    for clip in HERO_CLIPS.iter().chain(&["fly", "hover", "heroic"]) {
        assert!(
            hero.model.find_clip(clip).is_some(),
            "made hero has no {clip}"
        );
    }
}

#[test]
fn people_in_cars_sit_with_their_heads_under_the_roof() {
    use hw_client::carmesh::{floor_under, roof_over, seats};
    use hw_gen::urban::props::CarModel;
    let Some(mut b) = builder() else { return };
    let mut least = std::collections::BTreeMap::new();
    for look in 0..hw_sim::citizens::LOOKS {
        let c = people::build(&mut b, &Who::Citizen(look), 1, None).expect("citizen");
        let m = &c.model;
        for clip in ["drive", "ride"] {
            for t in [0.0, 0.8, 1.6, 2.4] {
                // Hips held at the origin: the seat's hip point.
                let hips = joint_at(m, clip, "pelvis", t);
                assert!(hips.length() < 0.03, "{look} {clip}: hips at {hips}");
                // The top of the head is about 0.22 m over the head bone.
                let head = joint_at(m, clip, "head", t) + glam::Vec3::Y * 0.22;
                for model in [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover] {
                    for s in seats(model) {
                        let (x, z) = (s.hip.x + head.x as f64, s.hip.z + head.z as f64);
                        let roof = roof_over(model, x, z).expect("a head under the roof") as f32;
                        let room = roof - (s.hip.y as f32 + head.y);
                        let k = format!("{model:?} seat {:.2} {clip}", s.hip.z);
                        let e = least.entry(k).or_insert((f32::MAX, 0));
                        if room < e.0 {
                            *e = (room, look);
                        }
                    }
                }
                // Feet forward, over the floor (a sole is ~6 cm under the
                // ball joint).
                for foot in ["ball_l", "ball_r", "foot_l", "foot_r"] {
                    let f = joint_at(m, clip, foot, t);
                    for model in [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover] {
                        for s in seats(model) {
                            let floor = floor_under(model, s.hip.z + f.z as f64);
                            let over = s.hip.y + f.y as f64 - 0.06 - floor;
                            assert!(over > 0.0, "look {look} {clip}: {foot} {f} through the floor of {model:?} at {s:?} ({over} m)");
                        }
                    }
                }
            }
        }
    }
    for (k, (room, look)) in &least {
        eprintln!("{k}: least headroom {room:.3} m (look {look})");
    }
    for (k, (room, look)) in &least {
        // A little room for hair.
        assert!(
            *room > 0.015,
            "look {look}'s head touches the roof: {k} ({room} m)"
        );
    }
}

#[test]
fn people_who_ride_in_cars_keep_their_clothes_in_the_car() {
    // A long skirt hangs off the knees of someone sitting, through the
    // floor and out under the car: such looks walk, they don't ride.
    let Some(mut b) = builder() else { return };
    let mut fits = Vec::new();
    for look in 0..hw_sim::citizens::LOOKS {
        let c = people::build(&mut b, &Who::Citizen(look), 1, None).expect("citizen");
        assert_eq!(c.sits, hw_client::riderview::fits_seats(&c.model));
        if c.sits {
            fits.push(look);
        }
    }
    eprintln!("looks that ride: {fits:?}");
    assert!(
        fits.len() * 3 >= hw_sim::citizens::LOOKS as usize * 2,
        "too few looks fit in a car: {fits:?}"
    );
}
