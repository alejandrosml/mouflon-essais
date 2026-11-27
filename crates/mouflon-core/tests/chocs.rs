//! Les chocs entre animaux, en mode collisions : un animal dessiné d'après
//! nature encaisse sur ses pattes, comme un vrai mouton. Il ne part jamais à
//! la renverse : celui qui fonce s'arrête contre l'autre, celui qui est
//! percuté cède d'un pas, deux béliers lancés front contre front s'arrêtent
//! net, le plus lourd emporte le plus léger, et personne ne quitte son
//! rebord. Au pas, ils se saluent au lieu de se heurter. Seules les mascottes
//! numériques (les animaux desktopPet) culbutent encore.
//!
//! Avant, tout contact, même au pas, envoyait les deux animaux en l'air pour
//! un tour complet.

use std::sync::Arc;

use mouflon_core::{
    Flock, Input, LedgeOptions, Mood, PetDef, PetEvent, Rect, Screen, Support, SurfaceKind, Vec2,
    WindowRect, World, load_pet, parse_pet_toml,
};

const RATE: f32 = 60.0;

fn bundled(name: &str) -> Arc<PetDef> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../pets/{name}/pet.toml"));
    Arc::new(load_pet(&path).unwrap())
}

/// Pelote, un animal desktopPet des données de test : une mascotte numérique.
fn mascot() -> Arc<PetDef> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/donnees/pelote/pet.toml");
    Arc::new(load_pet(&path).unwrap())
}

/// Un animal d'essai de la taille voulue, qui ne fait que ce qu'on lui dit :
/// debout sans fin, ou lancé à 160 points par seconde.
fn sized(id: &str, width: f32, height: f32) -> Arc<PetDef> {
    sized_with(id, width, height, "")
}

/// Le même, qui part à la renverse quand on le heurte, comme une mascotte
/// numérique : son réflexe `bump` mène à une chute.
fn tumbler(id: &str, width: f32, height: f32) -> Arc<PetDef> {
    sized_with(id, width, height, "bump = \"fall\"")
}

/// (`reflex` : une ligne de plus dans ses réflexes.)
fn sized_with(id: &str, width: f32, height: f32, reflex: &str) -> Arc<PetDef> {
    let text = format!(
        r#"
        [pet]
        id = "{id}"
        name = "{id}"
        [appearance]
        rig = "a.svg"
        height = {height}
        [physics]
        width = {width}
        stride = {stride}
        bounce = 0.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        thrown = "fall"
        held = "held"
        {reflex}
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = "forever"
        [[behavior]]
        name = "charge"
        motion = "charge"
        speed = 160.0
        duration = "forever"
        [[behavior]]
        name = "held"
        motion = "held"
        duration = "forever"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
        "#,
        stride = height * 0.6,
    );
    Arc::new(parse_pet_toml(&text, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 835.0),
        scale: 1.0,
    }])
}

fn input() -> Input {
    Input {
        local_hour: 15.0,
        ..Input::default()
    }
}

/// Ce qu'on a vu d'un animal pendant qu'on le regardait.
#[derive(Clone, Debug, Default)]
struct Seen {
    /// Combien de fois il a été heurté, et à quelle vitesse la première.
    bumped: u32,
    impact: f32,
    /// Il a rencontré l'autre (un salut).
    met: bool,
    /// Il a quitté le sol.
    flew: bool,
    /// Le plus grand angle de son corps (culbute ou rotation), en radians.
    roll: f32,
    /// Ses abscisses : la plus à gauche, la plus à droite, la dernière.
    left: f32,
    right: f32,
    x: f32,
    /// Sa vitesse au pas qui a suivi son premier heurt, et la dernière.
    after: Option<f32>,
    speed: f32,
    /// La surface qui le portait à la fin.
    ground: Option<u64>,
}

/// Avance de `seconds`, en notant ce qui arrive à chaque animal de `ids`.
fn watch(flock: &mut Flock, world: &World, seconds: f32, ids: &[u32]) -> Vec<Seen> {
    watch_with(flock, world, seconds, ids, |_, _| None)
}

/// La main qui a pris l'animal `id` en `grip` : elle le soulève jusqu'à
/// `lifted` (0,4 s), le lance vers `thrown` en `swing` secondes, puis le
/// lâche. À donner à [`watch_with`].
fn thrower<'a>(
    id: u32,
    world: &'a World,
    [grip, lifted, thrown]: [Vec2; 3],
    swing: f32,
) -> impl FnMut(&mut Flock, f32) -> Option<Vec2> + 'a {
    move |flock, t| {
        let cursor = if t < 0.4 {
            Some(grip.lerp(lifted, t / 0.4))
        } else if t < 0.4 + swing {
            Some(lifted.lerp(thrown, (t - 0.4) / swing))
        } else {
            None
        };
        let pet = flock.get_mut(id).unwrap();
        match cursor {
            Some(c) => pet.drag_to(c),
            None if pet.is_held() => pet.release(world),
            None => {}
        }
        cursor
    }
}

/// Comme [`watch`], avec la main : avant chaque image, `hand` fait ce
/// qu'elle veut du troupeau (le temps écoulé lui est donné) et dit où est
/// le pointeur.
fn watch_with(
    flock: &mut Flock,
    world: &World,
    seconds: f32,
    ids: &[u32],
    mut hand: impl FnMut(&mut Flock, f32) -> Option<Vec2>,
) -> Vec<Seen> {
    let mut seen: Vec<Seen> = ids
        .iter()
        .map(|&id| {
            let x = flock.get_mut(id).unwrap().position().x;
            Seen {
                left: x,
                right: x,
                x,
                ..Seen::default()
            }
        })
        .collect();
    for frame in 0..(seconds * RATE).ceil() as u32 {
        let cursor = hand(flock, frame as f32 / RATE);
        let input = Input { cursor, ..input() };
        flock.advance(1.0 / RATE, world, &input);
        for (seen, &id) in seen.iter_mut().zip(ids) {
            let pet = flock.get_mut(id).unwrap();
            let events = pet.take_events();
            let hit = events.iter().find_map(|e| match e {
                PetEvent::Bumped { speed } => Some(*speed),
                _ => None,
            });
            seen.met |= events.iter().any(|e| matches!(e, PetEvent::Met { .. }));
            seen.flew |= pet.support() == Support::Air;
            seen.roll = seen.roll.max(pet.view(1.0).pose.roll.abs());
            seen.x = pet.position().x;
            seen.left = seen.left.min(seen.x);
            seen.right = seen.right.max(seen.x);
            seen.speed = pet.velocity().x;
            if let Some(speed) = hit {
                if seen.bumped == 0 {
                    seen.impact = speed;
                    seen.after = Some(seen.speed);
                }
                seen.bumped += 1;
            }
            seen.ground = match pet.support() {
                Support::Ground { surface } => Some(surface),
                _ => None,
            };
        }
    }
    seen
}

/// Deux animaux posés au sol, `gap` points entre eux, qui se font face ; le
/// premier joue `first`, le second `second`.
fn face_to_face(
    flock: &mut Flock,
    world: &World,
    (def_a, first): (&Arc<PetDef>, &str),
    (def_b, second): (&Arc<PetDef>, &str),
    gap: f32,
) -> (u32, u32) {
    let a = flock.spawn(Arc::clone(def_a), 11, world, Some(500.0));
    let b = flock.spawn(Arc::clone(def_b), 23, world, Some(500.0 + gap));
    start(flock, world, &[(a, 1.0, first), (b, -1.0, second)]);
    (a, b)
}

/// Attend que chacun soit posé, puis le tourne et lui fait jouer son
/// comportement.
fn start(flock: &mut Flock, world: &World, roles: &[(u32, f32, &str)]) {
    let input = input();
    for _ in 0..(6.0 * RATE) as u32 {
        flock.advance(1.0 / RATE, world, &input);
        let landed = roles.iter().all(|&(id, ..)| {
            matches!(flock.get_mut(id).unwrap().support(), Support::Ground { .. })
        });
        if landed {
            break;
        }
    }
    for &(id, facing, behavior) in roles {
        let pet = flock.get_mut(id).unwrap();
        assert!(
            matches!(pet.support(), Support::Ground { .. }),
            "le n° {id} ne s'est pas posé"
        );
        pet.face(facing);
        assert!(pet.force(behavior), "pas de comportement « {behavior} »");
        pet.take_events();
    }
}

/// Le troupeau en mode collisions, dans la physique voulue, sans humeur
/// tirée au sort.
fn flock(realistic: bool) -> Flock {
    let mut flock = Flock::new();
    flock.set_mood(Mood::Even);
    flock.set_collisions(true);
    flock.set_realistic(realistic);
    flock
}

#[test]
fn a_ram_that_charges_a_standing_sheep_stops_against_it_and_both_stay_on_their_feet() {
    let world = world();
    for name in ["bleu-du-maine", "mouflon-corse", "jacob", "texel"] {
        let def = bundled(name);
        for realistic in [false, true] {
            let what = format!("{name}, physique réaliste : {realistic}");
            let mut flock = flock(realistic);
            let gap = def.physics.width * 2.0;
            let (a, b) = face_to_face(&mut flock, &world, (&def, "charge"), (&def, "stand"), gap);
            let start_b = flock.get_mut(b).unwrap().position().x;
            let seen = watch(&mut flock, &world, 1.6, &[a, b]);
            let (sa, sb) = (&seen[0], &seen[1]);
            assert_eq!((sa.bumped, sb.bumped), (1, 1), "{what} : {seen:?}");
            // Ni l'un ni l'autre ne quitte le sol ni ne tourne.
            for s in &seen {
                assert!(!s.flew && s.roll < 0.01, "{what} : à la renverse {s:?}");
            }
            // Celui qui est percuté cède d'un pas : il recule de quelques
            // points, moins que sa longueur.
            let pushed = sb.right - start_b;
            assert!(
                pushed > 2.0 && pushed < def.physics.width * 0.6,
                "{what} : percuté, il a bougé de {pushed}"
            );
            // Celui qui fonce a perdu l'essentiel de son élan dans le choc,
            // et n'est pas passé à travers l'autre.
            let after = sa.after.unwrap();
            assert!(
                after > 0.0 && after < sa.impact * 0.6,
                "{what} : il repart à {after} après un choc à {}",
                sa.impact
            );
            assert!(
                sa.right < sb.x - def.physics.width * 0.5,
                "{what} : passé à travers ({} contre {})",
                sa.right,
                sb.x
            );
        }
    }
}

#[test]
fn two_rams_charging_head_on_stop_dead() {
    let world = world();
    let def = bundled("bleu-du-maine");
    for realistic in [false, true] {
        let mut flock = flock(realistic);
        let gap = def.physics.width * 3.0;
        let (a, b) = face_to_face(&mut flock, &world, (&def, "charge"), (&def, "charge"), gap);
        // Jusqu'au choc.
        let mut before = (0.0, 0.0);
        let mut hit = None;
        let input = input();
        for _ in 0..(2.0 * RATE) as u32 {
            let speeds = (
                flock.get_mut(a).unwrap().velocity().x,
                flock.get_mut(b).unwrap().velocity().x,
            );
            flock.advance(1.0 / RATE, &world, &input);
            let bumped = |flock: &mut Flock, id| {
                flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Bumped { .. }))
            };
            if bumped(&mut flock, a) | bumped(&mut flock, b) {
                before = speeds;
                hit = Some((
                    flock.get_mut(a).unwrap().position().x,
                    flock.get_mut(b).unwrap().position().x,
                ));
                break;
            }
        }
        let (at_a, at_b) = hit.expect("ils ne se sont pas heurtés");
        assert!(before.0 > 120.0 && before.1 < -120.0, "{before:?}");
        // Arrêtés net : dans la demi-seconde qui suit, un léger recul, pas
        // un rebond.
        let seen = watch(&mut flock, &world, 0.5, &[a, b]);
        for (s, at) in seen.iter().zip([at_a, at_b]) {
            assert!(!s.flew && s.roll < 0.01, "à la renverse : {s:?}");
            assert!(
                (s.left - at).abs() < 4.0 && (s.right - at).abs() < 4.0,
                "réaliste {realistic} : il a bougé de {} à {} après le choc en {at}",
                s.left,
                s.right
            );
            assert!(s.speed.abs() < 1.0, "il glisse encore : {s:?}");
        }
        // Et la suite se passe sur leurs pattes.
        for s in watch(&mut flock, &world, 2.0, &[a, b]) {
            assert!(!s.flew && s.roll < 0.01, "à la renverse : {s:?}");
        }
    }
}

#[test]
fn sheep_that_meet_at_a_walk_greet_each_other_instead_of_colliding() {
    let world = world();
    let def = bundled("bleu-du-maine");
    for realistic in [false, true] {
        let mut flock = flock(realistic);
        let gap = def.physics.width * 1.6;
        let (a, b) = face_to_face(&mut flock, &world, (&def, "walk"), (&def, "walk"), gap);
        let seen = watch(&mut flock, &world, 2.5, &[a, b]);
        for s in &seen {
            assert_eq!(s.bumped, 0, "heurtés au pas : {s:?}");
            assert!(!s.flew && s.roll < 0.01, "à la renverse : {s:?}");
        }
        assert!(
            seen[0].met && seen[1].met,
            "réaliste {realistic} : ils ne se sont pas salués ({seen:?})"
        );
    }
}

#[test]
fn the_heavier_one_carries_the_lighter_along() {
    let world = world();
    let (big, small) = (sized("gros", 80.0, 60.0), sized("petit", 40.0, 30.0));
    // Le gros fonce sur le petit, puis le petit sur le gros.
    let run = |striker: &Arc<PetDef>, struck: &Arc<PetDef>| {
        let mut flock = flock(false);
        let (a, b) = face_to_face(
            &mut flock,
            &world,
            (striker, "charge"),
            (struck, "stand"),
            150.0,
        );
        let start = flock.get_mut(b).unwrap().position().x;
        let seen = watch(&mut flock, &world, 1.2, &[a, b]);
        assert_eq!((seen[0].bumped, seen[1].bumped), (1, 1), "{seen:?}");
        for s in &seen {
            assert!(!s.flew && s.roll < 0.01, "à la renverse : {s:?}");
        }
        (
            seen[0].after.unwrap() / seen[0].impact,
            seen[1].right - start,
        )
    };
    let (big_keeps, small_pushed) = run(&big, &small);
    let (small_keeps, big_pushed) = run(&small, &big);
    // Le gros garde presque tout son élan et emporte le petit ; le petit
    // s'arrête contre le gros, qui bouge à peine.
    assert!(big_keeps > 0.7, "le gros ne garde que {big_keeps}");
    assert!(small_keeps < 0.2, "le petit garde {small_keeps}");
    assert!(
        small_pushed > 6.0 * big_pushed.max(0.5),
        "le petit recule de {small_pushed}, le gros de {big_pushed}"
    );
}

#[test]
fn a_sheep_shoved_at_the_end_of_a_ledge_stays_on_it() {
    let def = bundled("bleu-du-maine");
    let w = def.physics.width;
    for realistic in [false, true] {
        let mut world = world();
        // Un rebord de 100 à 700, à la hauteur 400.
        world.set_windows(
            &[WindowRect {
                id: 7,
                frame: Rect::new(100.0, 400.0, 600.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let mut flock = flock(realistic);
        // Le second est au bout du rebord, le premier fonce sur lui.
        let a = flock.spawn(Arc::clone(&def), 11, &world, Some(700.0 - w * 2.4));
        let b = flock.spawn(Arc::clone(&def), 23, &world, Some(700.0 - w * 0.45));
        start(
            &mut flock,
            &world,
            &[(a, 1.0, "charge"), (b, -1.0, "stand")],
        );
        let on_ledge = |flock: &mut Flock, id| {
            let pet = flock.get_mut(id).unwrap();
            match pet.support() {
                Support::Ground { surface } => world
                    .surface(surface)
                    .is_some_and(|s| s.kind == SurfaceKind::Window),
                _ => false,
            }
        };
        assert!(
            on_ledge(&mut flock, a) && on_ledge(&mut flock, b),
            "ils ne sont pas sur le rebord"
        );
        let seen = watch(&mut flock, &world, 2.0, &[a, b]);
        assert_eq!(seen[1].bumped, 1, "réaliste {realistic} : {seen:?}");
        for s in &seen {
            assert!(!s.flew && s.roll < 0.01, "tombé du rebord : {s:?}");
        }
        // Il glisserait de plus de dix points : son museau s'arrête au bout
        // du rebord, pas plus loin.
        let hold = 700.0 - w * 0.32;
        assert!(
            on_ledge(&mut flock, b) && seen[1].right <= hold + 0.01,
            "réaliste {realistic} : poussé au-delà du bout du rebord ({} > {hold})",
            seen[1].right
        );
        assert!(
            seen[1].right > hold - 1.0,
            "réaliste {realistic} : il n'est pas allé jusqu'au bout ({} < {hold}), \
             l'essai ne dit rien du bord",
            seen[1].right
        );
    }
}

#[test]
fn a_small_one_hit_by_a_thrown_one_at_the_end_of_a_ledge_stays_on_it() {
    let (big, small) = (sized("gros", 60.0, 48.0), sized("petit", 30.0, 24.0));
    for realistic in [false, true] {
        let mut world = world();
        // Un rebord de 100 à 700, à la hauteur 400.
        world.set_windows(
            &[WindowRect {
                id: 7,
                frame: Rect::new(100.0, 400.0, 600.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let mut flock = flock(realistic);
        let a = flock.spawn(Arc::clone(&big), 11, &world, Some(250.0));
        let b = flock.spawn(Arc::clone(&small), 23, &world, Some(700.0 - 30.0 * 0.45));
        start(&mut flock, &world, &[(a, 1.0, "stand"), (b, -1.0, "stand")]);
        let ledge = match flock.get_mut(b).unwrap().support() {
            Support::Ground { surface } => surface,
            other => panic!("le petit n'est pas posé : {other:?}"),
        };
        assert_eq!(
            world.surface(ledge).map(|s| s.kind),
            Some(SurfaceKind::Window),
            "le petit n'est pas sur le rebord"
        );
        // La main prend le gros et le lance à toute volée le long du rebord,
        // sur le petit posé au bout.
        let grip = flock.get_mut(a).unwrap().position() + Vec2::new(0.0, -20.0);
        flock.get_mut(a).unwrap().grab(grip, &world);
        let hand = thrower(
            a,
            &world,
            [grip, Vec2::new(150.0, 330.0), Vec2::new(520.0, 350.0)],
            0.15,
        );
        let seen = watch_with(&mut flock, &world, 3.0, &[b], hand);
        let what = format!("réaliste {realistic} : {:?}", seen[0]);
        assert!(seen[0].bumped >= 1, "le petit n'a pas été heurté ; {what}");
        assert!(
            seen[0].impact > 1500.0,
            "le choc n'est pas celui d'un lancer à toute volée ; {what}"
        );
        assert!(
            !seen[0].flew && seen[0].roll < 0.01 && seen[0].ground == Some(ledge),
            "le petit a quitté le rebord ; {what}"
        );
        assert!(
            seen[0].right <= 700.0 - 30.0 * 0.32 + 0.01,
            "le petit a été poussé au-delà du bout ; {what}"
        );
    }
}

#[test]
fn a_digital_mascot_still_tumbles_but_the_sheep_it_runs_into_does_not() {
    let world = world();
    let (sheep, mascot) = (bundled("bleu-du-maine"), mascot());
    let mut flock = flock(false);
    let (a, b) = face_to_face(
        &mut flock,
        &world,
        (&mascot, "walk"),
        (&sheep, "stand"),
        90.0,
    );
    let seen = watch(&mut flock, &world, 2.5, &[a, b]);
    assert_eq!((seen[0].bumped, seen[1].bumped), (1, 1), "{seen:?}");
    // (Sa culbute est dans ses images : on la reconnaît à son vol, loin en
    // arrière.)
    assert!(
        seen[0].flew && seen[0].x < seen[0].right - 60.0,
        "la mascotte ne culbute plus : {:?}",
        seen[0]
    );
    assert!(
        !seen[1].flew && seen[1].roll < 0.01,
        "le mouton est parti à la renverse : {:?}",
        seen[1]
    );
}

#[test]
fn a_sheep_thrown_at_another_shoves_it_without_knocking_it_over() {
    let world = world();
    let def = bundled("bleu-du-maine");
    for realistic in [false, true] {
        let mut flock = flock(realistic);
        let a = flock.spawn(Arc::clone(&def), 11, &world, Some(400.0));
        let b = flock.spawn(Arc::clone(&def), 23, &world, Some(700.0));
        start(&mut flock, &world, &[(a, 1.0, "stand"), (b, -1.0, "stand")]);
        let floor = flock.get_mut(b).unwrap().position().y;
        let start_b = flock.get_mut(b).unwrap().position().x;
        // (Sa foulée à lui : bélier, brebis ou agneau, le sort en décide.)
        let stride = flock.get_mut(b).unwrap().def().stride();
        // La main prend le premier, le soulève, puis le lance à ras du sol
        // vers le second.
        let grip = flock.get_mut(a).unwrap().position() + Vec2::new(0.0, -20.0);
        flock.get_mut(a).unwrap().grab(grip, &world);
        let hand = thrower(
            a,
            &world,
            [
                grip,
                Vec2::new(380.0, floor - 90.0),
                Vec2::new(600.0, floor - 45.0),
            ],
            0.22,
        );
        let seen = watch_with(&mut flock, &world, 3.5, &[b], hand);
        let (seen_b, gave) = (&seen[0], seen[0].right - start_b);
        let what = format!("réaliste {realistic} : {seen_b:?}");
        assert!(seen_b.bumped >= 1, "le second n'a pas été heurté ; {what}");
        assert!(
            !seen_b.flew && seen_b.roll < 0.01,
            "percuté par un mouton lancé, il est parti à la renverse ; {what}"
        );
        assert!(gave > 2.0, "il n'a pas cédé d'un pas ; {what}");
        // Si fort qu'on lance l'autre, il ne cède que d'un pas ou deux : il
        // ne glisse pas d'un bout à l'autre de l'écran.
        let most = 2.0 * stride;
        assert!(
            seen_b.impact > 300.0 && gave <= most + 0.5,
            "percuté à {} pt/s, il a glissé de {gave} points (deux foulées : {most}) ; {what}",
            seen_b.impact
        );
    }
}

#[test]
fn one_that_tumbles_flies_no_farther_shouldered_by_a_rowdy_sheep_than_hit_at_the_same_pace() {
    let world = world();
    let (sheep, tumbler) = (bundled("bleu-du-maine"), tumbler("culbuto", 60.0, 64.0));
    // Le même mouton, au même pas, l'atteint deux fois : en énervé qui le
    // bouscule (hors du mode collisions), puis en mode collisions.
    let run = |rowdy: bool| {
        let mut flock = if rowdy {
            let mut flock = Flock::new();
            flock.set_mood(Mood::Rowdy);
            flock
        } else {
            flock(false)
        };
        let (a, b) = face_to_face(
            &mut flock,
            &world,
            (&sheep, "walk"),
            (&tumbler, "stand"),
            90.0,
        );
        let start_b = flock.get_mut(b).unwrap().position().x;
        let seen = watch(&mut flock, &world, 2.5, &[a, b]);
        assert!(
            seen[1].bumped == 1 && seen[1].flew,
            "énervé {rowdy} : il n'a pas culbuté ({:?})",
            seen[1]
        );
        // (Pour qui culbute, l'événement porte l'élan qu'il reçoit.)
        (seen[1].right - start_b, seen[1].impact)
    };
    let ((shouldered, push), (hit, same_push)) = (run(true), run(false));
    // Sa culbute vaut leur vraie vitesse : la bousculade d'un énervé ne lui
    // donne pas plus d'élan, et ne l'envoie pas plus loin.
    assert!(
        (push - same_push).abs() < 1.0,
        "bousculé au pas, il reçoit un élan de {push} ; heurté au même pas, de {same_push}"
    );
    assert!(
        (shouldered - hit).abs() < 2.0,
        "bousculé au pas, il recule de {shouldered} ; heurté au même pas, de {hit}"
    );
}

#[test]
fn a_rowdy_sheep_shoulders_the_other_aside_and_walks_on() {
    let world = world();
    let def = bundled("bleu-du-maine");
    // Hors du mode collisions : seuls les énervés bousculent.
    let mut flock = Flock::new();
    flock.set_mood(Mood::Rowdy);
    let (a, b) = face_to_face(&mut flock, &world, (&def, "walk"), (&def, "stand"), 80.0);
    let start_b = flock.get_mut(b).unwrap().position().x;
    let seen = watch(&mut flock, &world, 1.5, &[a, b]);
    assert_eq!(
        (seen[0].bumped, seen[1].bumped),
        (0, 1),
        "seul celui qu'il bouscule encaisse : {seen:?}"
    );
    for s in &seen {
        assert!(!s.flew && s.roll < 0.01, "à la renverse : {s:?}");
    }
    assert!(
        seen[1].right - start_b > 1.0,
        "bousculé, il n'a pas bougé : {seen:?}"
    );
}
