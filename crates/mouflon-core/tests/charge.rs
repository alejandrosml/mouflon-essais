//! Essai de charge du cœur : combien coûte un pas de simulation avec 20,
//! 100, 500 et 1 000 animaux, en physique classique et réaliste, collisions
//! comprises. Lancé à la main (il prend du temps) :
//!
//! ```text
//! cargo test --release -p mouflon-core --test charge -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use mouflon_core::{
    Flock, Input, LedgeOptions, Rect, Screen, TICK, Vec2, WindowRect, World, load_pet,
};

fn world() -> World {
    let mut world = World::new(vec![
        Screen {
            frame: Rect::new(0.0, 0.0, 1728.0, 1117.0),
            usable: Rect::new(0.0, 38.0, 1728.0, 1010.0),
            scale: 2.0,
        },
        Screen {
            frame: Rect::new(1728.0, 0.0, 2560.0, 1440.0),
            usable: Rect::new(1728.0, 25.0, 2560.0, 1415.0),
            scale: 1.0,
        },
    ]);
    let windows: Vec<WindowRect> = (0..12)
        .map(|i| WindowRect {
            id: 100 + i,
            frame: Rect::new(
                80.0 + 340.0 * i as f32,
                260.0 + 60.0 * (i % 5) as f32,
                520.0,
                420.0,
            ),
        })
        .collect();
    world.set_windows(&windows, LedgeOptions::default());
    world
}

#[test]
#[ignore = "essai de charge, à lancer à la main en --release"]
fn load() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let defs: Vec<Arc<_>> = ["southdown", "merinos-d-arles", "solognote", "bleu-du-maine"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = world();
    let input = Input {
        cursor: Some(Vec2::new(800.0, 500.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    for (realistic, collisions) in [(false, false), (false, true), (true, false), (true, true)] {
        for count in [20usize, 100, 500, 1000] {
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            flock.set_collisions(collisions);
            for i in 0..count {
                let x = 20.0 + (i as f32 * 4.3) % 4200.0;
                flock.spawn(
                    Arc::clone(&defs[i % defs.len()]),
                    i as u64 * 7919 + 3,
                    &world,
                    Some(x),
                );
            }
            // Ils tombent, se posent, vivent un peu.
            for _ in 0..(3.0 / TICK) as usize {
                flock.advance(TICK, &world, &input);
            }
            let steps = (2.0 / TICK) as usize;
            let started = Instant::now();
            for _ in 0..steps {
                flock.advance(TICK, &world, &input);
            }
            let per_step = started.elapsed().as_secs_f64() * 1000.0 / steps as f64;
            println!(
                "physique {:9} collisions {:3} : {count:5} animaux, {per_step:7.3} ms par pas ({:6.1} ms par seconde)",
                if realistic { "réaliste" } else { "classique" },
                if collisions { "oui" } else { "non" },
                per_step / TICK as f64 / 1000.0 * 1000.0 * TICK as f64 * (1.0 / TICK as f64),
            );
        }
    }
}

/// Le cas le plus lourd seul (mille animaux, physique réaliste, collisions),
/// pour un profileur (`valgrind --tool=callgrind`).
#[test]
#[ignore = "pour un profileur"]
fn heaviest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let defs: Vec<Arc<_>> = ["southdown", "merinos-d-arles", "solognote", "bleu-du-maine"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = world();
    let input = Input {
        cursor: Some(Vec2::new(800.0, 500.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    let mut flock = Flock::new();
    flock.set_realistic(true);
    flock.set_collisions(true);
    for i in 0..1000 {
        let x = 20.0 + (i as f32 * 4.3) % 4200.0;
        flock.spawn(
            Arc::clone(&defs[i % defs.len()]),
            i as u64 * 7919 + 3,
            &world,
            Some(x),
        );
    }
    for _ in 0..(4.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
    }
    measured(&mut flock, &world, &input);
}

/// Mille animaux sans collisions (ce que fait l'app par défaut), en
/// physique réaliste, étalés sur deux écrans : une foule calme, pour un
/// profileur.
#[test]
#[ignore = "pour un profileur"]
fn calm_heaviest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let defs: Vec<Arc<_>> = ["southdown", "merinos-d-arles", "solognote", "bleu-du-maine"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = world();
    let input = Input {
        cursor: Some(Vec2::new(800.0, 500.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    let mut flock = Flock::new();
    flock.set_realistic(true);
    for i in 0..1000 {
        let x = 20.0 + (i as f32 * 4.3) % 4200.0;
        flock.spawn(
            Arc::clone(&defs[i % defs.len()]),
            i as u64 * 7919 + 3,
            &world,
            Some(x),
        );
    }
    for _ in 0..(10.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
    }
    measured(&mut flock, &world, &input);
}

/// Les pas mesurés, après la mise en route (le profileur ne regarde que là :
/// `--toggle-collect=*measured*`).
#[inline(never)]
fn measured(flock: &mut Flock, world: &World, input: &Input) {
    for _ in 0..(1.0 / TICK) as usize {
        flock.advance(TICK, world, input);
    }
}

/// Une foule serrée, comme l'essai de charge de la CI : un seul petit écran
/// (1024 × 768, sans fenêtres), le Mérinos d'Arles et l'agneau Southdown,
/// physique réaliste et collisions. Les animaux s'empilent : bien plus de
/// contacts qu'étalés.
#[test]
#[ignore = "essai de charge, à lancer à la main en --release"]
fn crowded() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let defs: Vec<Arc<_>> = ["merinos-d-arles", "southdown"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1024.0, 768.0),
        usable: Rect::new(0.0, 25.0, 1024.0, 670.0),
        scale: 1.0,
    }]);
    let input = Input {
        cursor: Some(Vec2::new(500.0, 400.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    for count in [20usize, 100, 500, 1000] {
        let mut flock = Flock::new();
        flock.set_realistic(true);
        flock.set_collisions(true);
        for i in 0..count {
            flock.spawn(
                Arc::clone(&defs[i % defs.len()]),
                i as u64 * 7919 + 3,
                &world,
                None,
            );
        }
        // Ils tombent, s'empilent, vivent un peu (leurs événements sont
        // pris à chaque pas, comme dans l'app).
        let drain = |flock: &mut Flock| {
            let mut bumps = 0;
            for pet in flock.pets_mut() {
                bumps += pet
                    .take_events()
                    .iter()
                    .filter(|e| matches!(e, mouflon_core::PetEvent::Bumped { .. }))
                    .count();
            }
            bumps
        };
        for _ in 0..(10.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
            drain(&mut flock);
        }
        let steps = (2.0 / TICK) as usize;
        let mut bumps = 0;
        let started = Instant::now();
        for _ in 0..steps {
            flock.advance(TICK, &world, &input);
            bumps += drain(&mut flock);
        }
        let per_step = started.elapsed().as_secs_f64() * 1000.0 / steps as f64;
        println!("  heurts : {:.1} par pas", bumps as f64 / steps as f64);
        let air = flock
            .pets()
            .iter()
            .filter(|p| p.support() == mouflon_core::Support::Air)
            .count();
        println!(
            "foule serrée : {count:5} animaux ({} dans le troupeau), {per_step:7.3} ms par pas, {air} en l'air",
            flock.pets().len()
        );
    }
}

/// La foule serrée de mille animaux seule, pour un profileur.
#[test]
#[ignore = "pour un profileur"]
fn crowded_heaviest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let defs: Vec<Arc<_>> = ["merinos-d-arles", "southdown"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1024.0, 768.0),
        usable: Rect::new(0.0, 25.0, 1024.0, 670.0),
        scale: 1.0,
    }]);
    let input = Input {
        cursor: Some(Vec2::new(500.0, 400.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    let mut flock = Flock::new();
    flock.set_realistic(true);
    flock.set_collisions(true);
    for i in 0..1000 {
        flock.spawn(
            Arc::clone(&defs[i % defs.len()]),
            i as u64 * 7919 + 3,
            &world,
            None,
        );
    }
    for _ in 0..(10.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
    }
    measured(&mut flock, &world, &input);
}
