//! Les petites touffes d'encolure suivent leur racine, sans se coucher avec
//! le cou quand le Nez noir broute. Elles restent des morceaux d'image.
//!
//! Les nappes de longue laine (la bavette de la Racka, la laine du Scottish
//! Blackface et de la Manech tête rousse) : des mèches entières, qui pendent
//! de leur attache au lieu de tourner avec le cou, se tassent sur l'herbe et
//! se balancent depuis leur bord du haut.

use mouflon_core::{Gesture, PoseSignals};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::bake::BakedRig;
use mouflon_render::kurbo::{Affine, Point, Shape, Vec2};
use mouflon_render::rig::{Item, Part, Rig};

fn blacknose() -> Rig {
    Rig::parse(include_bytes!("../../../pets/nez-noir/nez-noir.svg")).unwrap()
}

fn absolute(rig: &Rig, pose: &PoseTransforms, part: Part) -> Affine {
    let mut transform = pose.part(part);
    let mut parent = rig.parent(part);
    while let Some(part) = parent {
        transform = pose.part(part) * transform;
        parent = rig.parent(part);
    }
    pose.root * transform
}

#[test]
fn neck_locks_keep_their_roots_on_the_neck_and_hang_through_the_whole_grazing_pose() {
    let rig = blacknose();
    for facing in [-1.0, 1.0] {
        for step in 0..=100 {
            let signals = PoseSignals {
                graze: step as f32 / 100.0,
                ..PoseSignals::default()
            };
            let posed = pose(&rig, &signals, facing, 0.0);
            let neck = absolute(&rig, &posed, Part::Neck);
            for part in Part::NECK_SWAYS {
                assert!(rig.has_part(part));
                assert_eq!(rig.parent(part), Some(Part::Neck));
                let pivot = rig.pivot(part);
                let tuft = absolute(&rig, &posed, part);
                assert!((tuft * pivot - neck * pivot).hypot() < 1e-9);
                let down = tuft * (pivot + Vec2::new(0.0, 8.0)) - tuft * pivot;
                assert!(down.x.abs() < 1e-9, "{part:?}, phase {step}: {down:?}");
                assert!((down.y - 8.0).abs() < 1e-9);
                // À 90° de cou, la touffe ne s'aplatit pas, et au-delà
                // elle ne se retourne pas. Le miroir reste celui de l'animal.
                assert!((tuft.determinant().abs() - 1.0).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn neck_locks_still_sway_in_the_wind_and_during_a_shake_without_moving_their_roots() {
    let rig = blacknose();
    let rest = PoseSignals {
        graze: 0.6,
        ..PoseSignals::default()
    };
    let calm = pose(&rig, &rest, 1.0, 0.0);
    for signals in [
        PoseSignals {
            fleece: 0.25,
            fleece_tip: 0.15,
            ..rest
        },
        PoseSignals {
            gesture: Some(Gesture::Shake),
            gesture_amount: 1.0,
            gesture_time: 0.2,
            ..rest
        },
    ] {
        let posed = pose(&rig, &signals, 1.0, 0.0);
        for part in Part::NECK_SWAYS {
            let pivot = rig.pivot(part);
            let tuft = absolute(&rig, &posed, part);
            let neck = absolute(&rig, &posed, Part::Neck);
            assert!((tuft * pivot - neck * pivot).hypot() < 1e-9);
            let tip = pivot + Vec2::new(0.0, 8.0);
            let moving = tuft * tip - tuft * pivot;
            let steady = absolute(&rig, &calm, part) * tip - absolute(&rig, &calm, part) * pivot;
            assert!((moving - steady).hypot() > 0.01, "{part:?}");
        }
    }
}

#[test]
fn neck_locks_bake_in_a_few_pieces_and_leave_the_belly_locks_on_the_body() {
    let rig = blacknose();
    let baked = BakedRig::bake(&rig, 0.5, 0.6).expect("la toison se cuit");
    assert!(baked.len() <= 32, "{} morceaux", baked.len());
    for part in Part::SWAYS {
        assert!(rig.has_part(part));
        assert_eq!(part.contract_parent(true), Some(Part::Body));
    }
}

#[test]
fn a_neck_lock_placed_beside_the_neck_keeps_the_same_absolute_pose() {
    const LOCK: &str = r##"<g id="neck-sway-1"><path d="M77 43L82 43L80 58Z" fill="#ddd"/>
        <circle id="pivot-neck-sway-1" cx="79" cy="44" r="0.5"/></g>"##;
    let drawing = |lock_in_neck: bool| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 100">
            <g id="body"><ellipse cx="48" cy="55" rx="30" ry="23" fill="#eee"/></g>
            <g id="neck"><rect x="70" y="30" width="14" height="26" fill="#ccc"/>{}
                <circle id="pivot-neck" cx="74" cy="55" r="0.5"/></g>{}
            <g id="head"><ellipse cx="92" cy="35" rx="14" ry="9" fill="#888"/>
                <circle id="pivot-head" cx="80" cy="31" r="0.5"/>
                <circle id="pivot-muzzle" cx="105" cy="41" r="0.5"/></g>
            <circle id="pivot-ground" cx="50" cy="96" r="0.5"/></svg>"##,
            if lock_in_neck { LOCK } else { "" },
            if lock_in_neck { "" } else { LOCK },
        )
    };
    let nested = Rig::parse(drawing(true).as_bytes()).unwrap();
    let beside = Rig::parse(drawing(false).as_bytes()).unwrap();
    for step in 0..=10 {
        let signals = PoseSignals {
            graze: step as f32 / 10.0,
            fleece: 0.2,
            ..PoseSignals::default()
        };
        let one = pose(&nested, &signals, 1.0, 0.0);
        let two = pose(&beside, &signals, 1.0, 0.0);
        for point in [Point::new(79.0, 44.0), Point::new(80.0, 58.0)] {
            let a = absolute(&nested, &one, Part::NeckSway1) * point;
            let b = absolute(&beside, &two, Part::NeckSway1) * point;
            assert!((a - b).hypot() < 1e-8, "phase {step}: {a:?}, {b:?}");
        }
    }
}

/// Le point le plus bas des tracés d'une partie (pas de sa boîte, qui,
/// tournée, descend plus bas que la laine).
fn lowest_drawn_point(rig: &Rig, posed: &PoseTransforms, part: Part) -> f64 {
    let at = absolute(rig, posed, part);
    let mut inside = Vec::new();
    let mut lowest = f64::NEG_INFINITY;
    for item in &rig.items {
        match item {
            Item::BeginPart(p) => inside.push(*p),
            Item::EndPart => {
                inside.pop();
            }
            Item::Fill {
                path, transform, ..
            }
            | Item::Stroke {
                path, transform, ..
            } if inside.last() == Some(&part) => {
                let drawn = (at * *transform) * path.clone();
                lowest = lowest.max(drawn.bounding_box().max_y());
            }
            _ => {}
        }
    }
    lowest
}

#[test]
fn neck_locks_rest_on_the_ground_instead_of_hanging_through_it() {
    // Couché, endormi la tête au sol : l'encolure repose sur l'herbe, ses
    // touffes avec elle. Pendues à la verticale, elles passaient jusqu'à
    // 4,5 u sous le sol.
    let rig = blacknose();
    let u = rig.bounds.height() / 100.0;
    let lying = PoseSignals {
        sit: 1.0,
        ..PoseSignals::default()
    };
    let asleep = PoseSignals {
        sleep: 1.0,
        blink: 1.0,
        ..lying
    };
    let mut poses = vec![
        (
            "broute",
            PoseSignals {
                graze: 1.0,
                nibble: 1.0,
                ..PoseSignals::default()
            },
        ),
        ("couché", lying),
        ("endormi", asleep),
    ];
    for step in 1..=10 {
        poses.push((
            "la tête descend au sol",
            PoseSignals {
                sleep_low: step as f32 / 10.0,
                ..asleep
            },
        ));
    }
    for facing in [-1.0, 1.0] {
        for (name, signals) in &poses {
            let posed = pose(&rig, signals, facing, 0.0);
            let ground = (posed.root * rig.ground).y;
            let neck = absolute(&rig, &posed, Part::Neck);
            for part in Part::NECK_SWAYS {
                let below = (lowest_drawn_point(&rig, &posed, part) - ground) / u;
                assert!(
                    below <= 0.25,
                    "{name} ({}), {part:?}, sens {facing} : {below:.2} u sous le sol",
                    signals.sleep_low
                );
                // La racine reste sur le cou.
                let pivot = rig.pivot(part);
                let tuft = absolute(&rig, &posed, part);
                assert!((tuft * pivot - neck * pivot).hypot() < 1e-9);
            }
        }
    }
}

/// Les dessins (bélier, brebis, agneau) des races à longue laine d'encolure.
fn sheeted() -> Vec<(String, Rig)> {
    let pets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut out = Vec::new();
    for breed in ["racka", "scottish-blackface", "manech-tete-rousse"] {
        for suffix in ["", "-belier", "-brebis", "-agneau"] {
            let name = format!("{breed}{suffix}");
            let Ok(svg) = std::fs::read(pets.join(breed).join(format!("{name}.svg"))) else {
                continue;
            };
            out.push((name, Rig::parse(&svg).unwrap()));
        }
    }
    assert_eq!(out.len(), 9, "trois dessins par race");
    out
}

#[test]
fn long_neck_wool_hangs_in_sheets_instead_of_turning_with_the_neck() {
    // Il baisse la tête pour brouter : l'encolure tourne de près d'un quart
    // de tour, sa longue laine reste à la verticale. L'attache de chaque
    // nappe suit le cou ; celle du poitrail, à la base de l'encolure, ne
    // bouge pas plus que le corps.
    for (name, rig) in sheeted() {
        let u = rig.bounds.height() / 100.0;
        let standing = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
        let mut turned: f64 = 0.0;
        for step in 0..=20 {
            let signals = PoseSignals {
                graze: step as f32 / 20.0,
                ..PoseSignals::default()
            };
            let posed = pose(&rig, &signals, 1.0, 0.0);
            let neck = absolute(&rig, &posed, Part::Neck);
            let [a, b, ..] = neck.as_coeffs();
            turned = turned.max(b.atan2(a).abs());
            for part in Part::NECK_SWAYS {
                assert!(rig.has_part(part), "{name} : {part:?}");
                let pivot = rig.pivot(part);
                let sheet = absolute(&rig, &posed, part);
                assert!((sheet * pivot - neck * pivot).hypot() < 1e-9);
                // Ses mèches pendent : ni tournées, ni penchées.
                let [a, b, c, d, ..] = sheet.as_coeffs();
                assert!((a - 1.0).abs() < 1e-9 && b.abs() < 1e-9 && c.abs() < 1e-9);
                // Tout au plus tassées sur l'herbe, jamais étirées.
                assert!(d > 0.1 && d <= 1.0 + 1e-9, "{name} {part:?} : {d}");
            }
            // Le poitrail : attaché à la base de l'encolure.
            let chest = Part::NeckSway1;
            let moved = absolute(&rig, &posed, chest) * rig.pivot(chest)
                - absolute(&rig, &standing, chest) * rig.pivot(chest);
            // (Le corps s'incline un peu vers l'avant quand il broute.)
            assert!(
                moved.hypot() < 3.0 * u,
                "{name} : le poitrail bouge de {moved:?}"
            );
        }
        assert!(
            turned > 1.0,
            "{name} : l'encolure n'a tourné que de {turned}"
        );
    }
}

#[test]
fn a_sheet_piles_up_on_the_grass_instead_of_hanging_through_it() {
    // En broutant comme en dormant la tête au sol, l'attache descend près
    // de l'herbe : la nappe s'y tasse.
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        blink: 1.0,
        ..PoseSignals::default()
    };
    let mut poses = Vec::new();
    for step in 0..=10 {
        let k = step as f32 / 10.0;
        poses.push(PoseSignals {
            graze: k,
            nibble: k,
            ..PoseSignals::default()
        });
        poses.push(PoseSignals {
            sleep_low: k,
            ..asleep
        });
    }
    let mut piled = 0;
    for (name, rig) in sheeted() {
        let u = rig.bounds.height() / 100.0;
        for facing in [-1.0, 1.0] {
            for signals in &poses {
                let posed = pose(&rig, signals, facing, 0.0);
                let ground = (posed.root * rig.ground).y;
                for part in Part::NECK_SWAYS {
                    let below = (lowest_drawn_point(&rig, &posed, part) - ground) / u;
                    assert!(
                        below <= 0.3,
                        "{name} {part:?}, broute {}, dort {} : {below:.2} u sous le sol",
                        signals.graze,
                        signals.sleep_low
                    );
                    let [.., d, _, _] = absolute(&rig, &posed, part).as_coeffs();
                    if d < 0.99 {
                        piled += 1;
                    }
                }
            }
        }
    }
    assert!(piled > 0, "aucune nappe n'a touché l'herbe");
}

#[test]
fn a_sheet_sways_from_its_top_edge_and_a_cut_lock_from_its_cut() {
    // Une nappe de mèches entières est attachée par son milieu : dans le
    // vent, son bord du haut reste en place et tout ce qui pend dessous
    // penche du même côté. Une mèche coupée ne quitte pas sa ligne de coupe.
    let windy = PoseSignals {
        fleece: 0.25,
        fleece_tip: 0.15,
        ..PoseSignals::default()
    };
    for (name, rig) in sheeted() {
        let calm = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
        let blown = pose(&rig, &windy, 1.0, 0.0);
        for part in Part::NECK_SWAYS {
            let pivot = rig.pivot(part);
            let (still, moving) = (absolute(&rig, &calm, part), absolute(&rig, &blown, part));
            let shift = |p: Point| (moving * p).x - (still * p).x;
            // Le point le plus haut de ce qu'elle dessine, et un point bas.
            let top = lowest_drawn_point(&rig, &calm, part);
            let own = rig.own_bounds(part).expect("la nappe dessine");
            let at_top = shift(Point::new(pivot.x, own.min_y()));
            let below = shift(Point::new(pivot.x, top));
            assert!(below.abs() > 0.3, "{name} {part:?} : le bas ne bouge pas");
            // Rien ne part de l'autre côté ; le haut bouge au plus d'un rien.
            assert!(
                at_top.abs() < 0.01 || at_top * below > 0.0,
                "{name} {part:?} : {at_top} et {below}"
            );
            assert!(at_top.abs() < 0.25 * below.abs(), "{name} {part:?}");
        }
    }
    let rig = blacknose();
    let blown = pose(&rig, &windy, 1.0, 0.0);
    for part in Part::NECK_SWAYS {
        let pivot = rig.pivot(part);
        let neck = absolute(&rig, &blown, Part::Neck);
        assert!((absolute(&rig, &blown, part) * pivot - neck * pivot).hypot() < 1e-9);
    }
}
