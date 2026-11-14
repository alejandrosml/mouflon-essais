//! La tétée vise le repère sous le flanc, tête relevée, sans déplacer les
//! appuis. Le même contrat vaut pour un rig et un squelette peint.

use std::path::Path;

use mouflon_core::{BoneRole, Gesture, PoseSignals, Vec2, load_pet};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{Affine, Point};
use mouflon_render::{Look, Part, PetArt, Rig};

fn art(name: &str) -> PetArt {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../pets/{name}/pet.toml"));
    PetArt::load(&load_pet(&path).unwrap()).unwrap()
}

fn absolute(rig: &Rig, posed: &PoseTransforms, part: Part) -> Affine {
    let mut t = posed.part(part);
    let mut up = rig.parent(part);
    while let Some(parent) = up {
        t = posed.part(parent) * t;
        up = rig.parent(parent);
    }
    posed.root * t
}

fn nurse(target: Vec2, time: f32) -> PoseSignals {
    PoseSignals {
        gesture: Some(Gesture::Nurse),
        gesture_amount: 1.0,
        gesture_time: time,
        nursing_target: Some(target),
        ..PoseSignals::default()
    }
}

#[test]
fn a_nursing_rig_reaches_beneath_the_flank_with_its_muzzle_pointing_up() {
    for name in ["bleu-du-maine", "nez-noir", "mouflon-corse"] {
        let art = art(name);
        let (udder, natural) = art.nursing_points().expect("repères de tétée");
        let Look::Rig(rig) = &art.look else {
            panic!("rig");
        };
        let h = rig.bounds.height();
        let muzzle = rig.muzzle.expect("museau explicite");
        for young_scale in [0.55, 0.70] {
            let target = Vec2::new(natural.x, udder.y / young_scale);
            for facing in [-1.0, 1.0] {
                let signals = nurse(target, 2.0);
                let posed = pose(rig, &signals, facing, 0.0);
                let head = absolute(rig, &posed, Part::Head);
                let actual = head * muzzle;
                let expected = rig.ground
                    + mouflon_render::kurbo::Vec2::new(
                        f64::from(target.x * facing) * h,
                        f64::from(target.y) * h,
                    );
                assert!(
                    (actual - expected).hypot() < 0.002 * h,
                    "{name} {facing}: museau {actual:?}, cible {expected:?}, repères {udder:?} {natural:?}"
                );
                let poll = head * rig.pivot(Part::Head);
                assert!(
                    actual.y < poll.y - 0.15 * (actual - poll).hypot(),
                    "{name}: le museau se lève vers la mamelle : {actual:?}, nuque {poll:?}"
                );
                let [a, b, ..] = head.as_coeffs();
                let turn = b.atan2(a * f64::from(facing));
                assert!(
                    turn.abs() < std::f64::consts::FRAC_PI_2,
                    "{name} : la tête se relève sans se retourner ({turn} rad)"
                );
            }
        }
    }
}

#[test]
fn nursing_wags_the_tail_without_sliding_the_four_hooves() {
    let art = art("bleu-du-maine");
    let (udder, muzzle) = art.nursing_points().unwrap();
    let Look::Rig(rig) = &art.look else {
        panic!("rig");
    };
    let target = Vec2::new(muzzle.x, udder.y / 0.58);
    let resting = pose(rig, &PoseSignals::default(), 1.0, 0.0);
    let mut tails = Vec::new();
    for i in 0..24 {
        let mut signals = nurse(target, 1.0 + i as f32 / 60.0);
        signals.breath = (i as f32 * 0.2).sin();
        let posed = pose(rig, &signals, 1.0, 0.0);
        tails.push(posed.part(Part::Tail));
        for leg in Part::LEGS {
            let lower = leg.lower().unwrap();
            let bounds = rig.part_bounds(lower);
            let hoof = Point::new(bounds.center().x, bounds.max_y());
            let rest = absolute(rig, &resting, lower) * hoof;
            let at = absolute(rig, &posed, lower) * hoof;
            assert!(
                (at - rest).hypot() < 0.03,
                "{leg:?}: {at:?} au lieu de {rest:?}"
            );
        }
    }
    assert!(
        tails.windows(2).any(|pair| pair[0] != pair[1]),
        "la queue frétille"
    );
}

#[test]
fn a_painted_lamb_also_reaches_the_maternal_target() {
    let art = art("solognote");
    let (udder, natural) = art.nursing_points().unwrap();
    let Look::Skeleton(art) = &art.look else {
        panic!("squelette");
    };
    let head_index = art
        .skeleton
        .bones
        .iter()
        .position(|b| b.role == Some(BoneRole::Head))
        .unwrap();
    let head_at = art.skeleton.bones[head_index].at;
    let muzzle = art.skeleton.muzzle.unwrap();
    let h = art.bounds.height();
    for young_scale in [0.55, 0.70] {
        let target = Vec2::new(natural.x, udder.y / young_scale);
        for facing in [-1.0, 1.0] {
            let posed = art.pose(&nurse(target, 2.0), facing, 0.0);
            let head = posed.root * posed.bones[head_index];
            let actual = head * Point::new(f64::from(muzzle.x), f64::from(muzzle.y));
            let expected = art.ground()
                + mouflon_render::kurbo::Vec2::new(
                    f64::from(target.x * facing) * h,
                    f64::from(target.y) * h,
                );
            assert!(
                (actual - expected).hypot() < 0.002 * h,
                "{actual:?} {expected:?}"
            );
            let poll = head * Point::new(f64::from(head_at.x), f64::from(head_at.y));
            assert!(
                actual.y < poll.y - 0.15 * (actual - poll).hypot(),
                "museau relevé : {actual:?}, nuque {poll:?}"
            );
            let [a, b, ..] = head.as_coeffs();
            let turn = b.atan2(a * f64::from(facing));
            assert!(turn.abs() < std::f64::consts::FRAC_PI_2);
        }
    }
}

#[test]
fn nursing_fades_back_to_rest_without_a_muzzle_jump() {
    for name in ["bleu-du-maine", "nez-noir", "solognote"] {
        let art = art(name);
        let (udder, reach) = art.nursing_points().unwrap();
        let target = Vec2::new(reach.x, udder.y / 0.58);
        let height = match &art.look {
            Look::Rig(rig) => rig.bounds.height(),
            Look::Skeleton(skeleton) => skeleton.bounds.height(),
            Look::Sprites(_) => unreachable!(),
        };
        for facing in [-1.0, 1.0] {
            let muzzle_at = |signals: &PoseSignals| match &art.look {
                Look::Rig(rig) => {
                    let posed = pose(rig, signals, facing, 0.0);
                    absolute(rig, &posed, Part::Head) * rig.muzzle.unwrap()
                }
                Look::Skeleton(art) => {
                    let head = art
                        .skeleton
                        .bones
                        .iter()
                        .position(|bone| bone.role == Some(BoneRole::Head))
                        .unwrap();
                    let point = art.skeleton.muzzle.unwrap();
                    let posed = art.pose(signals, facing, 0.0);
                    posed.root
                        * posed.bones[head]
                        * Point::new(f64::from(point.x), f64::from(point.y))
                }
                Look::Sprites(_) => unreachable!(),
            };
            let mut signals = nurse(target, 2.0);
            let start = muzzle_at(&signals);
            let mut previous = start;
            // Le cœur garde la cible pendant la sortie ; chaque centième
            // d'amplitude rend progressivement la pose au repos.
            for step in 1..=100 {
                signals.gesture_amount = 1.0 - step as f32 / 100.0;
                let current = muzzle_at(&signals);
                assert!(
                    (current - previous).hypot() < 0.04 * height,
                    "{name}, sens {facing}, sortie {step} : {previous:?} → {current:?}"
                );
                previous = current;
            }
            let rest = muzzle_at(&PoseSignals::default());
            assert!((previous - rest).hypot() < 1e-6 * height, "{name}");
            assert!((start - rest).hypot() > 0.01 * height, "{name}");
        }
    }
}
