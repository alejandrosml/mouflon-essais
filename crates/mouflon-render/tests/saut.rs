//! Le saut sur un rebord plus haut (voir `docs/animation-animale.md`,
//! « Sauter sur un rebord plus haut »), avec le vrai moteur et le vrai
//! dessin, pour toutes les races du dépôt aux pattes en deux segments : il
//! monte sur la fenêtre plus haute, ses pattes sont repliées en l'air, et
//! ses sabots ne glissent pas en se posant.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::def::Motion;
use mouflon_core::{
    Flock, Input, LedgeOptions, PetDef, PetView, Rect, Screen, Support, TICK, WindowRect, World,
    load_pet,
};
use mouflon_render::animate::pose;
use mouflon_render::kurbo::{Affine, Point, Vec2};
use mouflon_render::rig::Part;
use mouflon_render::{Look, PetArt};

/// De combien un sabot posé peut s'éloigner de l'endroit où il s'est posé
/// (points), comme dans `glissement.rs`.
const SLIDE: f64 = 0.5;
/// Posé : à moins d'un vingtième de point de son plus bas.
const PLANTED: f64 = 0.05;
/// Au sommet du bond, ses sabots sont repliés au moins de tant (en part de
/// la longueur de ses pattes) au-dessus de son point d'appui.
const TUCKED: f64 = 0.2;

/// Chaque dessin d'un animal, seul : celui de base, puis ceux de son sexe
/// et de son âge (`[appearance.female]`…).
fn drawings(name: &str, def: &PetDef) -> Vec<(String, PetDef)> {
    let mut base = def.clone();
    base.set_variants(Vec::new()).unwrap();
    let mut all = vec![(name.to_owned(), base)];
    for (variant, drawn) in def.variants() {
        all.push((format!("{name} ({})", variant.name()), (**drawn).clone()));
    }
    all
}

/// Les races du dépôt dont les pattes ont deux segments.
fn breeds() -> Vec<(String, Arc<PetDef>, PetArt)> {
    let pets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut names: Vec<_> = std::fs::read_dir(&pets)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
        .into_iter()
        .flat_map(|name| {
            let def = load_pet(&pets.join(&name).join("pet.toml"))
                .unwrap_or_else(|e| panic!("{name} : {e:?}"));
            drawings(&name, &def)
        })
        .filter_map(|(name, def)| {
            let def = Arc::new(def);
            let art = PetArt::load(&def).unwrap_or_else(|e| panic!("{name} : {e:?}"));
            let jointed = match &art.look {
                Look::Rig(rig) => Part::LEGS
                    .iter()
                    .any(|leg| leg.lower().is_some_and(|lower| rig.has_part(lower))),
                Look::Skeleton(sk) => sk.hooves().iter().any(Option::is_some),
                Look::Sprites(_) => false,
            };
            jointed.then_some((name, def, art))
        })
        .collect()
}

/// Où sont les quatre sabots à l'écran (dans l'ordre de [`Part::LEGS`]).
fn hooves(art: &PetArt, view: &PetView<'_>) -> [Option<Point>; 4] {
    let at = |ground: Point, k: f64| {
        Affine::translate(Vec2::new(
            f64::from(view.ground.x),
            f64::from(view.ground.y),
        )) * Affine::scale(k)
            * Affine::translate(-ground.to_vec2())
    };
    match &art.look {
        Look::Rig(rig) => {
            let k = f64::from(art.height) / rig.bounds.height();
            let p = pose(rig, &view.pose, view.facing, f64::from(art.stride) / k);
            let base = at(rig.ground, k) * p.root;
            Part::LEGS.map(|leg| {
                let lower = leg.lower()?;
                rig.has_part(lower).then(|| {
                    let b = rig.part_bounds(lower);
                    base * p.part(leg) * p.part(lower) * Point::new(b.center().x, b.max_y())
                })
            })
        }
        Look::Skeleton(sk) => {
            let k = f64::from(art.height) / sk.bounds.height().max(1.0);
            let p = sk.pose(&view.pose, view.facing, f64::from(art.stride) / k);
            let base = at(sk.ground(), k) * p.root;
            sk.hooves()
                .map(|hoof| hoof.map(|(lower, sole)| base * p.bones[lower] * sole))
        }
        Look::Sprites(_) => [None; 4],
    }
}

/// Une image : les sabots, le point d'appui, en l'air ou non, et ce qu'il
/// fait.
struct Frame {
    feet: [Option<Point>; 4],
    ground: Point,
    air: bool,
    behavior: String,
}

/// La plus grande portée de ses sauts sur un rebord, en points.
fn reach(def: &PetDef) -> f32 {
    def.behaviors
        .iter()
        .filter_map(|b| match b.motion {
            Motion::Leap { reach } => Some(reach),
            _ => None,
        })
        .fold(0.0, f32::max)
        * def.stride()
        / 1.6
}

/// Deux fenêtres côte à côte, la seconde `rise` points plus haut (derrière
/// la première) ; il marche sur la première vers la seconde et y saute.
/// Renvoie les images, depuis qu'il est au pied de la fenêtre, et s'il a
/// fini sur la seconde.
fn leap(def: &Arc<PetDef>, art: &PetArt, rise: f32) -> (Vec<Frame>, bool) {
    let corner = 600.0;
    let low = Rect::new(200.0, 500.0, 400.0 + 20.0, 250.0);
    let high = Rect::new(corner, 500.0 - rise, 400.0, 250.0 + rise);
    let mut world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }]);
    world.set_windows(
        &[
            WindowRect { id: 1, frame: low },
            WindowRect { id: 2, frame: high },
        ],
        LedgeOptions::default(),
    );
    let mut flock = Flock::new();
    flock.set_realistic(true);
    let id = flock.spawn(Arc::clone(def), 5, &world, Some(330.0));
    let input = Input::default();
    for _ in 0..(1.5 / TICK) as usize {
        flock.advance(TICK, &world, &input);
    }
    let mut frames = Vec::new();
    let (mut started, mut on_high) = (false, false);
    // Douze secondes, et au moins six après son départ : un agneau, qui
    // marche à sa taille, met plus longtemps à arriver au pied de la fenêtre.
    let (mut step, mut end) = (0, (12.0 / TICK) as usize);
    while step < end {
        step += 1;
        let pet = flock.get_mut(id).unwrap();
        if !started && step == end {
            end += (1.0 / TICK) as usize;
            if end > (60.0 / TICK) as usize {
                break;
            }
        }
        if !started {
            // Il marche jusqu'au pied de la fenêtre, puis il saute.
            pet.face(1.0);
            if pet.position().x + def.physics.width * 0.5 >= corner - 3.0 {
                assert!(pet.force("leap"));
                started = true;
                end = end.max(step + (6.0 / TICK) as usize);
            } else if pet.behavior_name() != "walk" {
                pet.force("walk");
            }
        }
        flock.advance(TICK, &world, &input);
        if started {
            let pet = flock.pets().iter().find(|p| p.id() == id).unwrap();
            // Monté : on le regarde encore se recevoir et repartir, pas plus
            // (un agneau joueur redescend vite en courant).
            if !on_high && matches!(pet.support(), Support::Ground { surface } if surface >> 8 == 2)
            {
                on_high = true;
                end = end.min(step + (2.5 / TICK) as usize);
            }
            let view = pet.view(1.0);
            frames.push(Frame {
                feet: hooves(art, &view),
                ground: Point::new(f64::from(view.ground.x), f64::from(view.ground.y)),
                air: view.support == Support::Air,
                behavior: view.behavior.to_owned(),
            });
        }
    }
    (frames, on_high)
}

/// Le pire glissement d'un sabot posé, après la réception (points), et le
/// nombre d'images où il est posé.
fn slide_after_landing(frames: &[Frame], leg: usize) -> (f64, usize) {
    let lowest = frames
        .iter()
        .filter_map(|f| f.feet[leg])
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let (mut worst, mut put, mut count) = (0.0f64, None::<f64>, 0);
    for pair in frames.windows(2) {
        let planted = |f: &Frame| f.feet[leg].is_some_and(|p| p.y > lowest - PLANTED);
        if !planted(&pair[0]) || !planted(&pair[1]) {
            put = None;
            continue;
        }
        count += 1;
        let (from, to) = (pair[0].feet[leg].unwrap().x, pair[1].feet[leg].unwrap().x);
        worst = worst.max((to - *put.get_or_insert(from)).abs());
    }
    (worst, count)
}

#[test]
fn every_breed_leaps_onto_a_higher_window_legs_folded_and_lands_without_sliding() {
    let breeds = breeds();
    assert!(breeds.len() >= 20, "{}", breeds.len());
    let mut failures = Vec::new();
    for (name, def, art) in &breeds {
        let rise = 0.8 * reach(def);
        let (frames, on_high) = leap(def, art, rise);
        if !on_high {
            failures.push(format!("{name} : pas monté sur la fenêtre ({rise:.0} pt)"));
            continue;
        }
        // Au sommet du bond, les pattes repliées.
        let flight: Vec<&Frame> = frames.iter().filter(|f| f.air).collect();
        let Some(top) = flight
            .iter()
            .min_by(|a, b| a.ground.y.total_cmp(&b.ground.y))
        else {
            failures.push(format!("{name} : pas de vol"));
            continue;
        };
        let leg = f64::from(def.stride()) / 1.6;
        let lifts: Vec<f64> = top
            .feet
            .iter()
            .flatten()
            .map(|p| (top.ground.y - p.y) / leg)
            .collect();
        let tucked = lifts.iter().sum::<f64>() / lifts.len().max(1) as f64;
        if tucked < TUCKED {
            failures.push(format!(
                "{name} : pattes peu repliées au sommet ({tucked:.2})"
            ));
        }
        // Posé, il se reçoit sur place : ses sabots ne bougent pas, ni en
        // avant ni en l'air, tant qu'il se reçoit (`leap_land`).
        let landed = frames.iter().rposition(|f| f.air).map_or(0, |i| i + 1);
        let reception: Vec<&Frame> = frames[landed..]
            .iter()
            .take_while(|f| f.behavior == "leap_land")
            .collect();
        // Un agneau, plus vif, se reçoit plus vite (0,18 s au lieu de 0,3 s).
        if reception.len() < (0.15 / TICK) as usize {
            failures.push(format!(
                "{name} : réception trop courte ({})",
                reception.len()
            ));
            continue;
        }
        for (k, part) in Part::LEGS.iter().enumerate() {
            let Some(first) = reception[0].feet[k] else {
                continue;
            };
            let moved = reception
                .iter()
                .filter_map(|f| f.feet[k])
                .map(|p| (p.x - first.x).abs().max((p.y - first.y).abs()))
                .fold(0.0f64, f64::max);
            if moved > SLIDE {
                failures.push(format!("{name} {part:?} : {moved:.2} pt en se recevant"));
            }
        }
        // Puis ses premiers pas sur le rebord, sans glisser non plus.
        let after = &frames[landed..(landed + (1.5 / TICK) as usize).min(frames.len())];
        for (k, part) in Part::LEGS.iter().enumerate() {
            let (worst, _) = slide_after_landing(after, k);
            if worst > SLIDE {
                failures.push(format!("{name} {part:?} : {worst:.2} pt en repartant"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
