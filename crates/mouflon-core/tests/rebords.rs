//! Le rebord d'une fenêtre, coupé par une autre posée devant : chaque animal
//! reste sur le bout qu'on voit encore, sans broncher. Les morceaux d'un
//! rebord sont numérotés de gauche à droite ; quand une fenêtre vient
//! s'ouvrir devant, bouge ou s'en va, ils changent de numéro. Avant,
//! l'animal resté à droite de la coupe perdait son rebord (toujours
//! visible), partait en chute et se reposait aussitôt sur le bon morceau :
//! un sursaut, son réflexe d'atterrissage et son bruit, pour tous ceux de
//! droite à la fois ; de même pour ceux du second morceau quand la fenêtre
//! de devant s'en allait.

use std::sync::Arc;

use mouflon_core::{
    Flock, Input, LedgeOptions, PetDef, PetEvent, Rect, Screen, Sex, Support, SurfaceKind, TICK,
    WindowRect, World, load_pet, parse_pet_toml,
};

/// Un mouton d'essai qui reste où il se pose.
const SITTER: &str = r#"
    [pet]
    id = "pose"
    name = "Posé"
    [appearance]
    rig = "a.svg"
    height = 45.0
    [physics]
    width = 57.5
    stride = 28.0
    bounce = 0.0
    [reflexes]
    spawn = "fall"
    fall = "fall"
    land = "stand"
    thrown = "fall"
    held = "held"
    [[behavior]]
    name = "stand"
    motion = "stand"
    duration = "forever"
    [[behavior]]
    name = "held"
    motion = "held"
    duration = "forever"
    [[behavior]]
    name = "fall"
    motion = "fall"
    duration = "forever"
"#;

fn sitter() -> Arc<PetDef> {
    Arc::new(parse_pet_toml(SITTER, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
}

/// Pelote, un animal au format desktopPet des données de test : ses
/// mouvements sont écrits image par image.
fn scripted() -> Arc<PetDef> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/donnees/pelote/pet.toml");
    Arc::new(load_pet(&path).unwrap())
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 835.0),
        scale: 1.0,
    }])
}

fn window(id: u64, x: f32, y: f32, w: f32, h: f32) -> WindowRect {
    WindowRect {
        id,
        frame: Rect::new(x, y, w, h),
    }
}

/// La fenêtre du fond : son rebord va de 100 à 900, à la hauteur 300.
fn back() -> WindowRect {
    window(1, 100.0, 300.0, 800.0, 300.0)
}

fn advance(flock: &mut Flock, world: &World, seconds: f32) {
    still(flock, world, seconds, &[], "");
}

/// Avance de `seconds` ; les animaux `calm` ne doivent pas broncher : à
/// chaque pas, posés, debout, sans le moindre atterrissage.
fn still(flock: &mut Flock, world: &World, seconds: f32, calm: &[u32], what: &str) {
    let input = Input::default();
    let frames = (seconds * 60.0).ceil() as u32;
    // (Ce qui leur est arrivé avant ne compte pas.)
    for &id in calm {
        flock.get_mut(id).unwrap().take_events();
    }
    for frame in 0..frames {
        flock.advance(1.0 / 60.0, world, &input);
        for &id in calm {
            let pet = flock.get_mut(id).unwrap();
            let landed = pet
                .take_events()
                .iter()
                .any(|e| matches!(e, PetEvent::Landed { .. }));
            assert!(
                matches!(pet.support(), Support::Ground { .. })
                    && pet.behavior_name() == "stand"
                    && !landed,
                "{what} : le n° {id} a bronché à l'image {frame} ({:?}, {}, atterri : {landed})",
                pet.support(),
                pet.behavior_name(),
            );
        }
    }
}

/// Où est l'animal `id` : la fenêtre qui le porte (ou rien : un sol, ou en
/// l'air), son abscisse et sa hauteur.
fn place(flock: &Flock, world: &World, id: u32) -> (Option<u64>, f32, f32) {
    let pet = flock.pets().iter().find(|p| p.id() == id).unwrap();
    let on = match pet.support() {
        Support::Ground { surface } => world
            .surface(surface)
            .filter(|s| s.kind == SurfaceKind::Window)
            .map(|s| s.id >> 8),
        _ => None,
    };
    (on, pet.position().x, pet.position().y)
}

/// Trois moutons posés sur le rebord du fond, à gauche (200), au milieu
/// (500) et à droite (800).
fn three_on_the_ledge(realistic: bool) -> (Flock, World, [u32; 3]) {
    let mut world = world();
    world.set_windows(&[back()], LedgeOptions::default());
    let mut flock = Flock::new();
    flock.set_realistic(realistic);
    let ids = [200.0, 500.0, 800.0].map(|x| flock.spawn(sitter(), 7, &world, Some(x)));
    advance(&mut flock, &world, 3.0);
    for (id, x) in ids.into_iter().zip([200.0, 500.0, 800.0]) {
        assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0), "posé");
    }
    (flock, world, ids)
}

#[test]
fn a_sheep_stays_on_the_piece_of_ledge_still_showing_when_a_window_opens_in_front() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, middle, right]) = three_on_the_ledge(realistic);
        // Une fenêtre s'ouvre devant, de 300 à 700 : le rebord du fond n'est
        // plus que deux bouts, de 100 à 300 et de 700 à 900.
        let front = window(2, 300.0, 200.0, 400.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let what = if realistic { "réaliste" } else { "classique" };
        still(&mut flock, &world, 2.0, &[left, right], what);
        assert_eq!(
            place(&flock, &world, left),
            (Some(1), 200.0, 300.0),
            "{what} : celui de gauche reste"
        );
        assert_eq!(
            place(&flock, &world, right),
            (Some(1), 800.0, 300.0),
            "{what} : celui de droite reste sur le bout qu'on voit"
        );
        // Celui du milieu, que la fenêtre recouvre, n'a plus rien sous lui :
        // il est tombé au sol.
        let (on, x, y) = place(&flock, &world, middle);
        assert_eq!((on, y), (None, 860.0), "{what} : celui du milieu tombe");
        assert!((x - 500.0).abs() < 1.0, "{what} : tombé droit ({x})");
    }
}

#[test]
fn a_sheep_on_the_second_piece_stays_when_the_window_in_front_goes_away() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, _, right]) = three_on_the_ledge(realistic);
        let front = window(2, 300.0, 200.0, 400.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        advance(&mut flock, &world, 2.0);
        // La fenêtre de devant se ferme : le rebord redevient d'un seul
        // tenant, et son second morceau n'existe plus.
        world.set_windows(&[back()], LedgeOptions::default());
        let what = if realistic { "réaliste" } else { "classique" };
        still(&mut flock, &world, 2.0, &[left, right], what);
        assert_eq!(
            place(&flock, &world, left),
            (Some(1), 200.0, 300.0),
            "{what}"
        );
        assert_eq!(
            place(&flock, &world, right),
            (Some(1), 800.0, 300.0),
            "{what} : celui de droite reste quand les deux bouts se rejoignent"
        );
    }
}

#[test]
fn a_sheep_rides_its_window_when_it_slides_behind_the_one_in_front() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, _, right]) = three_on_the_ledge(realistic);
        let front = window(2, 300.0, 200.0, 400.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        advance(&mut flock, &world, 1.0);
        // La fenêtre du fond glisse de 60 points vers la droite, par petits
        // déplacements (on la tire) : ses bouts visibles changent de
        // largeur, mais elle emporte ceux qu'elle porte.
        let what = if realistic { "réaliste" } else { "classique" };
        for step in 1..=30 {
            let moved = window(1, 100.0 + 2.0 * step as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[front, moved], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 30.0, &[left, right], what);
        }
        still(&mut flock, &world, 1.0, &[left, right], what);
        for (id, name, to) in [(left, "gauche", 260.0), (right, "droite", 860.0)] {
            let (on, x, y) = place(&flock, &world, id);
            assert_eq!((on, y), (Some(1), 300.0), "{what} : celui de {name} reste");
            assert!(
                (x - to).abs() < 0.5,
                "{what} : celui de {name} n'a pas suivi sa fenêtre ({x}, attendu {to})"
            );
        }
    }
}

#[test]
fn a_sheep_rides_a_window_dragged_half_off_the_screen() {
    for realistic in [false, true] {
        let mut world = world();
        // Une fenêtre à cheval sur le bord droit de l'écran (1440) : on ne
        // voit de son rebord que le bout de 1000 à 1440.
        world.set_windows(
            &[window(1, 1000.0, 300.0, 800.0, 300.0)],
            LedgeOptions::default(),
        );
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(sitter(), 7, &world, Some(1200.0));
        advance(&mut flock, &world, 3.0);
        assert_eq!(place(&flock, &world, id), (Some(1), 1200.0, 300.0), "posé");
        // On la ramène de 60 points vers la gauche : le bout qu'on voit
        // s'allonge, et elle emporte celui qu'elle porte.
        let what = if realistic { "réaliste" } else { "classique" };
        for step in 1..=30 {
            let moved = window(1, 1000.0 - 2.0 * step as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[moved], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 30.0, &[id], what);
        }
        still(&mut flock, &world, 1.0, &[id], what);
        let (on, x, y) = place(&flock, &world, id);
        assert_eq!((on, y), (Some(1), 300.0), "{what}");
        assert!(
            (x - 1140.0).abs() < 0.5,
            "{what} : il n'a pas suivi sa fenêtre ({x}, attendu 1140)"
        );
    }
}

#[test]
fn a_sheep_stays_where_it_is_when_its_window_is_resized() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, middle, right]) = three_on_the_ledge(realistic);
        // On tire le bord gauche de la fenêtre de 60 points vers la droite,
        // puis son bord droit de 60 points vers la gauche : elle rétrécit
        // sans bouger, et ne les emporte pas.
        let what = if realistic { "réaliste" } else { "classique" };
        let all = [left, middle, right];
        for step in 1..=30 {
            let d = 2.0 * step as f32;
            let resized = window(1, 100.0 + d, 300.0, 800.0 - d, 300.0);
            world.set_windows(&[resized], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 30.0, &all, what);
        }
        for step in 1..=30 {
            let d = 2.0 * step as f32;
            let resized = window(1, 160.0, 300.0, 740.0 - d, 300.0);
            world.set_windows(&[resized], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 30.0, &all, what);
        }
        still(&mut flock, &world, 1.0, &all, what);
        for (id, x) in all.into_iter().zip([200.0, 500.0, 800.0]) {
            assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0), "{what}");
        }
    }
}

#[test]
fn a_sheep_rides_a_window_that_slides_slowly_behind_another() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, _, right]) = three_on_the_ledge(realistic);
        let front = window(2, 300.0, 200.0, 400.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        advance(&mut flock, &world, 1.0);
        // La fenêtre du fond glisse de 60 points vers la droite, de moins
        // d'un demi-point par relevé (un pixel d'un écran à 250 %). Le bout
        // de droite garde son bord gauche (celui de la fenêtre de devant) :
        // il ne fait que s'allonger d'un rien à chaque fois.
        let what = if realistic { "réaliste" } else { "classique" };
        for step in 1..=150 {
            let moved = window(1, 100.0 + 0.4 * step as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[front, moved], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 60.0, &[left, right], what);
        }
        still(&mut flock, &world, 1.0, &[left, right], what);
        for (id, name, to) in [(left, "gauche", 260.0), (right, "droite", 860.0)] {
            let (on, x, y) = place(&flock, &world, id);
            assert_eq!((on, y), (Some(1), 300.0), "{what} : celui de {name} reste");
            assert!(
                (x - to).abs() < 0.5,
                "{what} : celui de {name} n'a pas suivi sa fenêtre ({x}, attendu {to})"
            );
        }
    }
}

#[test]
fn a_sheep_stays_where_it_is_when_its_window_is_resized_slowly() {
    for realistic in [false, true] {
        let (mut flock, mut world, [left, middle, right]) = three_on_the_ledge(realistic);
        // Le bord gauche de la fenêtre avance de 60 points, de moins d'un
        // demi-point par relevé : elle rétrécit, elle ne bouge pas.
        let what = if realistic { "réaliste" } else { "classique" };
        let all = [left, middle, right];
        for step in 1..=150 {
            let d = 0.4 * step as f32;
            let resized = window(1, 100.0 + d, 300.0, 800.0 - d, 300.0);
            world.set_windows(&[resized], LedgeOptions::default());
            still(&mut flock, &world, 1.0 / 60.0, &all, what);
        }
        still(&mut flock, &world, 1.0, &all, what);
        for (id, x) in all.into_iter().zip([200.0, 500.0, 800.0]) {
            let (on, at, y) = place(&flock, &world, id);
            assert_eq!((on, y), (Some(1), 300.0), "{what}");
            assert!(
                (at - x).abs() < 0.5,
                "{what} : la fenêtre qu'on redimensionne l'a emporté ({at}, il était en {x})"
            );
        }
    }
}

#[test]
fn a_sheep_that_has_just_landed_rides_its_window_from_the_very_next_poll() {
    // (Physique classique : en physique réaliste, la plateforme emporte
    // l'animal par ses pattes, avec l'élan qu'il faut.)
    let mut world = world();
    // Une fenêtre à cheval sur le bord droit de l'écran : son bout visible
    // change de largeur dès qu'elle bouge.
    world.set_windows(
        &[window(1, 1000.0, 300.0, 800.0, 300.0)],
        LedgeOptions::default(),
    );
    let mut flock = Flock::new();
    let id = flock.spawn(sitter(), 7, &world, Some(1200.0));
    // Il tombe et se pose. Dès le pas où il se pose, un pas de simulation
    // à la fois, la fenêtre recule de 3 points avant chaque pas.
    let input = Input::default();
    let mut moved = 0.0;
    let mut landed = false;
    for _ in 0..2400 {
        flock.advance(TICK, &world, &input);
        landed |= place(&flock, &world, id).0 == Some(1);
        if landed {
            if moved >= 60.0 {
                break;
            }
            moved += 3.0;
            let frame = window(1, 1000.0 - moved, 300.0, 800.0, 300.0);
            world.set_windows(&[frame], LedgeOptions::default());
        }
    }
    assert!(landed, "il ne s'est pas posé sur la fenêtre");
    flock.advance(TICK, &world, &input);
    let (on, x, _) = place(&flock, &world, id);
    assert_eq!(on, Some(1));
    assert!(
        (x - (1200.0 - moved)).abs() < 0.5,
        "il a pris du retard sur sa fenêtre : en {x}, attendu {}",
        1200.0 - moved
    );
}

#[test]
fn a_scripted_pet_rides_its_window_too() {
    for realistic in [false, true] {
        let mut world = world();
        let front = window(2, 300.0, 200.0, 400.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(scripted(), 7, &world, Some(800.0));
        let what = if realistic { "réaliste" } else { "classique" };
        // Elle se pose sur le bout de droite, et s'endort (elle ne bouge
        // plus d'elle-même).
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{what} : pas posée");
        assert!(flock.get_mut(id).unwrap().force("sleep1b"), "{what}");
        advance(&mut flock, &world, 0.5);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(
            on,
            Some(1),
            "{what} : elle a quitté le rebord en s'endormant"
        );
        // La fenêtre du fond glisse de 40 points vers la droite.
        for step in 1..=20 {
            let moved = window(1, 100.0 + 2.0 * step as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[front, moved], LedgeOptions::default());
            advance(&mut flock, &world, 1.0 / 30.0);
        }
        advance(&mut flock, &world, 0.5);
        let (on, x, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{what} : elle est tombée de sa fenêtre");
        assert!(
            (x - (from + 40.0)).abs() < 1.0,
            "{what} : elle n'a pas suivi sa fenêtre (de {from} à {x}, attendu {})",
            from + 40.0
        );
    }
}

#[test]
fn a_pet_falls_straight_when_its_window_moves_out_of_reach() {
    for (pet, sleep, who) in [
        (sitter(), None, "posé"),
        (scripted(), Some("sleep1b"), "scripté"),
    ] {
        for realistic in [false, true] {
            let mut world = world();
            world.set_windows(&[back()], LedgeOptions::default());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(500.0));
            let what = format!(
                "{who}, {}",
                if realistic { "réaliste" } else { "classique" }
            );
            let input = Input::default();
            for _ in 0..600 {
                flock.advance(1.0 / 60.0, &world, &input);
                if place(&flock, &world, id).0 == Some(1) {
                    break;
                }
            }
            assert_eq!(place(&flock, &world, id).0, Some(1), "{what} : pas posé");
            if let Some(sleep) = sleep {
                // (Endormie, elle ne bouge plus d'elle-même.)
                assert!(flock.get_mut(id).unwrap().force(sleep), "{what}");
            }
            advance(&mut flock, &world, 0.5);
            let (on, from, _) = place(&flock, &world, id);
            assert_eq!(on, Some(1), "{what}");
            // On tire la fenêtre tout en haut de l'écran, et de 30 points
            // vers la droite : si près du haut, elle n'a plus de rebord où
            // se tenir. L'animal tombe d'où il était, sans faire d'abord un
            // bond de côté avec elle.
            world.set_windows(
                &[window(1, 130.0, 40.0, 800.0, 300.0)],
                LedgeOptions::default(),
            );
            flock.advance(TICK, &world, &input);
            let (on, x, _) = place(&flock, &world, id);
            assert_eq!(on, None, "{what} : il tient à un rebord qui n'est plus là");
            assert!(
                (x - from).abs() < 2.0,
                "{what} : un bond de côté avant de tomber (de {from} à {x})"
            );
        }
    }
}

#[test]
fn a_pet_carried_over_a_window_in_front_stays_on_its_window() {
    // Dans les deux physiques. (En physique réaliste, la plateforme n'emporte
    // l'animal qu'après qu'il a cherché son morceau : il le cherche là où
    // elle va le poser. Avant, il le cherchait là où il était : le mouton
    // sursautait, et l'animal desktopPet, posé sur le morceau resté sous
    // lui, en partait jusqu'au sol.)
    for (realistic, pet, sleep, who) in [
        (false, sitter(), None, "posé, physique classique"),
        (
            false,
            scripted(),
            Some("sleep1b"),
            "scripté, physique classique",
        ),
        (true, sitter(), None, "posé, physique réaliste"),
        (
            true,
            scripted(),
            Some("sleep1b"),
            "scripté, physique réaliste",
        ),
    ] {
        let mut world = world();
        // Devant la fenêtre du fond, deux fenêtres : l'une cache son rebord
        // jusqu'à 300, l'autre de 500 à 600. On en voit deux bouts, de 300
        // à 500 et de 600 à 900.
        let fronts = [
            window(2, 50.0, 200.0, 250.0, 400.0),
            window(3, 500.0, 200.0, 100.0, 400.0),
        ];
        world.set_windows(&[fronts[0], fronts[1], back()], LedgeOptions::default());
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(440.0));
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{who} : pas posé");
        if let Some(sleep) = sleep {
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
        }
        advance(&mut flock, &world, 1.0);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who}");
        flock.get_mut(id).unwrap().take_events();
        // D'un seul coup, la fenêtre du fond glisse de 170 points vers la
        // droite : elle l'emmène par-dessus la fenêtre de devant, sur son
        // second bout (de 600 à 1070). Le premier bout, tenu des deux côtés,
        // n'a pas bougé : ce n'est plus le sien.
        let moved = window(1, 270.0, 300.0, 800.0, 300.0);
        world.set_windows(&[fronts[0], fronts[1], moved], LedgeOptions::default());
        for step in 0..120 {
            flock.advance(TICK, &world, &input);
            let (on, x, y) = place(&flock, &world, id);
            let landed = flock
                .get_mut(id)
                .unwrap()
                .take_events()
                .iter()
                .any(|e| matches!(e, PetEvent::Landed { .. }));
            assert!(
                on == Some(1) && y == 300.0 && !landed,
                "{who} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
            );
        }
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - (from + 170.0)).abs() < 1.0,
            "{who} : il n'a pas suivi sa fenêtre (de {from} à {x})"
        );
    }
}

#[test]
fn a_lamb_that_has_just_arrived_rides_the_window_with_its_mother() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine/pet.toml");
    let breed = Arc::new(load_pet(&path).unwrap());
    let mut world = world();
    world.set_windows(&[back()], LedgeOptions::default());
    let mut flock = Flock::new();
    let ewe = flock.spawn_as(
        Arc::clone(&breed),
        7,
        &world,
        Some(500.0),
        Some(Sex::Female),
    );
    let input = Input::default();
    for _ in 0..600 {
        flock.advance(1.0 / 60.0, &world, &input);
        if place(&flock, &world, ewe).0 == Some(1) {
            break;
        }
    }
    assert_eq!(
        place(&flock, &world, ewe).0,
        Some(1),
        "la brebis n'est pas posée"
    );
    advance(&mut flock, &world, 0.5);
    // L'agneau arrive près d'elle (hors d'un pas de simulation), et la
    // fenêtre bouge de 20 points avant le pas suivant.
    let lamb = flock
        .spawn_lamb(ewe, 11, 4.0, &world)
        .expect("un agneau pour la brebis");
    let before = [ewe, lamb].map(|id| place(&flock, &world, id));
    assert_eq!(before[1].0, Some(1), "l'agneau n'est pas sur la fenêtre");
    world.set_windows(
        &[window(1, 120.0, 300.0, 800.0, 300.0)],
        LedgeOptions::default(),
    );
    flock.advance(TICK, &world, &input);
    for (id, (_, from, _), who) in [(ewe, before[0], "la brebis"), (lamb, before[1], "l'agneau")] {
        let (on, x, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who} a quitté la fenêtre");
        assert!(
            (x - (from + 20.0)).abs() < 1.5,
            "{who} n'a pas suivi la fenêtre (de {from} à {x}, attendu {})",
            from + 20.0
        );
    }
}

#[test]
fn a_sheep_whose_menu_was_open_catches_up_with_its_window() {
    let (mut flock, mut world, [left, _, right]) = three_on_the_ledge(false);
    // Le menu de celui de gauche est ouvert : il attend, hors du temps,
    // pendant que la fenêtre glisse de 60 points.
    flock.hold_still(Some(left));
    for step in 1..=30 {
        let moved = window(1, 100.0 + 2.0 * step as f32, 300.0, 800.0, 300.0);
        world.set_windows(&[moved], LedgeOptions::default());
        still(&mut flock, &world, 1.0 / 30.0, &[right], "classique");
    }
    assert_eq!(
        place(&flock, &world, left).1,
        200.0,
        "il a bougé, menu ouvert"
    );
    assert_eq!(place(&flock, &world, right).1, 860.0);
    // Le menu se ferme : il rattrape sa fenêtre, d'un coup, sans tomber.
    flock.hold_still(None);
    still(&mut flock, &world, 0.5, &[left, right], "classique");
    assert_eq!(place(&flock, &world, left), (Some(1), 260.0, 300.0));
}

#[test]
fn a_sheep_does_not_slide_when_realistic_physics_is_turned_off_on_a_moving_window() {
    let (mut flock, mut world, pets) = three_on_the_ledge(true);
    let input = Input::default();
    // On tire la fenêtre à 600 points par seconde ; en chemin, la physique
    // réaliste est décochée.
    let mut at = 100.0;
    let mut drag = |flock: &mut Flock, world: &mut World, steps: usize| {
        for _ in 0..steps {
            at += 5.0;
            world.set_windows(
                &[window(1, at, 300.0, 800.0, 300.0)],
                LedgeOptions::default(),
            );
            flock.advance(TICK, world, &input);
        }
    };
    drag(&mut flock, &mut world, 60);
    flock.set_realistic(false);
    // Le premier pas classique : chacun est posé sur le rebord tel qu'il
    // est (plus sur celui, lissé, de la physique réaliste), sans garder la
    // vitesse de la fenêtre.
    drag(&mut flock, &mut world, 1);
    let first = pets.map(|id| place(&flock, &world, id).1);
    for id in pets {
        let speed = flock.get_mut(id).unwrap().velocity().x;
        assert!(
            speed.abs() < 1.0,
            "il garde {speed} pt/s de la fenêtre qui le portait"
        );
    }
    // Ensuite, il avance avec elle, ni plus ni moins, et s'arrête avec elle.
    drag(&mut flock, &mut world, 29);
    advance(&mut flock, &world, 1.5);
    for (id, from) in pets.into_iter().zip(first) {
        let (on, x, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "il a quitté sa fenêtre");
        assert!(
            (x - (from + 145.0)).abs() < 1.0,
            "il a glissé sur sa fenêtre : de {from} à {x}, elle a avancé de 145"
        );
    }
}

/// Le morceau de rebord qui porte l'animal `id`, s'il est sur une fenêtre.
fn piece_under(flock: &Flock, world: &World, id: u32) -> Option<(f32, f32)> {
    let pet = flock.pets().iter().find(|p| p.id() == id)?;
    match pet.support() {
        Support::Ground { surface } => world
            .surface(surface)
            .filter(|s| s.kind == SurfaceKind::Window)
            .map(|s| (s.x0, s.x1)),
        _ => None,
    }
}

#[test]
fn the_piece_of_a_ledge_nearest_to_a_spot_is_found_within_a_margin_only() {
    let mut world = world();
    // Une fenêtre de devant cache le rebord du fond de 500 à 600.
    let front = window(2, 500.0, 200.0, 100.0, 400.0);
    world.set_windows(&[front, back()], LedgeOptions::default());
    let mut pieces: Vec<_> = world
        .surfaces()
        .iter()
        .filter(|s| s.kind == SurfaceKind::Window && s.id >> 8 == 1)
        .copied()
        .collect();
    pieces.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    assert_eq!(pieces.len(), 2);
    let (left, right) = (pieces[0], pieces[1]);
    assert_eq!(
        (left.x0, left.x1, right.x0, right.x1),
        (100.0, 500.0, 600.0, 900.0)
    );
    let found = |from: u64, x: f32, margin: f32| world.ledge_piece(from, x, margin).map(|s| s.id);
    // Celui qui passe sous l'abscisse, d'où qu'on parte.
    assert_eq!(found(left.id, 450.0, 0.0), Some(left.id));
    assert_eq!(found(left.id, 650.0, 0.0), Some(right.id));
    assert_eq!(found(right.id, 450.0, 20.0), Some(left.id));
    // Sous la fenêtre de devant : rien sans marge ; le plus proche, s'il
    // est à portée.
    assert_eq!(found(left.id, 550.0, 0.0), None);
    assert_eq!(found(left.id, 510.0, 20.0), Some(left.id));
    assert_eq!(found(left.id, 590.0, 20.0), Some(right.id));
    assert_eq!(found(left.id, 550.0, 20.0), None);
    // À égale distance des deux : celui d'où l'on part.
    assert_eq!(found(left.id, 550.0, 60.0), Some(left.id));
    assert_eq!(found(right.id, 550.0, 60.0), Some(right.id));
    // Le rebord de la fenêtre de devant est un autre rebord ; un sol n'en
    // est pas un.
    let above = world
        .surfaces()
        .iter()
        .find(|s| s.kind == SurfaceKind::Window && s.id >> 8 == 2)
        .copied()
        .unwrap();
    assert_eq!(found(above.id, 550.0, 0.0), Some(above.id));
    assert_eq!(found(above.id, 450.0, 20.0), None);
    let floor = world
        .surfaces()
        .iter()
        .find(|s| s.kind == SurfaceKind::Floor)
        .copied()
        .unwrap();
    assert_eq!(found(floor.id, 450.0, 20.0), None);
}

/// Vu à la relecture : une fenêtre de devant plus étroite qu'un cinquième
/// de l'animal sépare deux morceaux. Porté juste au-delà, il gardait le
/// morceau d'avant (resté sur place, à moins d'un cinquième de sa largeur) :
/// un animal desktopPet en tombait, un mouton y revenait d'un bond dès son
/// premier pas.
#[test]
fn a_pet_carried_just_past_a_narrow_window_in_front_stands_on_the_piece_under_it() {
    for (pet, sleep, who) in [
        (sitter(), None, "posé"),
        (scripted(), Some("sleep1b"), "scripté"),
    ] {
        let mut world = world();
        // Huit points de large : elle cache le rebord du fond de 500 à 508.
        let front = window(2, 500.0, 200.0, 8.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let mut flock = Flock::new();
        let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(480.0));
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{who} : pas posé");
        if let Some(sleep) = sleep {
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
        }
        advance(&mut flock, &world, 0.5);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who}");
        assert!(
            piece_under(&flock, &world, id).is_some_and(|(_, x1)| x1 == 500.0) && from < 500.0,
            "{who} : il devait être sur le bout de gauche (en {from})"
        );
        flock.get_mut(id).unwrap().take_events();
        // D'un seul coup, la fenêtre du fond l'emmène en 511 : au-dessus du
        // bout de droite (qui commence en 508), à 11 points du bout de
        // gauche, resté en 500.
        let shift = 511.0 - from;
        let moved = window(1, 100.0 + shift, 300.0, 800.0, 300.0);
        world.set_windows(&[front, moved], LedgeOptions::default());
        for step in 0..120 {
            flock.advance(TICK, &world, &input);
            let (on, x, y) = place(&flock, &world, id);
            let landed = flock
                .get_mut(id)
                .unwrap()
                .take_events()
                .iter()
                .any(|e| matches!(e, PetEvent::Landed { .. }));
            assert!(
                on == Some(1) && y == 300.0 && !landed,
                "{who} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
            );
            let piece = piece_under(&flock, &world, id);
            assert!(
                piece.is_some_and(|(x0, x1)| x0 <= x && x <= x1),
                "{who} : au pas {step}, en {x}, il tient au morceau {piece:?}, qui n'est pas sous lui"
            );
        }
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - 511.0).abs() < 0.5,
            "{who} : il n'a pas suivi sa fenêtre (de {from} à {x})"
        );
    }
}

/// Une fenêtre s'ouvre devant et couvre tout juste le milieu de l'animal :
/// il tient au bout de rebord qu'on voit encore, à moins d'un cinquième de
/// sa largeur, que ce bout ait gardé le numéro de son morceau ou non. Avant,
/// du côté de l'autre morceau, il décollait un pas et se reposait (à moins
/// de 0,15 largeur), ou tombait pour de bon (de 0,15 à 0,2).
#[test]
fn a_window_opening_over_the_middle_of_a_sheep_leaves_it_on_the_end_still_showing() {
    for realistic in [false, true] {
        let what = if realistic { "réaliste" } else { "classique" };
        // (De 739 à…) Le bout de droite commence à 2,9 puis 10,9 points de
        // son milieu (57,5 de large : un cinquième fait 11,5) ; puis le bout
        // de gauche finit à 2,9 et 10,9 points.
        for (x0, w, side) in [
            (739.0, 83.9, "à droite, tout près"),
            (739.0, 91.9, "à droite, à un cinquième"),
            (822.9, 300.0, "à gauche, tout près"),
            (830.9, 300.0, "à gauche, à un cinquième"),
        ] {
            let mut world = world();
            world.set_windows(&[back()], LedgeOptions::default());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let id = flock.spawn(sitter(), 7, &world, Some(820.0));
            advance(&mut flock, &world, 3.0);
            assert_eq!(place(&flock, &world, id), (Some(1), 820.0, 300.0), "posé");
            let front = if side.starts_with("à droite") {
                window(2, x0, 200.0, w, 400.0)
            } else {
                // (Elle couvre la droite du rebord : de 817,1 ou 809,1 au bout.)
                window(2, 1640.0 - x0, 200.0, w, 400.0)
            };
            world.set_windows(&[front, back()], LedgeOptions::default());
            still(&mut flock, &world, 1.0, &[id], &format!("{what}, {side}"));
            assert_eq!(
                place(&flock, &world, id),
                (Some(1), 820.0, 300.0),
                "{what}, {side}"
            );
        }
        // Plus loin que cela, il n'a plus rien sous lui : il tombe.
        let mut world = world();
        world.set_windows(&[back()], LedgeOptions::default());
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(sitter(), 7, &world, Some(820.0));
        advance(&mut flock, &world, 3.0);
        let front = window(2, 739.0, 200.0, 96.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        advance(&mut flock, &world, 2.0);
        assert_eq!(
            place(&flock, &world, id).0,
            None,
            "{what} : à 15 points du bout visible, il devait tomber"
        );
    }
}

/// Un tirage au sort qui se rejoue : les essais au hasard disent la graine
/// de ce qu'ils trouvent.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * ((self.next() % 1_000_000) as f32 / 1_000_000.0)
    }
}

/// Des moutons posés sur une fenêtre qui ne bouge pas, pendant que des
/// fenêtres de toutes largeurs s'ouvrent et se ferment devant elle et que
/// la physique change : aucun ne bouge ni ne bronche tant qu'il reste un
/// bout de rebord à moins d'un cinquième de sa largeur, et aucun n'en tombe
/// tant qu'il en reste un.
fn fronts_that_come_and_go(seeds: std::ops::RangeInclusive<u64>) {
    let reach = 57.5 * 0.2;
    let near = |world: &World, x: f32| {
        world.surfaces().iter().any(|s| {
            s.kind == SurfaceKind::Window && s.id >> 8 == 1 && s.contains_x(x, reach - 0.01)
        })
    };
    let mut checked = 0u64;
    for seed in seeds {
        let mut rng = Lcg(seed * 104_729);
        let mut world = world();
        let back = window(1, 200.0, 300.0, 1000.0, 300.0);
        world.set_windows(&[back], LedgeOptions::default());
        let mut flock = Flock::new();
        let mut realistic = seed % 2 == 0;
        flock.set_realistic(realistic);
        let input = Input::default();
        let mut pets: Vec<(u32, f32)> = (0..7u64)
            .map(|k| {
                let x = 260.0 + 140.0 * k as f32;
                (flock.spawn(sitter(), seed + k, &world, Some(x)), x)
            })
            .collect();
        for _ in 0..400 {
            flock.advance(TICK, &world, &input);
        }
        for &(id, x) in &pets {
            assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0), "posé");
        }
        for round in 0..300 {
            match rng.next() % 6 {
                0 | 1 => {
                    let mut all = Vec::new();
                    for k in 0..(rng.next() % 4) {
                        all.push(window(
                            10 + u64::from(k),
                            rng.range(100.0, 1200.0),
                            200.0,
                            rng.range(4.0, 300.0),
                            400.0,
                        ));
                    }
                    all.push(back);
                    world.set_windows(&all, LedgeOptions::default());
                }
                2 => {
                    realistic = !realistic;
                    flock.set_realistic(realistic);
                }
                _ => {}
            }
            for &(id, _) in &pets {
                flock.get_mut(id).unwrap().take_events();
            }
            for _ in 0..=(rng.next() % 3) {
                flock.advance(TICK, &world, &input);
            }
            let mut kept = Vec::new();
            for (id, x) in pets.drain(..) {
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                let (on, at, _) = place(&flock, &world, id);
                let what = format!(
                    "graine {seed}, tour {round}, n° {id} parti de {x} (réaliste : {realistic})"
                );
                if on == Some(1) {
                    checked += 1;
                    assert!(
                        (at - x).abs() <= 0.01 && !landed,
                        "{what} : en {at}, atterri : {landed}"
                    );
                    kept.push((id, x));
                } else {
                    assert!(
                        !near(&world, x),
                        "{what} : tombé avec un bout de rebord à portée"
                    );
                }
            }
            pets = kept;
            if pets.is_empty() {
                break;
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn sheep_on_a_still_window_never_flinch_while_windows_in_front_come_and_go() {
    fronts_that_come_and_go(1..=200);
}

/// La version longue : `cargo test --release -p mouflon-core --test rebords
/// -- --ignored`.
#[test]
#[ignore = "long : trois mille tirages"]
fn sheep_on_a_still_window_never_flinch_whatever_comes_and_goes_in_front() {
    fronts_that_come_and_go(1..=3000);
}

/// Les mêmes moutons sur une fenêtre qu'on tire, qu'on redimensionne, et
/// devant laquelle d'autres vont et viennent, en physique classique. À
/// chaque relevé : si elle a glissé d'un bloc, chacun a glissé d'autant, là
/// où il lui reste un bout de rebord à portée (un cinquième de sa largeur) ;
/// sinon il est resté où il était, retenu par la fenêtre de devant ; si sa
/// largeur a changé, personne n'a bougé ; et l'on ne la quitte que là où il
/// n'y a plus de bout de rebord à portée. Jamais un sursaut.
fn a_window_dragged_and_resized_among_others(seeds: std::ops::RangeInclusive<u64>) {
    let reach = 57.5 * 0.2;
    // Un bout de rebord de la fenêtre du fond à portée de `x`, à `slack`
    // près (pour ne rien affirmer à la limite même).
    let near = |world: &World, x: f32, slack: f32| {
        world.surfaces().iter().any(|s| {
            s.kind == SurfaceKind::Window && s.id >> 8 == 1 && s.contains_x(x, reach + slack)
        })
    };
    let (mut rode, mut held, mut left_it) = (0u64, 0u64, 0u64);
    for seed in seeds {
        let mut rng = Lcg(seed * 7_919 + 13);
        let mut world = world();
        // Le cadre de la fenêtre du fond : sa gauche et sa largeur. Elle
        // reste loin des bords de l'écran, qui arrêteraient les moutons.
        let mut frame = (220.0f32, 900.0f32);
        let back = |frame: (f32, f32)| window(1, frame.0, 300.0, frame.1, 300.0);
        let mut fronts: Vec<WindowRect> = Vec::new();
        world.set_windows(&[back(frame)], LedgeOptions::default());
        let mut flock = Flock::new();
        let input = Input::default();
        let mut pets: Vec<(u32, f32)> = (0..6u64)
            .map(|k| {
                let x = 300.0 + 140.0 * k as f32;
                (flock.spawn(sitter(), seed + k, &world, Some(x)), x)
            })
            .collect();
        for _ in 0..400 {
            flock.advance(TICK, &world, &input);
        }
        for &(id, x) in &pets {
            assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0), "posé");
        }
        for round in 0..200 {
            let before = frame;
            match rng.next() % 8 {
                // Elle glisse d'un bloc.
                0..=3 => frame.0 = (frame.0 + rng.range(-40.0, 40.0)).clamp(140.0, 300.0),
                // On la redimensionne par la droite, puis par la gauche.
                4 => frame.1 = (frame.1 + rng.range(-60.0, 60.0)).clamp(760.0, 980.0),
                5 => {
                    let left = (frame.0 + rng.range(-30.0, 30.0)).clamp(140.0, 300.0);
                    frame.1 = (frame.1 - (left - frame.0)).clamp(760.0, 980.0);
                    frame.0 = left;
                }
                // D'autres fenêtres s'ouvrent et se ferment devant elle.
                6 => {
                    fronts.clear();
                    for k in 0..(rng.next() % 4) {
                        fronts.push(window(
                            10 + u64::from(k),
                            rng.range(100.0, 1200.0),
                            200.0,
                            rng.range(4.0, 300.0),
                            400.0,
                        ));
                    }
                }
                // Un relevé où rien n'a changé.
                _ => {}
            }
            let mut all = fronts.clone();
            all.push(back(frame));
            world.set_windows(&all, LedgeOptions::default());
            // Ce que la fenêtre fait de ses moutons à ce relevé. (« D'un
            // bloc » : sa largeur, comptée de son bord gauche à son bord
            // droit, n'a pas changé de 0,05 point.)
            let moved = frame.0 - before.0;
            let span = |frame: (f32, f32)| (frame.0 + frame.1) - frame.0;
            let block = moved != 0.0 && (span(frame) - span(before)).abs() < 0.05;
            for &(id, _) in &pets {
                flock.get_mut(id).unwrap().take_events();
            }
            for _ in 0..=(rng.next() % 3) {
                flock.advance(TICK, &world, &input);
            }
            let mut kept = Vec::new();
            for (id, x) in pets.drain(..) {
                let carried = if block { x + moved } else { x };
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                let (on, at, _) = place(&flock, &world, id);
                let what = format!(
                    "graine {seed}, tour {round}, n° {id} : de {x} vers {carried} (fenêtre de {before:?} à {frame:?})"
                );
                if on != Some(1) {
                    left_it += 1;
                    assert!(
                        !near(&world, carried, -0.01) && !near(&world, x, -0.01),
                        "{what} : il l'a quittée avec un bout de rebord à portée"
                    );
                    continue;
                }
                assert!(!landed, "{what} : il a sursauté (en {at})");
                let there = (at - carried).abs() <= 0.01;
                let stayed = (at - x).abs() <= 0.01;
                if near(&world, carried, -0.01) {
                    assert!(there, "{what} : elle ne l'a pas emporté (en {at})");
                } else if !near(&world, carried, 0.01) {
                    assert!(
                        stayed,
                        "{what} : emporté là où il n'y a pas de rebord (en {at})"
                    );
                } else {
                    assert!(there || stayed, "{what} : ni emporté ni resté (en {at})");
                }
                if block && there {
                    rode += 1;
                } else if block {
                    held += 1;
                }
                kept.push((id, at));
            }
            pets = kept;
            if pets.is_empty() {
                break;
            }
        }
    }
    // (L'essai a bien vu des moutons emportés, d'autres retenus au bord
    // d'une fenêtre de devant, d'autres quitter la fenêtre.)
    assert!(
        rode > 1000 && held > 10 && left_it > 10,
        "{rode} emportés, {held} retenus, {left_it} partis"
    );
}

#[test]
fn sheep_ride_a_window_dragged_and_resized_while_others_come_and_go_in_front() {
    a_window_dragged_and_resized_among_others(1..=200);
}

/// La version longue des deux tirages : `cargo test --release -p mouflon-core
/// --test rebords -- --ignored`.
#[test]
#[ignore = "long : trois mille tirages"]
fn sheep_ride_a_window_dragged_and_resized_whatever_comes_and_goes_in_front() {
    a_window_dragged_and_resized_among_others(1..=3000);
}

/// Sa fenêtre glisse sous une autre : elle l'emporte tant qu'il lui reste un
/// bout de rebord où tenir, puis la fenêtre de devant le retient à son bord,
/// sans qu'il tombe ni ne sursaute. (Avant, il la suivait dessous et tombait
/// de là ; ou, son morceau ayant changé de numéro, il décollait et se
/// reposait sur place.) Elle repart : il repart avec elle.
#[test]
fn a_pet_whose_window_slides_under_another_is_held_at_its_edge() {
    // Ce qu'il faut à chacun pour tenir : un mouton, jusqu'à un cinquième
    // de sa largeur (57,5) au-delà du bout ; un animal desktopPet, le bout
    // sous son centre.
    for (pet, sleep, reach, who) in [
        (sitter(), None, 11.5, "posé"),
        (scripted(), Some("sleep1b"), 0.0, "scripté"),
    ] {
        let mut world = world();
        // La fenêtre de devant cache le rebord du fond à partir de 500.
        let front = window(2, 500.0, 200.0, 600.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let mut flock = Flock::new();
        let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(470.0));
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{who} : pas posé");
        if let Some(sleep) = sleep {
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
        }
        advance(&mut flock, &world, 0.5);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who}");
        assert!(from > 440.0 && from < 500.0, "{who} : en {from}");
        flock.get_mut(id).unwrap().take_events();
        let calm = |flock: &mut Flock, world: &World, what: &str| {
            for step in 0..4 {
                flock.advance(TICK, world, &input);
                let (on, x, y) = place(flock, world, id);
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                assert!(
                    on == Some(1) && y == 300.0 && !landed,
                    "{who}, {what} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
                );
            }
        };
        // Elle glisse de 120 points vers la droite, 10 par relevé : son
        // rebord visible finit toujours en 500.
        for k in 1..=12 {
            let moved = window(1, 100.0 + 10.0 * k as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[front, moved], LedgeOptions::default());
            calm(&mut flock, &world, &format!("relevé {k}"));
        }
        let (_, held, _) = place(&flock, &world, id);
        assert!(
            held <= 500.0 + reach + 0.01 && held > 500.0 + reach - 10.0 - 0.01,
            "{who} : il devait suivre jusqu'au bord et y rester (de {from} à {held}, bord en 500, portée {reach})"
        );
        // Elle repart de 50 points vers la gauche : il repart avec elle.
        for k in 1..=5 {
            let moved = window(1, 220.0 - 10.0 * k as f32, 300.0, 800.0, 300.0);
            world.set_windows(&[front, moved], LedgeOptions::default());
            calm(&mut flock, &world, &format!("retour {k}"));
        }
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - (held - 50.0)).abs() < 0.01,
            "{who} : il n'est pas reparti avec elle (de {held} à {x})"
        );
    }
}

/// Vu à la relecture : poussée d'un seul coup à cheval sur le bord de
/// l'écran, la fenêtre emmenait son mouton hors de l'écran ; comme il n'y a
/// pas de rebord là-bas, elle le laissait où il était, sans plus rien sous
/// lui, et il tombait, alors qu'on voyait encore un bout de son rebord.
/// Elle ne l'emmène pas plus loin que le bord de l'écran : à une
/// demi-largeur du bord pour un mouton (là où il s'arrête en marchant), au
/// bord même pour un animal desktopPet (ses règles le laissent à demi sorti).
#[test]
fn a_window_pushed_half_off_the_screen_in_one_go_leaves_its_pets_at_the_edge() {
    for (pet, sleep, who) in [
        (sitter(), None, "posé"),
        (scripted(), Some("sleep1b"), "scripté"),
    ] {
        let inset = if sleep.is_some() {
            0.0
        } else {
            pet.physics.width * 0.5
        };
        // Vers la droite (l'écran finit en 1440), puis vers la gauche.
        for (start, spot, pushed, edge) in [
            (1000.0, 1200.0, 1300.0, 1440.0 - inset),
            (100.0, 300.0, -200.0, inset),
        ] {
            let what = format!("{who}, fenêtre de {start} à {pushed}");
            let mut world = world();
            world.set_windows(
                &[window(1, start, 300.0, 500.0, 300.0)],
                LedgeOptions::default(),
            );
            let mut flock = Flock::new();
            let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(spot));
            let input = Input::default();
            for _ in 0..600 {
                flock.advance(1.0 / 60.0, &world, &input);
                if place(&flock, &world, id).0 == Some(1) {
                    break;
                }
            }
            assert_eq!(place(&flock, &world, id).0, Some(1), "{what} : pas posé");
            if let Some(sleep) = sleep {
                assert!(flock.get_mut(id).unwrap().force(sleep), "{what}");
            }
            advance(&mut flock, &world, 0.5);
            let (on, from, _) = place(&flock, &world, id);
            assert_eq!(on, Some(1), "{what}");
            flock.get_mut(id).unwrap().take_events();
            let calm = |flock: &mut Flock, world: &World, when: &str| {
                for step in 0..60 {
                    flock.advance(TICK, world, &input);
                    let (on, x, y) = place(flock, world, id);
                    let landed = flock
                        .get_mut(id)
                        .unwrap()
                        .take_events()
                        .iter()
                        .any(|e| matches!(e, PetEvent::Landed { .. }));
                    assert!(
                        on == Some(1) && y == 300.0 && !landed,
                        "{what}, {when} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
                    );
                    let piece = piece_under(flock, world, id);
                    assert!(
                        piece.is_some_and(|(x0, x1)| x0 <= x && x <= x1),
                        "{what}, {when} : en {x}, il tient au morceau {piece:?}, qui n'est pas sous lui"
                    );
                }
            };
            // D'un seul relevé, 300 points vers le bord : on ne voit plus de
            // son rebord qu'un bout, contre le bord de l'écran.
            world.set_windows(
                &[window(1, pushed, 300.0, 500.0, 300.0)],
                LedgeOptions::default(),
            );
            calm(&mut flock, &world, "poussée");
            let (_, x, _) = place(&flock, &world, id);
            assert!(
                (x - edge).abs() < 0.01,
                "{what} : il devait rester au bord de l'écran, en {edge} (de {from} à {x})"
            );
            // Elle revient d'un coup : il revient avec elle.
            world.set_windows(
                &[window(1, start, 300.0, 500.0, 300.0)],
                LedgeOptions::default(),
            );
            calm(&mut flock, &world, "revenue");
            let (_, back, _) = place(&flock, &world, id);
            assert!(
                (back - (x + start - pushed)).abs() < 0.01,
                "{what} : il n'est pas revenu avec elle (de {x} à {back})"
            );
        }
    }
}

/// Vu à la relecture : un animal desktopPet à demi sorti de l'écran, sur
/// une fenêtre qui en dépasse aussi. Qu'elle bouge d'un point, il bouge
/// d'un point avec elle : le bord de l'écran ne le ramène pas d'un coup à
/// une demi-largeur vers l'intérieur.
#[test]
fn a_scripted_pet_half_off_the_screen_just_rides_its_window() {
    let pet = scripted();
    let mut world = world();
    // Elle dépasse à gauche de l'écran : on voit son rebord de 0 à 400.
    world.set_windows(
        &[window(1, -200.0, 300.0, 600.0, 300.0)],
        LedgeOptions::default(),
    );
    let mut flock = Flock::new();
    let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(12.0));
    let input = Input::default();
    for _ in 0..600 {
        flock.advance(1.0 / 60.0, &world, &input);
        if place(&flock, &world, id).0 == Some(1) {
            break;
        }
    }
    assert_eq!(place(&flock, &world, id).0, Some(1), "pas posée");
    assert!(flock.get_mut(id).unwrap().force("sleep1b"));
    advance(&mut flock, &world, 0.5);
    let (on, from, _) = place(&flock, &world, id);
    assert!(
        on == Some(1) && from < pet.physics.width * 0.5,
        "elle devait être à demi sortie de l'écran (en {from}, sur {on:?})"
    );
    // Un point vers l'intérieur, puis un point vers le bord.
    for (left, wanted) in [(-199.0, from + 1.0), (-200.0, from)] {
        world.set_windows(
            &[window(1, left, 300.0, 600.0, 300.0)],
            LedgeOptions::default(),
        );
        advance(&mut flock, &world, 0.25);
        let (on, x, _) = place(&flock, &world, id);
        assert!(
            on == Some(1) && (x - wanted).abs() < 0.01,
            "fenêtre en {left} : elle devait être en {wanted}, sur sa fenêtre (en {x}, sur {on:?})"
        );
    }
}

/// Vu à la relecture (et déjà là avant la 1.0.6) : une fenêtre s'ouvre
/// devant et couvre tout le bout de rebord où marche un mouton ; son numéro
/// de morceau désigne maintenant un bout resté visible, loin de là. Il y
/// était posé d'un bond (427 points, par-dessus les fenêtres de devant) dès
/// qu'il marchait en s'en éloignant ; à reculons, il restait plus d'une
/// seconde posé sur rien. Il n'a plus rien sous lui : il tombe d'où il est.
#[test]
fn a_walker_whose_piece_number_now_names_a_far_one_falls_where_it_stands() {
    let bleu = {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../pets/bleu-du-maine/pet.toml");
        Arc::new(load_pet(&path).unwrap())
    };
    for realistic in [false, true] {
        // Il marche vers la gauche, ou recule vers la gauche (tourné à droite).
        for (behavior, facing) in [("walk", -1.0), ("back_up", 1.0)] {
            let what = format!(
                "{behavior}, {}",
                if realistic { "réaliste" } else { "classique" }
            );
            let mut world = world();
            // Devant la fenêtre du fond (rebord de 100 à 900), une fenêtre
            // de 300 à 600 : on en voit deux bouts, de 100 à 300 et de 600
            // à 900.
            let front = window(2, 300.0, 200.0, 300.0, 400.0);
            world.set_windows(&[front, back()], LedgeOptions::default());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let id = flock.spawn(Arc::clone(&bleu), 7, &world, Some(230.0));
            let input = Input::default();
            for _ in 0..900 {
                flock.advance(1.0 / 60.0, &world, &input);
                if place(&flock, &world, id).0 == Some(1) {
                    break;
                }
            }
            assert_eq!(place(&flock, &world, id).0, Some(1), "{what} : pas posé");
            let pet = flock.get_mut(id).unwrap();
            pet.face(facing);
            assert!(pet.force(behavior), "{what}");
            for _ in 0..12 {
                flock.advance(TICK, &world, &input);
            }
            let (on, from, _) = place(&flock, &world, id);
            let width = bleu.physics.width;
            assert!(
                on == Some(1) && from > 100.0 + width && from < 300.0,
                "{what} : il devait marcher sur le bout de gauche (en {from}, sur {on:?})"
            );
            // Une fenêtre s'ouvre devant, de 50 à 320 : le bout de gauche
            // est caché, il ne reste que celui de 600 à 900, qui prend son
            // numéro.
            let cover = window(3, 50.0, 200.0, 270.0, 400.0);
            world.set_windows(&[cover, front, back()], LedgeOptions::default());
            for step in 0..240 {
                flock.advance(TICK, &world, &input);
                let (on, x, _) = place(&flock, &world, id);
                assert!(
                    on != Some(1),
                    "{what} : au pas {step}, posé sur la fenêtre du fond en {x}, alors que son bout est caché (il était en {from})"
                );
                assert!(
                    (x - from).abs() < width,
                    "{what} : au pas {step}, un bond de côté de {from} à {x}"
                );
                if step == 3 {
                    let pet = flock.pets().iter().find(|p| p.id() == id).unwrap();
                    assert!(
                        matches!(pet.support(), Support::Air),
                        "{what} : au pas {step}, il devait déjà tomber ({:?})",
                        pet.support()
                    );
                }
            }
        }
    }
}

/// Vu à la relecture, en physique réaliste (le réglage d'origine) : la
/// plateforme emporte l'animal après qu'il a cherché son morceau. Quand sa
/// fenêtre, à moitié cachée, part d'un coup vers son côté caché, le bout
/// qu'on voit s'écarte de lui avant qu'elle l'ait emporté : il ne faut pas
/// le lâcher pour autant (une correction de la veille le faisait tomber).
#[test]
fn a_half_hidden_window_that_starts_moving_keeps_its_pets_in_realistic_physics() {
    for (pet, sleep, who) in [
        (sitter(), None, "posé"),
        (scripted(), Some("sleep1b"), "scripté"),
    ] {
        let mut world = world();
        // La fenêtre de devant cache le rebord du fond à partir de 500.
        let front = window(2, 500.0, 200.0, 600.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let mut flock = Flock::new();
        flock.set_realistic(true);
        // Près du bout libre, à gauche.
        let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(125.0));
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{who} : pas posé");
        if let Some(sleep) = sleep {
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
        }
        advance(&mut flock, &world, 1.0);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who}");
        assert!(
            from < 160.0,
            "{who} : il devait être près du bout libre (en {from})"
        );
        flock.get_mut(id).unwrap().take_events();
        let calm = |flock: &mut Flock, world: &World, steps: usize, when: &str| {
            for step in 0..steps {
                flock.advance(TICK, world, &input);
                let (on, x, y) = place(flock, world, id);
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                assert!(
                    on == Some(1) && y == 300.0 && !landed,
                    "{who}, {when} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
                );
            }
        };
        // Après un calme, un relevé la montre 60 points plus à droite : son
        // bout visible commence maintenant en 160, plus loin que lui.
        world.set_windows(
            &[front, window(1, 160.0, 300.0, 800.0, 300.0)],
            LedgeOptions::default(),
        );
        calm(&mut flock, &world, 120, "partie d'un coup");
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - (from + 60.0)).abs() < 0.5,
            "{who} : il n'a pas suivi sa fenêtre (de {from} à {x})"
        );
        // Puis on la tire à 400 points par seconde, un relevé toutes les
        // 33 ms : il la suit.
        let mut left = 160.0;
        for _ in 0..12 {
            left += 400.0 / 30.0;
            world.set_windows(
                &[front, window(1, left, 300.0, 800.0, 300.0)],
                LedgeOptions::default(),
            );
            calm(&mut flock, &world, 4, "tirée");
        }
        calm(&mut flock, &world, 120, "arrêtée");
        // (En physique réaliste, ses pattes glissent quand elle s'arrête
        // net, de 3 points pour le mouton, de 23 pour l'animal desktopPet,
        // avant comme après la 1.0.6 : il est à peu près là où elle l'a
        // mené, à moins d'une demi-largeur.)
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - (from + left - 100.0)).abs() < pet.physics.width * 0.5,
            "{who} : il n'a pas suivi sa fenêtre qu'on tirait (de {from} à {x}, elle a avancé de {})",
            left - 100.0
        );
    }
}

/// De même quand c'est le bord de l'écran qui coupe son rebord.
#[test]
fn a_window_cut_by_the_screen_edge_keeps_its_sheep_in_realistic_physics() {
    let mut world = world();
    // L'écran finit en 1440 : on voit son rebord de 1000 à 1440.
    world.set_windows(
        &[window(1, 1000.0, 300.0, 500.0, 300.0)],
        LedgeOptions::default(),
    );
    let mut flock = Flock::new();
    flock.set_realistic(true);
    let pets = [1030.0, 1045.0].map(|x| flock.spawn(sitter(), 7, &world, Some(x)));
    advance(&mut flock, &world, 3.0);
    for (id, x) in pets.into_iter().zip([1030.0, 1045.0]) {
        assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0), "posé");
    }
    // D'un relevé, 60 points vers le bord : son rebord va de 1060 à 1440.
    world.set_windows(
        &[window(1, 1060.0, 300.0, 500.0, 300.0)],
        LedgeOptions::default(),
    );
    still(&mut flock, &world, 1.0, &pets, "réaliste, bord de l'écran");
    for (id, x) in pets.into_iter().zip([1090.0, 1105.0]) {
        assert_eq!(place(&flock, &world, id), (Some(1), x, 300.0));
    }
}

/// Vu à la relecture du 27 novembre au soir : en physique réaliste, une
/// fenêtre déplacée d'un coup là où son animal ne tiendrait plus (sous une
/// fenêtre de devant) l'emportait quand même, et il en tombait. Comme en
/// physique classique, elle ne l'emporte pas : il reste sur le bout qu'on
/// voit encore sous lui. Elle revient d'un coup : il repart avec elle.
#[test]
fn a_window_moved_in_one_go_under_another_leaves_its_pets_in_realistic_physics() {
    for (pet, sleep, who) in [
        (sitter(), None, "posé"),
        (scripted(), Some("sleep1b"), "scripté"),
    ] {
        let mut world = world();
        // La fenêtre de devant cache le rebord du fond à partir de 500.
        let front = window(2, 500.0, 200.0, 600.0, 400.0);
        world.set_windows(&[front, back()], LedgeOptions::default());
        let mut flock = Flock::new();
        flock.set_realistic(true);
        let id = flock.spawn(Arc::clone(&pet), 7, &world, Some(440.0));
        let input = Input::default();
        for _ in 0..600 {
            flock.advance(1.0 / 60.0, &world, &input);
            if place(&flock, &world, id).0 == Some(1) {
                break;
            }
        }
        assert_eq!(place(&flock, &world, id).0, Some(1), "{who} : pas posé");
        if let Some(sleep) = sleep {
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
        }
        advance(&mut flock, &world, 1.0);
        let (on, from, _) = place(&flock, &world, id);
        assert_eq!(on, Some(1), "{who}");
        assert!(from > 400.0 && from < 480.0, "{who} : en {from}");
        flock.get_mut(id).unwrap().take_events();
        let calm = |flock: &mut Flock, world: &World, steps: usize, when: &str| {
            for step in 0..steps {
                flock.advance(TICK, world, &input);
                let (on, x, y) = place(flock, world, id);
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                assert!(
                    on == Some(1) && y == 300.0 && !landed,
                    "{who}, {when} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, atterri : {landed})"
                );
            }
        };
        // D'un coup, 100 points vers la droite : là où elle l'emmènerait,
        // son rebord est caché ; sous lui, on le voit encore (de 200 à 500).
        world.set_windows(
            &[front, window(1, 200.0, 300.0, 800.0, 300.0)],
            LedgeOptions::default(),
        );
        calm(&mut flock, &world, 120, "poussée sous l'autre");
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - from).abs() < 0.5,
            "{who} : il devait rester où il était (de {from} à {x})"
        );
        // Elle revient d'un coup : il repart avec elle. (Regardé tout de
        // suite : l'animal desktopPet finit par se réveiller et marcher.)
        world.set_windows(&[front, back()], LedgeOptions::default());
        calm(&mut flock, &world, 12, "revenue");
        let (_, x, _) = place(&flock, &world, id);
        assert!(
            (x - (from - 100.0)).abs() < 0.5,
            "{who} : il n'est pas reparti avec elle (de {from} à {x})"
        );
        calm(&mut flock, &world, 108, "revenue, ensuite");
    }
}

/// Vu à la relecture du 27 novembre, tard le soir : une fenêtre déplacée
/// d'un coup emmène un animal scripté juste au-delà du bout d'un autre
/// morceau de son rebord, sous une fenêtre de devant (à moins d'un
/// cinquième de sa largeur de ce bout, mais plus loin que ce qu'il lui faut
/// pour tenir). Il ne tient pas là-bas : il reste où il était, sur le bout
/// qu'on voit encore sous lui. En physique réaliste, il gardait le morceau
/// d'arrivée sans être dessus : l'animal desktopPet tombait à travers son
/// bout de rebord, l'animal en bandes d'images sursautait.
#[test]
fn a_scripted_pet_held_back_by_a_window_moved_in_one_go_stays_on_what_shows_under_it() {
    let lainou = || {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/donnees/lainou/pet.toml");
        Arc::new(load_pet(&path).unwrap())
    };
    let mut failures = Vec::new();
    for (realistic, physics) in [(false, "classique"), (true, "réaliste")] {
        for (pet, sleep, dest, who) in [
            (scripted(), "sleep1b", 504.0, "desktopPet"),
            (scripted(), "sleep1b", 510.0, "desktopPet"),
            (scripted(), "sleep1b", 515.0, "desktopPet"),
            (lainou(), "dort", 513.0, "bandes d'images"),
        ] {
            let mut world = world();
            // Le rebord du fond, de 100 à 900 ; la fenêtre de devant le
            // cache de 500 à 600 : deux bouts, 100..500 et 600..900.
            let front = window(3, 500.0, 200.0, 100.0, 400.0);
            world.set_windows(&[front, back()], LedgeOptions::default());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let id = flock.spawn(pet, 7, &world, Some(700.0));
            let input = Input::default();
            for _ in 0..600 {
                flock.advance(1.0 / 60.0, &world, &input);
                if place(&flock, &world, id).0 == Some(1) {
                    break;
                }
            }
            assert!(flock.get_mut(id).unwrap().force(sleep), "{who}");
            for _ in 0..120 {
                flock.advance(TICK, &world, &input);
            }
            let (on, from, _) = place(&flock, &world, id);
            assert_eq!(on, Some(1), "{physics}, {who} : pas posé");
            flock.get_mut(id).unwrap().take_events();
            // D'un coup, vers la gauche : là où elle l'emmènerait (`dest`),
            // il est sous la fenêtre de devant, juste au-delà du bout
            // 100..500 ; sous lui, on voit encore le bout de droite.
            let dx = dest - from;
            world.set_windows(
                &[front, window(1, 100.0 + dx, 300.0, 800.0, 300.0)],
                LedgeOptions::default(),
            );
            let what = format!("{physics}, {who}, emmené vers {dest}");
            let mut failed = None;
            for step in 0..120 {
                flock.advance(TICK, &world, &input);
                let (on, x, y) = place(&flock, &world, id);
                let landed = flock
                    .get_mut(id)
                    .unwrap()
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Landed { .. }));
                // (Et le morceau qui le porte est bien sous lui.)
                let under = piece_under(&flock, &world, id);
                let held = under.is_some_and(|(x0, x1)| x0 <= x && x <= x1);
                if !(on == Some(1) && y == 300.0 && !landed && held) {
                    failed = Some(format!(
                        "{what} : il a bronché au pas {step} (en {x}, {y}, sur {on:?}, morceau {under:?}, atterri : {landed})"
                    ));
                    break;
                }
            }
            let (_, x, _) = place(&flock, &world, id);
            if failed.is_none() && (x - from).abs() > 0.5 {
                failed = Some(format!("{what} : il a bougé de {from} à {x}"));
            }
            failures.extend(failed);
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Vu à la relecture : posé sur le dos d'un autre (mode collisions), sur
/// une fenêtre qu'on tire, quand la physique réaliste est décochée. Un dos
/// ne porte qu'en physique réaliste : il tombe, et garde l'élan de ce qui
/// le portait.
#[test]
fn a_sheep_on_a_back_keeps_its_momentum_when_realistic_physics_is_turned_off() {
    let (mut flock, mut world, pets) = three_on_the_ledge(true);
    let input = Input::default();
    let below = pets[1];
    // (Les dos sont solides en mode collisions.)
    flock.set_collisions(true);
    let rider = flock.spawn(sitter(), 11, &world, Some(500.0));
    advance(&mut flock, &world, 3.0);
    let on_the_back = |flock: &Flock| {
        flock
            .pets()
            .iter()
            .find(|p| p.id() == rider)
            .is_some_and(|p| match p.support() {
                Support::Ground { surface } => {
                    mouflon_core::physics::surface_pet(surface) == Some(below)
                }
                _ => false,
            })
    };
    assert!(
        on_the_back(&flock),
        "il devait se poser sur le dos de l'autre"
    );
    // On tire la fenêtre de plus en plus vite, jusqu'à 600 points par
    // seconde, puis à cette vitesse.
    let mut at = 100.0;
    for step in 1..=180u32 {
        at += 5.0 * (step as f32 / 60.0).min(1.0);
        world.set_windows(
            &[window(1, at, 300.0, 800.0, 300.0)],
            LedgeOptions::default(),
        );
        flock.advance(TICK, &world, &input);
    }
    assert!(
        on_the_back(&flock),
        "il a quitté le dos pendant qu'on tirait"
    );
    flock.set_realistic(false);
    at += 5.0;
    world.set_windows(
        &[window(1, at, 300.0, 800.0, 300.0)],
        LedgeOptions::default(),
    );
    flock.advance(TICK, &world, &input);
    let pet = flock.get_mut(rider).unwrap();
    assert!(
        matches!(pet.support(), Support::Air),
        "sans dos pour le porter, il devait tomber : {:?}",
        pet.support()
    );
    let speed = pet.velocity().x;
    assert!(
        speed > 400.0,
        "il tombe sans l'élan de ce qui le portait : {speed} pt/s"
    );
}
