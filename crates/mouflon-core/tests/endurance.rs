//! Essai d'endurance du cœur : toutes les races du dépôt (béliers, brebis,
//! agneaux), des écrans et des fenêtres qui changent sans prévenir, un
//! utilisateur brouillon (il attrape, lance, touche, part, revient, change
//! les réglages), du vent, des soucoupes. Quoi qu'il arrive, aucun animal
//! ne prend une place qui n'est pas un nombre, ne quitte le monde, ne reste
//! en l'air sans fin, ni ne garde pour appui une surface qui n'existe plus.
//!
//! La version courte tourne avec les autres essais ; la longue se lance à
//! la main :
//!
//! ```text
//! cargo test --release -p mouflon-core --test endurance -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Arc;

use mouflon_core::math::Rng;
use mouflon_core::{
    Flock, Input, LedgeOptions, Mood, PetDef, Rect, Screen, Sex, Support, TICK, Vec2, WindowRect,
    World, load_pet,
};

/// Toutes les races du dépôt.
fn breeds() -> Vec<Arc<PetDef>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .expect("pets/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("pet.toml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect()
}

/// Un ou deux écrans, de tailles au hasard (du petit portable au grand
/// écran, côte à côte).
fn screens(rng: &mut Rng) -> Vec<Screen> {
    let mut out = Vec::new();
    let mut x = 0.0;
    for _ in 0..if rng.chance(0.4) { 2 } else { 1 } {
        let (w, h) = (
            rng.range(640.0, 2600.0).round(),
            rng.range(480.0, 1500.0).round(),
        );
        let (menu, dock) = (rng.range(0.0, 40.0).round(), rng.range(0.0, 90.0).round());
        out.push(Screen {
            frame: Rect::new(x, 0.0, w, h),
            usable: Rect::new(x, menu, w, h - menu - dock),
            scale: if rng.chance(0.5) { 2.0 } else { 1.0 },
        });
        x += w;
    }
    out
}

/// Des fenêtres au hasard dans le monde (certaines débordent, certaines
/// sont minuscules ou plus grandes qu'un écran).
fn windows(rng: &mut Rng, arena: Rect, next_id: &mut u64) -> Vec<WindowRect> {
    (0..(rng.next_u32() % 9))
        .map(|_| {
            *next_id += 1;
            let w = rng.range(20.0, arena.w * 0.9);
            let h = rng.range(20.0, arena.h * 0.9);
            WindowRect {
                id: *next_id,
                frame: Rect::new(
                    rng.range(arena.x - 100.0, arena.x + arena.w - 20.0),
                    rng.range(arena.y - 40.0, arena.y + arena.h - 20.0),
                    w,
                    h,
                ),
            }
        })
        .collect()
}

/// Ce qu'on relève d'une vie.
#[derive(Default)]
struct Life {
    /// Ce qui ne devrait jamais arriver, dans l'ordre.
    faults: Vec<String>,
    steps: usize,
    pets: usize,
}

/// Une vie de `seconds` secondes simulées, tirée de `seed`.
fn live(defs: &[Arc<PetDef>], seed: u64, seconds: f32) -> Life {
    // Pour chercher : `MOUFLON_TRACE=graine,animal,pas` suit cet animal
    // autour de ce pas.
    let trace = std::env::var("MOUFLON_TRACE").ok().and_then(|v| {
        let mut parts = v.split(',').map(|x| x.trim().parse::<u64>().ok());
        let (s, pet, step) = (parts.next()??, parts.next()??, parts.next()??);
        (s == seed).then_some((pet as u32, step as usize))
    });
    let mut rng = Rng::new(seed);
    let mut world = World::new(screens(&mut rng));
    let mut next_window = 100u64;
    let mut shown: Vec<WindowRect> = Vec::new();
    let mut flock = Flock::new();
    flock.set_realistic(rng.chance(0.7));
    flock.set_collisions(rng.chance(0.4));
    let mut input = Input {
        cursor: Some(Vec2::new(300.0, 300.0)),
        local_hour: 15.0,
        since_key: None,
        idle: Some(1.0),
    };
    let mut life = Life::default();
    // Depuis combien de pas chacun est en l'air, sur une surface disparue,
    // hors du monde.
    let mut airborne: std::collections::HashMap<u32, (usize, usize, usize)> =
        std::collections::HashMap::new();
    // L'animal tenu, et jusqu'à quel pas.
    let mut held: Option<(u32, usize, Vec2)> = None;
    let mut next_action = 0usize;
    let total = (seconds / TICK) as usize;

    let spawn = |flock: &mut Flock, world: &World, rng: &mut Rng| {
        let def = &defs[rng.next_u32() as usize % defs.len()];
        let arena = world.arena();
        let x = rng
            .chance(0.8)
            .then(|| rng.range(arena.x, arena.x + arena.w));
        let sex = match rng.next_u32() % 3 {
            0 => Some(Sex::Male),
            1 => Some(Sex::Female),
            _ => None,
        };
        let id = flock.spawn_as(Arc::clone(def), u64::from(rng.next_u32()), world, x, sex);
        // Une brebis vient parfois avec son agneau.
        if rng.chance(0.35) {
            let weeks = rng.range(0.5, 14.0);
            flock.spawn_lamb(id, u64::from(rng.next_u32()), weeks, world);
        }
    };
    for _ in 0..(8 + rng.next_u32() % 30) {
        spawn(&mut flock, &world, &mut rng);
    }

    for step in 0..total {
        // De temps en temps, quelque chose change.
        if step >= next_action {
            next_action = step + (rng.range(0.05, 2.5) / TICK) as usize;
            let arena = world.arena();
            let ids: Vec<u32> = flock.pets().iter().map(mouflon_core::Pet::id).collect();
            let pick = |rng: &mut Rng| ids.get(rng.next_u32() as usize % ids.len().max(1)).copied();
            match rng.next_u32() % 22 {
                0..=3 => {
                    shown = windows(&mut rng, arena, &mut next_window);
                    world.set_windows(&shown, LedgeOptions::default());
                }
                4 => {
                    // L'écran change (résolution, écran branché ou retiré) ;
                    // comme dans l'app, le relevé des fenêtres suit aussitôt
                    // (les mêmes, où qu'elles soient : leurs rebords sont
                    // refaits pour les nouveaux écrans).
                    world.set_screens(screens(&mut rng));
                    world.set_windows(&shown, LedgeOptions::default());
                }
                5..=7 => {
                    // Le curseur file ailleurs.
                    input.cursor = Some(Vec2::new(
                        rng.range(arena.x - 50.0, arena.x + arena.w + 50.0),
                        rng.range(arena.y - 50.0, arena.y + arena.h + 50.0),
                    ));
                    input.idle = Some(0.0);
                }
                8..=10 => {
                    // On en attrape un, pour un instant.
                    if held.is_none()
                        && let Some(id) = pick(&mut rng)
                        && let Some(pet) = flock.get_mut(id)
                    {
                        let at = pet.position() - Vec2::new(0.0, 10.0);
                        pet.grab(at, &world);
                        let until = step + (rng.range(0.05, 1.5) / TICK) as usize;
                        let drift = Vec2::new(rng.range(-900.0, 900.0), rng.range(-900.0, 600.0));
                        held = Some((id, until, drift));
                        input.cursor = Some(at);
                    }
                }
                11 => {
                    if let Some(id) = pick(&mut rng)
                        && let Some(pet) = flock.get_mut(id)
                    {
                        pet.poke(&world, &input);
                    }
                }
                12 => flock.blow(
                    if rng.chance(0.5) { 1.0 } else { -1.0 },
                    rng.range(1.0, 8.0),
                ),
                13 => {
                    flock.invite_saucer(&world, u64::from(rng.next_u32()));
                }
                14 => {
                    flock.regroup();
                }
                15 => {
                    flock.together(&world, &input);
                }
                16 => {
                    if let Some(id) = pick(&mut rng) {
                        if rng.chance(0.5) {
                            flock.leave(id, &world, &input);
                        } else {
                            flock.remove(id);
                            if held.is_some_and(|(h, ..)| h == id) {
                                held = None;
                            }
                        }
                    }
                }
                17 => {
                    if flock.pets().len() < 70 {
                        for _ in 0..(1 + rng.next_u32() % 4) {
                            spawn(&mut flock, &world, &mut rng);
                        }
                    }
                }
                18 => {
                    flock.set_realistic(rng.chance(0.6));
                    flock.set_collisions(rng.chance(0.4));
                }
                19 => {
                    flock.set_mood(match rng.next_u32() % 4 {
                        0 => Mood::Even,
                        1 => Mood::Mixed,
                        2 => Mood::Calm,
                        _ => Mood::Even,
                    });
                    // Le menu d'un animal s'ouvre, ou se ferme.
                    let still = rng.chance(0.5).then(|| pick(&mut rng)).flatten();
                    flock.hold_still(still);
                }
                20 => {
                    // La nuit tombe, ou le jour se lève ; il part, il revient.
                    input.local_hour = rng.range(0.0, 24.0);
                    input.idle = Some(if rng.chance(0.3) { 900.0 } else { 0.5 });
                    input.since_key = rng.chance(0.5).then(|| rng.range(0.0, 20.0));
                }
                _ => {
                    // Plus de curseur du tout (il est sur un autre écran).
                    if rng.chance(0.2) {
                        input.cursor = None;
                    }
                }
            }
        }
        // Celui qu'on tient suit la main, puis on le lâche (lancé).
        if let Some((id, until, drift)) = held {
            match flock.get_mut(id) {
                Some(pet) if step < until && pet.is_held() => {
                    let to = input.cursor.unwrap_or(pet.position()) + drift * TICK;
                    input.cursor = Some(to);
                    pet.drag_to(to);
                }
                Some(pet) => {
                    if pet.is_held() {
                        pet.release(&world);
                    }
                    held = None;
                }
                None => held = None,
            }
        }

        // Le temps passe : régulier le plus souvent, parfois haché.
        let elapsed = match rng.next_u32() % 40 {
            0 => 0.0,
            1 => TICK * rng.range(0.1, 14.0),
            _ => TICK,
        };
        let alpha = flock.advance(elapsed, &world, &input);
        life.steps += 1;
        life.pets = life.pets.max(flock.pets().len());

        let arena = world.arena();
        if let Some((pet_id, around)) = trace
            && step + 3000 > around
            && step < around + 6
            && (step % 240 == 0 || step + 6 > around)
            && let Some(pet) = flock.pets().iter().find(|p| p.id() == pet_id)
        {
            eprintln!(
                "  {step} : {} {} {:?} à {:?}, vitesse {:?}, monde {arena:?}, réaliste {} \
                 collisions {} enfant {} mère {:?} part {} tenu {} immobile {:?}",
                pet.def().meta.id,
                pet.behavior_name(),
                pet.support(),
                pet.position(),
                pet.velocity(),
                flock.realistic(),
                flock.collisions(),
                pet.is_child(),
                pet.mother(),
                pet.is_leaving(),
                pet.is_held(),
                flock.still(),
            );
            for saucer in flock.saucers() {
                eprintln!(
                    "      soucoupe sur n° {} à {:?}, faisceau {}",
                    saucer.target(),
                    saucer.view().pos,
                    saucer.view().beam
                );
            }
        }
        let mut finished = Vec::new();
        for pet in flock.pets() {
            let (id, name) = (pet.id(), pet.def().meta.id.as_str());
            let (pos, vel) = (pet.position(), pet.velocity());
            if !(pos.x.is_finite() && pos.y.is_finite() && vel.x.is_finite() && vel.y.is_finite()) {
                life.faults.push(format!(
                    "pas {step} : {name} n° {id} à {pos:?}, vitesse {vel:?} ({})",
                    pet.behavior_name()
                ));
                continue;
            }
            let view = pet.view(alpha);
            if !(view.ground.x.is_finite() && view.ground.y.is_finite()) {
                life.faults.push(format!(
                    "pas {step} : {name} n° {id} dessiné à {:?}",
                    view.ground
                ));
            }
            if pet.is_finished() {
                finished.push(id);
                continue;
            }
            // Celui dont le menu est ouvert attend, hors du temps : il
            // reprendra (et tombera, s'il le faut) quand le menu se fermera.
            if flock.still() == Some(id) {
                airborne.remove(&id);
                continue;
            }
            // Dans le monde, à sa propre taille près (celui qui part sort
            // par un bord ; une soucoupe l'emporte par le haut).
            let size = pet.def().physics.height.max(pet.def().physics.width);
            let margin = 3.0 * size + 400.0;
            let outside = pos.x < arena.x - margin
                || pos.x > arena.x + arena.w + margin
                || pos.y > arena.y + arena.h + margin
                || pos.y < arena.y - margin - 4000.0;
            // Jamais en l'air sans fin, ni sur une surface disparue, ni
            // hors du monde plus de trois secondes (un écran débranché : il
            // revient).
            let entry = airborne.entry(id).or_insert((0, 0, 0));
            if outside && !pet.is_leaving() && !pet.is_held() {
                entry.2 += 1;
            } else {
                entry.2 = 0;
            }
            if entry.2 == (3.0 / TICK) as usize {
                life.faults.push(format!(
                    "pas {step} : {name} n° {id} hors du monde depuis 3 s, à {pos:?} ({}), monde {arena:?}",
                    pet.behavior_name()
                ));
            }
            match pet.support() {
                Support::Air if !pet.is_leaving() => entry.0 += 1,
                _ => entry.0 = 0,
            }
            match pet.support() {
                Support::Ground { surface }
                    if world.surface(surface).is_none() && !flock.realistic() =>
                {
                    entry.1 += 1;
                }
                _ => entry.1 = 0,
            }
            if entry.0 == (45.0 / TICK) as usize {
                life.faults.push(format!(
                    "pas {step} : {name} n° {id} en l'air depuis 45 s, à {pos:?}, vitesse {vel:?} ({})",
                    pet.behavior_name()
                ));
            }
            if entry.1 == (2.0 / TICK) as usize {
                life.faults.push(format!(
                    "pas {step} : {name} n° {id} sur une surface disparue depuis 2 s, à {pos:?} ({})",
                    pet.behavior_name()
                ));
            }
        }
        // Comme l'app : ceux qui sont partis sont retirés.
        for id in finished {
            flock.remove(id);
            airborne.remove(&id);
        }
        if life.faults.len() > 20 {
            break;
        }
    }
    life
}

fn endure(seeds: std::ops::Range<u64>, seconds: f32) {
    let defs = breeds();
    assert!(defs.len() >= 28, "{} races", defs.len());
    let mut steps = 0;
    for seed in seeds {
        let life = live(&defs, seed * 7919 + 11, seconds);
        steps += life.steps;
        assert!(
            life.faults.is_empty(),
            "graine {seed} ({} animaux au plus) :\n{}",
            life.pets,
            life.faults.join("\n")
        );
    }
    eprintln!("{steps} pas sans faute");
}

#[test]
fn a_messy_minute_with_every_breed_leaves_no_pet_lost_or_broken() {
    endure(0..6, 60.0);
}

#[test]
#[ignore = "essai d'endurance long, à lancer à la main en --release"]
fn hours_of_messy_life_leave_no_pet_lost_or_broken() {
    endure(100..160, 600.0);
}
