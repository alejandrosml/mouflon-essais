//! Tout le troupeau quitte l'écran en bondissant.
//!
//! Au signal, chacun file vers le bord le plus proche par bonds d'agneau
//! (son `pronk`, allongé ; à défaut, son saut), et sort. Ceux des fenêtres
//! sautent d'abord au sol. Rien ne les retient en chemin : pendant la
//! sortie, chacun avance dans un monde à lui, le sol de son écran prolongé
//! très loin de chaque côté, sans fenêtres ni bords. Un animal qui ne sait
//! pas bondir (un animal en images sans saut, un animal desktopPet) dit au
//! revoir à sa façon et s'efface.
//!
//! Le troupeau est vide quand tout le monde est sorti ; au bout de
//! [`EXODUS_MAX`] secondes, ceux qui restent sont retirés.

use std::sync::Arc;

use crate::def::{Motion, Rules};
use crate::math::{Rect, Vec2};
use crate::world::{Screen, World};

use super::{Flock, Input, Pet, Support, TICK};

/// Vitesse d'un bond de sortie, en longueurs de son corps par seconde, et
/// ses bornes (pt/s) : un agneau ne traîne pas derrière un bélier.
const BOUND_SPEED: f32 = 5.0;
const BOUND_SPEED_RANGE: (f32, f32) = (230.0, 340.0);
/// Le sol de chaque écran se prolonge d'autant de chaque côté (points).
const RUNWAY: f32 = 20_000.0;
/// Au-delà, ceux qui ne sont pas sortis sont retirés (secondes).
pub const EXODUS_MAX: f32 = 7.0;

/// Il file vers un bord de l'écran.
#[derive(Clone, Copy, Debug)]
pub(super) struct Flight {
    dir: f32,
    speed: f32,
    behavior: usize,
    /// Le monde où il court (son rang dans [`Exodus::runways`]).
    runway: usize,
}

impl Flight {
    /// La vitesse du bond qu'il prend, vers l'avant (pt/s).
    pub(super) fn speed(&self) -> f32 {
        self.speed
    }

    pub(super) fn behavior(&self) -> usize {
        self.behavior
    }
}

/// La sortie du troupeau.
#[derive(Clone, Debug, Default)]
pub(super) struct Exodus {
    /// Un monde par écran : son sol, prolongé de chaque côté.
    runways: Vec<World>,
    time: f32,
}

impl Pet {
    /// Le comportement dont il fait ses bonds de sortie : son `pronk`, sinon
    /// un saut. Aucun pour un animal qui suit son propre scénario.
    fn bounding_behavior(&self) -> Option<usize> {
        if self.def.rules == Rules::DesktopPet || self.is_child() {
            return None;
        }
        let find = |wanted: fn(&Motion) -> bool| {
            self.def
                .behaviors
                .iter()
                .position(|b| wanted(&b.motion) && b.clip.is_none())
        };
        find(|m| matches!(m, Motion::Pronk { .. }))
            .or_else(|| find(|m| matches!(m, Motion::Jump { .. })))
    }

    /// Il file vers la gauche (`dir` négatif) ou la droite, par bonds, dans
    /// le monde de rang `runway`. `false` s'il ne sait pas bondir.
    fn flee(&mut self, dir: f32, runway: usize) -> bool {
        if self.leaving.is_some() || self.finished {
            return false;
        }
        let Some(behavior) = self.bounding_behavior() else {
            return false;
        };
        let p = self.def.physics;
        // Plus rien de ce qu'il faisait ne le retient.
        self.family.stop();
        self.clear_smooth();
        self.ghost = false;
        self.hold = None;
        self.takeoff = None;
        self.leap = None;
        self.walking_off = false;
        self.knocked = false;
        self.free_fall = false;
        self.grace = false;
        self.spin = 0.0;
        self.carry = Vec2::ZERO;
        if self.support == Support::Held {
            self.support = Support::Air;
            self.air_time = 0.0;
        }
        self.facing = if dir < 0.0 { -1.0 } else { 1.0 };
        self.flight = Some(Flight {
            dir: self.facing,
            speed: (p.width * BOUND_SPEED).clamp(BOUND_SPEED_RANGE.0, BOUND_SPEED_RANGE.1),
            behavior,
            runway,
        });
        // Posé, il part tout de suite ; en l'air, dès qu'il touche le sol
        // (voir `Pet::settle`).
        if matches!(self.support, Support::Ground { .. }) {
            self.enter(behavior);
        }
        true
    }

    /// En fuite : toujours tourné vers son bord, et de nouveau en train de
    /// bondir si autre chose l'a pris (une réaction d'atterrissage).
    pub(super) fn keep_fleeing(&mut self) {
        let Some(flight) = self.flight else {
            return;
        };
        self.facing = flight.dir;
        if matches!(self.support, Support::Ground { .. })
            && self.behavior != flight.behavior
            && self.takeoff.is_none()
        {
            self.enter(flight.behavior);
        }
    }

    /// Son corps, pattes comprises (repère du monde).
    fn body_rect(&self) -> Rect {
        let p = self.def.physics;
        Rect::new(
            self.pos.x - p.width * 0.5,
            self.pos.y - p.height,
            p.width,
            p.height,
        )
    }
}

/// Le sol de cet écran, prolongé très loin de chaque côté : un monde sans
/// fenêtres ni bords.
fn runway(screen: &Screen) -> World {
    let wide = |r: Rect| Rect::new(r.x - RUNWAY, r.y, r.w + 2.0 * RUNWAY, r.h);
    World::new(vec![Screen {
        frame: wide(screen.frame),
        usable: wide(screen.usable),
        scale: screen.scale,
    }])
}

/// L'écran de ce point (à défaut, le plus proche), et de quel côté en
/// sortir : vers son bord le plus proche derrière lequel il n'y a pas
/// d'autre écran ; s'il y en a derrière les deux, vers le plus proche.
fn way_out(world: &World, pos: Vec2) -> (usize, f32) {
    let screens = world.screens();
    let distance = |s: &Screen| {
        let r = s.room();
        let dx = (r.left() - pos.x).max(pos.x - r.right()).max(0.0);
        let dy = (r.top() - pos.y).max(pos.y - r.bottom()).max(0.0);
        dx + dy
    };
    let Some((rank, screen)) = screens
        .iter()
        .enumerate()
        .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
    else {
        return (0, 1.0);
    };
    let room = screen.room();
    let y = pos.y.clamp(room.top() + 1.0, room.bottom() - 1.0);
    let open = |x: f32| !world.on_screens(Vec2::new(x, y));
    let (left, right) = (open(room.left() - 2.0), open(room.right() + 2.0));
    let nearest = if pos.x - room.left() < room.right() - pos.x {
        -1.0
    } else {
        1.0
    };
    let dir = match (left, right) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => nearest,
    };
    (rank, dir)
}

impl Flock {
    /// Tout le troupeau quitte l'écran en bondissant (voir le module). Les
    /// surprises en cours s'arrêtent, celui qu'on tenait est lâché. Jusqu'à
    /// ce qu'il soit vide, [`Flock::advance`] ne fait plus que cela.
    pub fn bound_away(&mut self, world: &World, input: &Input) {
        if self.exodus.is_some() {
            return;
        }
        self.calm(world);
        self.still = None;
        let runways: Vec<World> = world.screens().iter().map(runway).collect();
        for pet in &mut self.pets {
            let (rank, dir) = way_out(world, pet.pos);
            if runways.is_empty() || !pet.flee(dir, rank) {
                if pet.support == Support::Held {
                    pet.release(world);
                }
                pet.leave(world, input);
            }
        }
        self.exodus = Some(Exodus { runways, time: 0.0 });
        self.generation += 1;
    }

    /// Les surprises en cours s'arrêtent : le vent tombe, une soucoupe en
    /// visite disparaît (celui qu'elle tenait retombe).
    pub fn calm(&mut self, world: &World) {
        self.gust = None;
        for saucer in std::mem::take(&mut self.saucers) {
            if saucer.holds()
                && let Some(pet) = self.pets.iter_mut().find(|p| p.id == saucer.target())
            {
                pet.release(world);
            }
        }
    }

    /// Ajoute un animal en l'air, à cet endroit précis (du sexe voulu s'il
    /// est donné) ; renvoie son identifiant.
    pub fn spawn_at(
        &mut self,
        def: Arc<crate::PetDef>,
        seed: u64,
        world: &World,
        pos: Vec2,
        sex: Option<crate::Sex>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let rng = crate::math::Rng::new(seed ^ (u64::from(id) << 32));
        let mut pet = Pet::at(id, def, rng, pos, world, sex);
        pet.take_temper(self.mood);
        self.pets.push(pet);
        self.generation += 1;
        id
    }

    /// Le troupeau est-il en train de sortir (voir [`Flock::bound_away`]) ?
    #[must_use]
    pub fn is_leaving_the_screen(&self) -> bool {
        self.exodus.is_some()
    }

    /// Le temps écoulé pendant la sortie : le même pas fixe que le reste.
    pub(super) fn advance_exodus(&mut self, elapsed: f32, world: &World, input: &Input) -> f32 {
        self.accumulator += elapsed.clamp(0.0, 0.1);
        while self.accumulator >= TICK {
            self.step_exodus(TICK, world, input);
            self.accumulator -= TICK;
        }
        self.accumulator / TICK
    }

    fn step_exodus(&mut self, dt: f32, world: &World, input: &Input) {
        let Some(exodus) = &mut self.exodus else {
            return;
        };
        exodus.time += dt;
        let late = exodus.time >= EXODUS_MAX;
        let quiet = Input {
            cursor: None,
            ..*input
        };
        for pet in &mut self.pets {
            match pet.flight {
                Some(flight) => {
                    let Some(runway) = exodus.runways.get(flight.runway) else {
                        pet.finished = true;
                        continue;
                    };
                    pet.keep_fleeing();
                    pet.step(dt, runway, &quiet);
                    // Sorti : plus rien de lui sur aucun écran.
                    let body = pet.body_rect();
                    let seen = world.screens().iter().any(|s| {
                        let f = s.frame;
                        body.right() > f.left() && body.left() < f.right()
                    });
                    if !seen {
                        pet.finished = true;
                    }
                }
                // Celui qui dit au revoir le fait là où il est.
                None => pet.step(dt, world, &quiet),
            }
            if late {
                pet.finished = true;
            }
        }
        let before = self.pets.len();
        self.pets.retain(|p| !p.finished);
        if self.pets.len() != before {
            self.generation += 1;
        }
        if self.pets.is_empty() {
            self.families = super::family::Families::default();
            self.exodus = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{LedgeOptions, WindowRect};

    fn bundled(name: &str) -> Arc<crate::PetDef> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../pets/{name}/pet.toml"));
        Arc::new(crate::format::load_pet(&path).expect("un animal du dépôt"))
    }

    /// Un animal des données de test : Pelote (desktopPet) ou Lainou (bandes
    /// d'images).
    fn fixture(name: &str) -> Arc<crate::PetDef> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/donnees/{name}/pet.toml"));
        Arc::new(crate::format::load_pet(&path).expect("un animal des données de test"))
    }

    fn screen(x: f32, y: f32, w: f32, h: f32) -> Screen {
        let frame = Rect::new(x, y, w, h);
        Screen {
            frame,
            usable: Rect::new(x, y + 25.0, w, h - 25.0 - 40.0),
            scale: 2.0,
        }
    }

    fn settled(flock: &mut Flock, world: &World, seconds: f32) {
        let steps = (seconds * 60.0) as usize;
        for _ in 0..steps {
            flock.advance(1.0 / 60.0, world, &Input::default());
        }
    }

    /// Avance jusqu'à ce que le troupeau soit vide ; renvoie le temps mis.
    fn until_empty(flock: &mut Flock, world: &World) -> f32 {
        let mut time = 0.0;
        while !flock.pets().is_empty() {
            flock.advance(1.0 / 60.0, world, &Input::default());
            time += 1.0 / 60.0;
            assert!(
                time < EXODUS_MAX + 1.0,
                "encore {} animaux",
                flock.pets().len()
            );
        }
        time
    }

    #[test]
    fn the_whole_flock_bounds_off_the_screen_each_to_its_nearest_edge() {
        for realistic in [false, true] {
            let world = World::new(vec![screen(0.0, 0.0, 1440.0, 900.0)]);
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let def = bundled("bleu-du-maine");
            let left = flock.spawn(Arc::clone(&def), 1, &world, Some(300.0));
            let right = flock.spawn(Arc::clone(&def), 2, &world, Some(1100.0));
            settled(&mut flock, &world, 4.0);
            flock.bound_away(&world, &Input::default());
            assert!(flock.is_leaving_the_screen());
            // Une seconde plus tard, chacun a pris son côté, par bonds.
            let mut airborne = false;
            for _ in 0..60 {
                flock.advance(1.0 / 60.0, &world, &Input::default());
                airborne |= flock.pets().iter().any(|p| p.support() == Support::Air);
            }
            let x = |id| {
                flock
                    .pets()
                    .iter()
                    .find(|p| p.id() == id)
                    .map(|p| p.position().x)
            };
            assert!(x(left).is_none_or(|x| x < 200.0), "{:?}", x(left));
            assert!(x(right).is_none_or(|x| x > 1200.0), "{:?}", x(right));
            assert!(airborne, "ils bondissent");
            let time = until_empty(&mut flock, &world);
            assert!(time < 4.0, "sortis en {time} s (réaliste : {realistic})");
            assert!(!flock.is_leaving_the_screen());
        }
    }

    #[test]
    fn a_sheep_on_a_window_jumps_down_and_leaves_with_the_others() {
        let mut world = World::new(vec![screen(0.0, 0.0, 1440.0, 900.0)]);
        world.set_windows(
            &[WindowRect {
                id: 1,
                frame: Rect::new(500.0, 400.0, 400.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let mut flock = Flock::new();
        flock.set_realistic(true);
        let id = flock.spawn(bundled("ouessant"), 3, &world, Some(700.0));
        settled(&mut flock, &world, 4.0);
        let pet = &flock.pets()[0];
        assert!(
            pet.position().y < 420.0,
            "sur la fenêtre : {:?}",
            pet.position()
        );
        flock.bound_away(&world, &Input::default());
        settled(&mut flock, &world, 1.5);
        // Au sol de l'écran, en route vers un bord.
        if let Some(pet) = flock.pets().iter().find(|p| p.id() == id) {
            assert!(pet.position().y > 700.0, "{:?}", pet.position());
        }
        until_empty(&mut flock, &world);
    }

    #[test]
    fn between_two_screens_it_leaves_by_the_open_side() {
        // Deux écrans côte à côte : celui de gauche ne s'ouvre qu'à gauche.
        let world = World::new(vec![
            screen(0.0, 0.0, 1000.0, 800.0),
            screen(1000.0, 0.0, 1000.0, 800.0),
        ]);
        let floor = 760.0;
        assert_eq!(way_out(&world, Vec2::new(900.0, floor)), (0, -1.0));
        assert_eq!(way_out(&world, Vec2::new(1100.0, floor)), (1, 1.0));
        // Seul : vers le bord le plus proche.
        let alone = World::new(vec![screen(0.0, 0.0, 1000.0, 800.0)]);
        assert_eq!(way_out(&alone, Vec2::new(900.0, floor)), (0, 1.0));
        assert_eq!(way_out(&alone, Vec2::new(100.0, floor)), (0, -1.0));
    }

    #[test]
    fn one_that_cannot_bound_says_goodbye_and_nobody_is_left() {
        let world = World::new(vec![screen(0.0, 0.0, 1440.0, 900.0)]);
        let mut flock = Flock::new();
        flock.spawn(fixture("pelote"), 5, &world, Some(700.0));
        flock.spawn(fixture("lainou"), 8, &world, Some(1000.0));
        flock.spawn(bundled("bleu-du-maine"), 6, &world, Some(400.0));
        settled(&mut flock, &world, 3.0);
        flock.bound_away(&world, &Input::default());
        assert!(flock.pets().iter().any(Pet::is_leaving));
        until_empty(&mut flock, &world);
    }

    #[test]
    fn a_held_sheep_is_let_go_and_leaves_too() {
        let world = World::new(vec![screen(0.0, 0.0, 1440.0, 900.0)]);
        let mut flock = Flock::new();
        flock.set_realistic(true);
        let id = flock.spawn(bundled("bleu-du-maine"), 7, &world, Some(700.0));
        settled(&mut flock, &world, 3.0);
        let at = flock.pets()[0].position();
        let pet = flock.get_mut(id).expect("là");
        pet.grab(at - Vec2::new(0.0, 20.0), &world);
        pet.drag_to(Vec2::new(700.0, 300.0));
        settled(&mut flock, &world, 0.5);
        flock.bound_away(&world, &Input::default());
        assert!(!flock.pets()[0].is_held());
        until_empty(&mut flock, &world);
    }
}
