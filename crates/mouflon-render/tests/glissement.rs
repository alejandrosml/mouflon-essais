//! Les sabots posés ne glissent pas : toutes les races du dépôt aux pattes
//! en deux segments, avec le vrai moteur, quand elles marchent, trottent,
//! galopent, s'arrêtent, font demi-tour et repartent, et quand elles
//! broutent ; arrêtées, elles se remettent d'aplomb.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::def::Duration;
use mouflon_core::{Input, LegCycle, Pet, PetDef, PetView, Rect, Screen, TICK, World, load_pet};
use mouflon_render::animate::pose;
use mouflon_render::kurbo::{Affine, Point, Vec2};
use mouflon_render::rig::Part;
use mouflon_render::{Look, PetArt};

/// De combien un sabot posé peut s'éloigner de l'endroit où il s'est posé,
/// sur tout un appui, en points : un demi-point, moins d'un pixel d'écran
/// Retina (un appui couvre 20 à 25 points de chemin, au pas comme au galop ;
/// à l'arrêt, le corps n'avance plus, et c'est le sabot qui ne doit pas
/// bouger). Avant que le moteur ne pose lui-même les sabots, ils glissaient
/// de 8 à 15 points en s'arrêtant ou en repartant (jusqu'à leur place au
/// repos, ou emportés par le corps qui partait), de 10 à 18 en broutant.
const SLIDE: f64 = 0.5;

/// Posé : à moins d'un vingtième de point de son plus bas.
const PLANTED: f64 = 0.05;

/// Arrêté, une patte est à sa place à moins de ceci près (en foulées du
/// pas ; le moteur laisse 0,05 : un sabot si près de sa place y reste).
const SETTLED: f32 = 0.06;

/// Il naît en haut de l'écran : sa chute et son atterrissage (qui ne sont
/// pas l'objet de ce test) ne comptent pas (secondes).
const LANDED: f32 = 1.5;

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
        .filter_map(|(name, mut def)| {
            // Chaque comportement du programme dure le temps qu'on lui donne.
            for behavior in &mut def.behaviors {
                if matches!(behavior.duration, Duration::Range(..)) {
                    behavior.duration = Duration::Range(600.0, 600.0);
                }
            }
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

/// Une image : les sabots, l'abscisse du corps, le sens où il regarde, ce
/// que le moteur dit de ses pattes, et s'il est à la fin d'un arrêt.
struct Frame {
    feet: [Option<Point>; 4],
    body: f64,
    facing: f32,
    legs: [LegCycle; 4],
    settled: bool,
}

/// Le programme : chaque comportement, forcé pendant tant de secondes (repris
/// si le moteur en change) ; toutes les images, une fois l'animal à terre.
fn run(def: &Arc<PetDef>, art: &PetArt, script: &[(&str, f32)]) -> Vec<Frame> {
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 40000.0, 600.0),
        usable: Rect::new(0.0, 0.0, 40000.0, 560.0),
        scale: 2.0,
    }]);
    let input = Input::default();
    let mut pet = Pet::spawn(0, Arc::clone(def), 3, &world, Some(20000.0));
    let mut frames = Vec::new();
    let mut t = 0.0;
    for &(behavior, seconds) in script {
        let ticks = (seconds / TICK) as usize;
        for i in 0..ticks {
            if pet.behavior_name() != behavior {
                assert!(pet.force(behavior), "{behavior}");
            }
            pet.step(TICK, &world, &input);
            t += TICK;
            if t > LANDED {
                let view = pet.view(1.0);
                frames.push(Frame {
                    feet: hooves(art, &view),
                    body: f64::from(view.ground.x),
                    facing: view.facing,
                    legs: view.pose.legs,
                    settled: behavior == "stand" && i + 1 == ticks,
                });
            }
        }
    }
    frames
}

/// Le pire glissement d'un sabot sur un appui : de combien il s'éloigne de
/// l'endroit où il s'est posé (points) ; et le nombre d'appuis. Posé : à
/// moins d'un vingtième de point de son plus bas (chaque dessin a ses sabots
/// à sa hauteur : ceux du Lacaune descendent un tiers de point sous la ligne
/// du sol). Seules les images d'un demi-tour ne comptent pas : le dessin s'y
/// retourne en se resserrant, et tout bouge.
fn stances(frames: &[Frame], leg: usize) -> (f64, usize) {
    let turning = |f: &Frame| f.facing.abs() < 0.99;
    let lowest = frames
        .iter()
        .filter(|f| !turning(f))
        .filter_map(|f| f.feet[leg])
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let (mut worst, mut count, mut put) = (0.0f64, 0, None::<f64>);
    for pair in frames.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let planted = |f: &Frame| f.feet[leg].is_some_and(|p| p.y > lowest - PLANTED);
        if turning(a) || turning(b) || a.facing * b.facing < 0.0 || !planted(a) || !planted(b) {
            if put.take().is_some() {
                count += 1;
            }
            continue;
        }
        let (from, to) = (a.feet[leg].unwrap().x, b.feet[leg].unwrap().x);
        worst = worst.max((to - *put.get_or_insert(from)).abs());
    }
    (worst, count + usize::from(put.is_some()))
}

/// À la fin de chaque arrêt, les quatre pattes sont posées à leur place.
fn unsettled(frames: &[Frame]) -> Option<String> {
    frames.iter().filter(|f| f.settled).find_map(|f| {
        let far: Vec<String> = f
            .legs
            .iter()
            .filter(|l| l.raise > 0.0 || l.x.abs() > SETTLED)
            .map(|l| format!("{:+.3}", l.x))
            .collect();
        (!far.is_empty()).then(|| far.join(" "))
    })
}

#[test]
fn every_breed_keeps_its_hooves_planted_when_it_moves_stops_turns_and_starts_again() {
    let breeds = breeds();
    // Les dessins vectoriels, et la Solognote peinte sur un squelette.
    assert!(
        breeds
            .iter()
            .any(|(_, _, art)| matches!(art.look, Look::Rig(_)))
    );
    assert!(
        breeds
            .iter()
            .any(|(_, _, art)| matches!(art.look, Look::Skeleton(_)))
    );
    let mut failures = Vec::new();
    for (name, def, art) in &breeds {
        for gait in ["walk", "trot", "run"] {
            // Il part, va, s'arrête et se remet d'aplomb, repart, fait
            // demi-tour, repart, s'arrête.
            let script = [
                ("stand", 2.0),
                (gait, 2.5),
                ("stand", 2.0),
                (gait, 1.5),
                ("turn", 1.5),
                (gait, 1.5),
                ("stand", 2.0),
            ];
            let frames = run(def, art, &script);
            if !frames.iter().any(|f| f.facing < -0.99) || !frames.iter().any(|f| f.facing > 0.99) {
                failures.push(format!("{name} {gait} : pas de demi-tour"));
            }
            for (leg, part) in Part::LEGS.iter().enumerate() {
                if frames.iter().all(|f| f.feet[leg].is_none()) {
                    continue;
                }
                let (worst, count) = stances(&frames, leg);
                if count < 4 || worst > SLIDE {
                    failures.push(format!(
                        "{name} {gait} {part:?} : {worst:.2} pt sur un appui ({count} appuis)"
                    ));
                }
            }
            if let Some(far) = unsettled(&frames) {
                failures.push(format!("{name} {gait} : pas d'aplomb à l'arrêt ({far})"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_grazing_sheep_steps_to_the_next_tuft_and_settles_without_sliding() {
    // Deux stations d'herbe et leurs pas (toutes les 8,5 s, un pas d'une
    // seconde), et trois secondes après le dernier.
    let mut failures = Vec::new();
    for (name, def, art) in &breeds() {
        let frames = run(def, art, &[("stand", 2.0), ("graze", 20.0)]);
        let moved = frames
            .first()
            .zip(frames.last())
            .map(|(a, b)| (b.body - a.body).abs());
        if moved.is_none_or(|m| m < 1.0) {
            failures.push(format!("{name} : il n'a pas bougé en broutant"));
        }
        for (leg, part) in Part::LEGS.iter().enumerate() {
            if frames.iter().all(|f| f.feet[leg].is_none()) {
                continue;
            }
            let (worst, _) = stances(&frames, leg);
            if worst > SLIDE {
                failures.push(format!("{name} {part:?} : {worst:.2} pt sur un appui"));
            }
        }
        // Après son pas, il s'est remis d'aplomb : ses pattes sont à leur
        // place.
        let last = frames.last().map(|f| f.legs).unwrap_or_default();
        if last.iter().any(|l| l.raise > 0.0 || l.x.abs() > SETTLED) {
            failures.push(format!("{name} : pas d'aplomb en broutant"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
