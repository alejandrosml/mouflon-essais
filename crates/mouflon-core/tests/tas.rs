//! Un tas d'animaux, physique réaliste et collisions : rien ne saute à
//! l'écran. Un animal qu'on voit bouger va où sa vitesse le mène ; replacé
//! d'un coup par la simulation (séparé d'un autre, ramené à un bord), son
//! dessin y glisse. Et le tas se calme : il ne bouillonne pas sans fin.
//!
//! Comme dans l'essai du 2 novembre (31 sauts en trois minutes), mais avec
//! bien plus d'animaux : cent sur l'écran d'un MacBook, trois cents sur un
//! petit écran où ils ne tiennent pas tous au sol.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Flock, Input, Rect, Screen, Support, TICK, Vec2, World, load_pet};

/// Ce que le dessin d'un animal peut faire de plus que sa vitesse en un pas
/// (points) : glisser vers son corps replacé, se redresser.
const LEEWAY: f32 = 10.0;

struct Pile {
    /// Pas où le dessin d'un animal a bondi.
    jumps: Vec<String>,
    /// Plus grand écart entre un dessin et son corps (fois sa taille).
    worst_lag: f32,
    /// Part des animaux en l'air, en moyenne sur le dernier tiers.
    airborne: f32,
}

fn pile(width: f32, height: f32, count: usize, seconds: f32) -> Pile {
    // Lainou et Pelote, des données de test : en bandes d'images et desktopPet.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/donnees");
    let defs: Vec<Arc<_>> = ["lainou", "pelote"]
        .iter()
        .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
        .collect();
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, width, height),
        usable: Rect::new(0.0, 32.0, width, height - 98.0),
        scale: 2.0,
    }]);
    let input = Input {
        cursor: Some(Vec2::new(width * 0.5, height * 0.3)),
        local_hour: 15.0,
        ..Input::default()
    };
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
    let mut result = Pile {
        jumps: Vec::new(),
        worst_lag: 0.0,
        airborne: 0.0,
    };
    let steps = (seconds / TICK) as usize;
    let (mut air, mut counted) = (0usize, 0usize);
    for step in 0..steps {
        let before: HashMap<u32, (Vec2, Vec2)> = flock
            .pets()
            .iter()
            .map(|p| (p.id(), (p.view(1.0).ground, p.position())))
            .collect();
        flock.advance(TICK, &world, &input);
        for pet in flock.pets_mut() {
            pet.take_events();
        }
        if step >= steps * 2 / 3 {
            air += flock
                .pets()
                .iter()
                .filter(|p| p.support() == Support::Air)
                .count();
            counted += flock.pets().len();
        }
        for pet in flock.pets() {
            let drawn = pet.view(1.0).ground;
            if pet.support() == Support::Air {
                let size = pet.def().physics.width.max(pet.def().physics.height);
                result.worst_lag = result
                    .worst_lag
                    .max((drawn - pet.position()).length() / size);
            }
            let Some(&(was_drawn, was_at)) = before.get(&pet.id()) else {
                continue;
            };
            // Sorti de l'écran, un animal desktopPet réapparaît ailleurs :
            // pas un saut qu'on voit.
            if (pet.position() - was_at).length() > 300.0 {
                continue;
            }
            let moved = (drawn - was_drawn).length();
            let allowed = pet.velocity().length() * TICK * 3.0 + LEEWAY;
            if moved > allowed {
                result.jumps.push(format!(
                    "{:.2} s : n° {} ({}) {moved:.1} pt au lieu de {allowed:.1}",
                    step as f32 * TICK,
                    pet.id(),
                    pet.behavior_name()
                ));
            }
        }
    }
    result.airborne = air as f32 / counted.max(1) as f32;
    eprintln!(
        "{count} animaux : {:.0} % en l'air",
        result.airborne * 100.0
    );
    result
}

#[test]
fn a_hundred_pets_on_a_laptop_screen_never_jump() {
    let pile = pile(2056.0, 1329.0, 100, 15.0);
    assert!(pile.jumps.is_empty(), "{:#?}", pile.jumps);
    // Un dessin n'est jamais loin de son corps.
    assert!(
        pile.worst_lag < 1.5,
        "retard : {} fois sa taille",
        pile.worst_lag
    );
    // Le tas se calme : la plupart sont posés (avant le 2 novembre, deux
    // sur trois restaient en l'air à se bousculer).
    assert!(pile.airborne < 0.3, "{} en l'air", pile.airborne);
}

#[test]
fn three_hundred_pets_on_a_small_screen_never_jump() {
    // Ils ne tiennent pas tous au sol : ils passent les uns devant les
    // autres plutôt que de se bousculer sans fin.
    let pile = pile(1024.0, 768.0, 300, 8.0);
    assert!(pile.jumps.is_empty(), "{:#?}", pile.jumps);
    assert!(
        pile.worst_lag < 1.5,
        "retard : {} fois sa taille",
        pile.worst_lag
    );
    assert!(pile.airborne < 0.3, "{} en l'air", pile.airborne);
}
